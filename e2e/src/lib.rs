//! End-to-end tests (docs/specs/06-testing.md, layer L5): headless Chromium driven over WebDriver (`thirtyfour` and
//! `chromedriver`) against the web bundle served by a real daemon, which runs the pinned omp against the fake LLM in an
//! isolated environment (`omfp_testkit`). Run through `just test-e2e`, which builds the bundle first.
//!
//! Every selector is a `data-testid` (TST-031). Waiting polls the page until a condition holds or [`TIMEOUT`] passes;
//! nothing waits for a fixed time.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;
use fake_llm::{FakeLlm, Script, Step};
use omfp_daemon::config::{Cli, Config};
use omfp_daemon::{DaemonHandle, Ports, start_with};
use omfp_testkit::{
    DenyProxy, FAKE_MODEL, KNOWN_DISCOVERY_HOSTS, OmpEnv, omp_bin, require_pinned_omp,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use thirtyfour::ChromeCapabilities;
use thirtyfour::prelude::*;

/// Longest wait for any condition (omp startup and a model turn included).
pub const TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(50);

/// The web bundle built by `just build-web`, or `OMFP_WEB_BUNDLE`.
#[must_use]
pub fn web_bundle() -> PathBuf {
    std::env::var_os("OMFP_WEB_BUNDLE").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/dx/omfp/release/web/public"),
        PathBuf::from,
    )
}

/// omp settings added to the isolated environment, e.g. `tools.approvalMode`.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// omp `tools.approvalMode` (`always-ask`, …); omp's default (`yolo`) when `None`.
    pub approval_mode: Option<&'static str>,
}

/// `chromedriver` on a free port; killed on drop.
struct ChromeDriver {
    child: Child,
    url: String,
}

impl ChromeDriver {
    fn start() -> anyhow::Result<Self> {
        let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        let mut child = Command::new("chromedriver")
            .arg(format!("--port={port}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("cannot start chromedriver (run inside `nix develop`)")?;
        let stdout = child.stdout.take().context("chromedriver stdout")?;
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next() {
                Some(Ok(line)) if line.contains("started successfully") => break,
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error.into()),
                None => bail!("chromedriver exited before it was ready"),
            }
        }
        // Keep draining its output so it never blocks on a full pipe.
        std::thread::spawn(move || lines.for_each(drop));
        Ok(Self {
            child,
            url: format!("http://127.0.0.1:{port}"),
        })
    }
}

impl Drop for ChromeDriver {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One journey: fake LLM, isolated omp, daemon serving the web bundle, and a headless browser on its URL.
pub struct Journey {
    pub fake: FakeLlm,
    proxy: DenyProxy,
    pub env: OmpEnv,
    data: TempDir,
    _profile: TempDir,
    daemon: Option<DaemonHandle>,
    pub driver: WebDriver,
    _chromedriver: ChromeDriver,
    started: Instant,
}

impl std::fmt::Debug for Journey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Journey")
            .field("data", &self.data.path())
            .finish_non_exhaustive()
    }
}

impl Journey {
    /// Starts everything with the model replying `steps` in order, then opens the UI with the token in the URL.
    pub async fn start(steps: Vec<Step>, settings: Settings) -> anyhow::Result<Self> {
        let started = Instant::now();
        require_pinned_omp();
        let bundle = web_bundle();
        if !bundle.join("index.html").is_file() {
            bail!(
                "no web bundle at {}: run `just build-web` (or `just test-e2e`)",
                bundle.display()
            );
        }
        let fake = FakeLlm::start(Script::new(FAKE_MODEL, steps)).await?;
        let proxy = DenyProxy::start().await?;
        let env = OmpEnv::new(&fake.base_url(), Some(&proxy))?;
        if let Some(mode) = settings.approval_mode {
            let config = env.agent_dir().join("config.yml");
            let text = std::fs::read_to_string(&config).unwrap_or_default();
            let text = format!("{text}tools:\n  approvalMode: {mode}\n");
            std::fs::write(&config, text)?;
        }
        let data = tempfile::tempdir()?;
        let daemon = start_daemon(&env, data.path(), &bundle).await?;
        let profile = tempfile::tempdir()?;
        let chromedriver = ChromeDriver::start()?;
        let driver = WebDriver::new(&chromedriver.url, chrome(profile.path())?).await?;
        driver.goto(daemon.url()).await?;
        Ok(Self {
            fake,
            proxy,
            env,
            data,
            _profile: profile,
            daemon: Some(daemon),
            driver,
            _chromedriver: chromedriver,
            started,
        })
    }

    /// The daemon's data directory.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        self.data.path()
    }

    /// The running daemon.
    ///
    /// # Panics
    /// After [`Journey::finish`].
    #[must_use]
    pub fn daemon(&self) -> &DaemonHandle {
        self.daemon.as_ref().expect("the daemon runs until finish")
    }

    /// Polls `probe` until it returns `Some`, failing after [`TIMEOUT`] with `what` and the page's state.
    pub async fn until<T>(
        &self,
        what: &str,
        mut probe: impl AsyncFnMut(&Self) -> anyhow::Result<Option<T>>,
    ) -> anyhow::Result<T> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(value) = probe(self).await? {
                return Ok(value);
            }
            if Instant::now() > deadline {
                let page = self.text("body").await.unwrap_or_default();
                bail!("timed out waiting for {what}; the page shows:\n{page}");
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Evaluates `script` (a function body getting `arguments`) in the page.
    pub async fn eval(&self, script: &str, args: Vec<Value>) -> anyhow::Result<Value> {
        Ok(self.driver.execute(script, args).await?.json().clone())
    }

    /// The visible text of every element matching `css`, in document order.
    pub async fn texts(&self, css: &str) -> anyhow::Result<Vec<String>> {
        let value = self
            .eval(
                "return [...document.querySelectorAll(arguments[0])].map(e => e.innerText.trim());",
                vec![json!(css)],
            )
            .await?;
        Ok(serde_json::from_value(value)?)
    }

    /// The visible text of the first element matching `css` (empty when there is none).
    pub async fn text(&self, css: &str) -> anyhow::Result<String> {
        Ok(self
            .texts(css)
            .await?
            .into_iter()
            .next()
            .unwrap_or_default())
    }

    /// Attribute `name` of every element matching `css`.
    pub async fn attrs(&self, css: &str, name: &str) -> anyhow::Result<Vec<Option<String>>> {
        let value = self
            .eval(
                "return [...document.querySelectorAll(arguments[0])].map(e => e.getAttribute(arguments[1]));",
                vec![json!(css), json!(name)],
            )
            .await?;
        Ok(serde_json::from_value(value)?)
    }

    /// The first element matching `css`, once it is displayed.
    pub async fn element(&self, css: &str) -> anyhow::Result<WebElement> {
        Ok(self
            .driver
            .query(By::Css(css))
            .wait(TIMEOUT, POLL)
            .and_displayed()
            .first()
            .await?)
    }

    /// Clicks the first displayed element matching `css`.
    pub async fn click(&self, css: &str) -> anyhow::Result<()> {
        self.element(css).await?.click().await?;
        Ok(())
    }

    /// Types `text` into the first displayed element matching `css`.
    pub async fn type_into(&self, css: &str, text: &str) -> anyhow::Result<()> {
        self.element(css).await?.send_keys(text).await?;
        Ok(())
    }

    /// Chooses the option with value `value` in the `<select>` matching `css`.
    pub async fn select(&self, css: &str, value: &str) -> anyhow::Result<()> {
        let select = self.element(css).await?;
        select
            .find(By::Css(format!("option[value=\"{value}\"]")))
            .await?
            .click()
            .await?;
        Ok(())
    }

    /// Waits until the page is connected and shows its shell.
    pub async fn connected(&self) -> anyhow::Result<()> {
        self.element("[data-testid=shell]").await?;
        self.until("the connection", async |page| {
            Ok(
                (page.texts("[data-testid=connection-banner]").await? == Vec::<String>::new())
                    .then_some(()),
            )
        })
        .await
    }

    /// Creates a thread through the new-thread dialog (no workspace) and waits until it is open.
    pub async fn create_thread(&self, title: &str) -> anyhow::Result<String> {
        self.click("[data-testid=new-thread]").await?;
        self.type_into("[data-testid=new-thread-title]", title)
            .await?;
        self.click("[data-testid=create-thread]").await?;
        self.opened(title).await
    }

    /// Waits until the thread titled `title` is open and its composer ready; returns its id.
    pub async fn opened(&self, title: &str) -> anyhow::Result<String> {
        self.until(&format!("thread {title:?} to open"), async |page| {
            let header = page.text("[data-testid=thread-title]").await?;
            let composer = page.texts("[data-testid=composer-input]").await?;
            if header != title || composer.is_empty() {
                return Ok(None);
            }
            let id = page
                .attrs(
                    "[data-testid=thread-row][aria-current=true]",
                    "data-thread-id",
                )
                .await?;
            Ok(id.into_iter().next().flatten())
        })
        .await
    }

    /// Sends `text` from the composer with Enter.
    pub async fn send(&self, text: &str) -> anyhow::Result<()> {
        let input = self.element("[data-testid=composer-input]").await?;
        input.send_keys(text).await?;
        input.send_keys(Key::Enter).await?;
        Ok(())
    }

    /// Waits until the assistant's messages include one containing `needle`.
    pub async fn reply(&self, needle: &str) -> anyhow::Result<()> {
        self.until(&format!("a reply containing {needle:?}"), async |page| {
            let replies = page
                .texts("[data-testid=message][data-role=assistant]")
                .await?;
            Ok(replies
                .iter()
                .any(|reply| reply.contains(needle))
                .then_some(()))
        })
        .await
    }

    /// Waits until the thread `thread_id` shows `status` in the thread list (`idle`, `running`, `awaiting`, …).
    pub async fn status(&self, thread_id: &str, status: &str) -> anyhow::Result<()> {
        let css = format!("[data-testid=thread-row][data-thread-id=\"{thread_id}\"]");
        self.until(
            &format!("thread {thread_id} to be {status}"),
            async |page| {
                let statuses = page.attrs(&css, "data-status").await?;
                Ok((statuses.first().cloned().flatten().as_deref() == Some(status)).then_some(()))
            },
        )
        .await
    }

    /// Saves a screenshot of the window as `omfp-<name>.png` in `OMFP_SCREENSHOT_DIR` (default `/tmp`; the dev shell's
    /// `TMPDIR` goes away with the shell) and returns its path.
    pub async fn screenshot(&self, name: &str) -> anyhow::Result<PathBuf> {
        let dir = std::env::var_os("OMFP_SCREENSHOT_DIR")
            .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        let path = dir.join(format!("omfp-{name}.png"));
        self.driver.screenshot(&path).await?;
        eprintln!("screenshot: {}", path.display());
        Ok(path)
    }

    /// Fails when the first element matching `css` scrolls sideways (`scrollWidth > clientWidth`).
    pub async fn fits(&self, css: &str) -> anyhow::Result<()> {
        let value = self
            .eval(
                "const e = document.querySelector(arguments[0]); return [e.scrollWidth, e.clientWidth];",
                vec![json!(css)],
            )
            .await?;
        let (scroll, client): (u64, u64) = serde_json::from_value(value)?;
        if scroll > client {
            bail!("{css} overflows sideways: scrollWidth {scroll} > clientWidth {client}");
        }
        Ok(())
    }

    /// Stops the browser and the daemon, then checks that the page showed no error, nothing reached the network and
    /// the model got only scripted requests. Prints how long the journey took.
    pub async fn finish(mut self, name: &str) -> anyhow::Result<()> {
        let errors = self.texts("[data-testid=error]").await?;
        if !errors.is_empty() {
            bail!("the page shows errors: {errors:#?}");
        }
        self.driver.clone().quit().await?;
        if let Some(daemon) = self.daemon.take() {
            daemon.shutdown().await?;
        }
        let stray = self.proxy.unexpected_attempts(KNOWN_DISCOVERY_HOSTS);
        if !stray.is_empty() {
            bail!("stray network access: {stray:#?}");
        }
        if !self.fake.failures().is_empty() {
            bail!("unexpected model requests: {:#?}", self.fake.failures());
        }
        eprintln!("{name}: {:.1} s", self.started.elapsed().as_secs_f64());
        Ok(())
    }
}

async fn start_daemon(env: &OmpEnv, data: &Path, bundle: &Path) -> anyhow::Result<DaemonHandle> {
    let omp = omp_bin();
    let path = |path: &Path| path.to_string_lossy().into_owned();
    let cli = Cli::try_parse_from([
        "daemon".to_owned(),
        "--bind".to_owned(),
        "127.0.0.1:0".to_owned(),
        "--data-dir".to_owned(),
        path(data),
        "--workspaces-root".to_owned(),
        path(&data.join("workspaces")),
        "--omp-bin".to_owned(),
        omp,
        "--static-dir".to_owned(),
        path(bundle),
    ])?;
    let config = Config::from_cli(cli, None, None).map_err(anyhow::Error::msg)?;
    let mut ports = Ports::resolve(&config)?;
    ports.omp_env = env.env_overrides();
    start_with(config, ports).await
}

/// Headless Chromium with its own profile; every non-loopback request goes to a closed proxy port.
fn chrome(profile: &Path) -> anyhow::Result<ChromeCapabilities> {
    let mut caps = DesiredCapabilities::chrome();
    if let Some(binary) = std::env::var_os("CHROME_BIN") {
        caps.set_binary(&binary.to_string_lossy())?;
    }
    for arg in [
        "--headless=new",
        "--no-sandbox",
        "--disable-gpu",
        "--disable-dev-shm-usage",
        "--window-size=1400,1000",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "--disable-default-apps",
        "--proxy-server=http://127.0.0.1:9",
    ] {
        caps.add_arg(arg)?;
    }
    caps.add_arg(&format!("--user-data-dir={}", profile.display()))?;
    Ok(caps)
}
