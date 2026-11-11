//! `omfp desktop` (M7, spike S-04): one Tauri window per process (D-16) on the local daemon of the data directory,
//! found with [`omfp_daemon::discover`] or started in the background (ARC-003), or with `--connect` on another
//! daemon. The window loads the daemon's own UI over http: no bundled assets, no Tauri IPC, no capabilities.

use std::cell::Cell;
use std::ffi::OsString;
use std::fs::{File, TryLockError};
use std::net::{SocketAddr, TcpListener};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use clap::Parser as _;
use gio::glib::{ToVariant as _, Variant};
use omfp_daemon::config::{Cli, Config, default_config_file};
use omfp_daemon::theme::default_theme_file;
use omfp_daemon::watch::{self, WatchedFile};
use omfp_protocol::Theme;
use omfp_protocol::method::ThemeSet;
use tauri::webview::{PermissionKind, PermissionResponse};
use tauri::{
    Manager as _, Theme as WindowTheme, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::remote::{Connection, Target, default_data_dir, reachable, read_token, websocket_url};

/// `omfp desktop`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct Args {
    /// Opens the window on this daemon (e.g. `https://host:7420`) instead of the local one; the theme file of this
    /// machine is then pushed to it (UI-091, needs --token).
    #[arg(long, value_name = "URL")]
    pub connect: Option<String>,
    /// Access token (DMN-003) [default: the data directory's token file].
    #[arg(long)]
    pub token: Option<String>,
    /// Data directory of the local daemon; also holds the window's web storage (`webview/`) [default: the daemon's,
    /// `$XDG_DATA_HOME/omfp`, else `~/.local/share/omfp`].
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Selects this thread in the new window (the tray's waiting threads, UI-218).
    #[arg(long, value_name = "ID")]
    pub thread: Option<String>,
    /// Arguments for `omfp daemon` when the local daemon has to be started, e.g. `-- --static-dir ui/dist`.
    #[arg(last = true, value_name = "DAEMON ARGS")]
    pub daemon_args: Vec<String>,
}

/// The process environment the defaults depend on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub xdg_data_home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Env {
    fn current() -> Self {
        let var = |name| std::env::var_os(name).map(PathBuf::from);
        Self {
            xdg_data_home: var("XDG_DATA_HOME"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            home: var("HOME"),
        }
    }
}

/// `OMFP_SMOKE` (TST-033): the window closes itself once the shell is connected and rendered (exit 0), or the
/// process exits 1 after [`SMOKE_DEADLINE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Smoke {
    /// `OMFP_SMOKE=1`: on an in-process daemon (DMN-004) that stops with the window, so nothing outlives the test.
    InProcess,
    /// `OMFP_SMOKE=local`: on the local daemon, found or started in the background as without the variable.
    Local,
}

impl Smoke {
    /// Reads `OMFP_SMOKE`; unset means a normal window.
    pub fn parse(value: Option<&str>) -> anyhow::Result<Option<Self>> {
        match value {
            None => Ok(None),
            Some("1") => Ok(Some(Self::InProcess)),
            Some("local") => Ok(Some(Self::Local)),
            Some(other) => bail!("OMFP_SMOKE must be `1` or `local`, not `{other}`"),
        }
    }
}

/// How long the smoke check waits for the shell.
pub const SMOKE_DEADLINE: Duration = Duration::from_secs(60);
/// How long a started daemon gets to write `daemon.json`.
const START_DEADLINE: Duration = Duration::from_secs(30);

/// Runs `omfp desktop`; returns the process exit code.
pub fn run(args: &Args) -> anyhow::Result<i32> {
    let smoke = Smoke::parse(std::env::var("OMFP_SMOKE").ok().as_deref())?;
    let env = Env::current();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let mut in_process = None;
    let (window_url, browser_url, data_dir) = if let Some(connect) = &args.connect {
        if !args.daemon_args.is_empty() {
            bail!("daemon arguments after `--` need the local daemon; drop them or --connect");
        }
        let base = connect_base(connect)?;
        let data_dir = args
            .data_dir
            .clone()
            .or_else(|| default_data_dir(env.xdg_data_home.clone(), env.home.clone()))
            .context("cannot determine the data directory: pass --data-dir, or set XDG_DATA_HOME or HOME")?;
        match theme_push(&base, args.token.as_deref(), &env) {
            ThemePush::Push { target, file } => {
                runtime.spawn(push_theme(target, file, watch::POLL));
            }
            ThemePush::NeedsToken => eprintln!(
                "omfp desktop: the theme file of this machine is not pushed to {base}: pass --token"
            ),
            ThemePush::NoThemeFile => {}
        }
        let url = window_url(&base, args.token.as_deref(), false, None);
        (
            window_url(&base, args.token.as_deref(), false, args.thread.as_deref()),
            url,
            data_dir,
        )
    } else {
        let plan = LocalDaemon::plan(&args.data_dir, &args.daemon_args, &env)?;
        let (addr, token) = if smoke == Some(Smoke::InProcess) {
            let handle = runtime.block_on(omfp_daemon::start(plan.in_process_config()?))?;
            let started = (handle.addr(), handle.token().expose().to_owned());
            in_process = Some(handle);
            started
        } else {
            let addr = plan.find_or_start()?;
            let token = match &args.token {
                Some(token) => token.clone(),
                None => read_token(&plan.data_dir)?,
            };
            (addr, token)
        };
        let base = local_base(addr)?;
        (
            window_url(&base, Some(&token), true, args.thread.as_deref()),
            window_url(&base, Some(&token), false, None),
            plan.data_dir,
        )
    };
    // UI-004: a browser can attach to the same daemon.
    println!("omfp desktop: {browser_url}");
    let code = open_window(window_url, &data_dir.join("webview"), smoke)?;
    if let Some(handle) = in_process {
        runtime.block_on(handle.shutdown())?;
    }
    Ok(code)
}

/// The base URL of `--connect`: `http(s)://host[:port][/path]`; a URL without a scheme is `http://`.
pub fn connect_base(connect: &str) -> anyhow::Result<Url> {
    let with_scheme = if connect.contains("://") {
        connect.to_owned()
    } else {
        format!("http://{connect}")
    };
    let mut url =
        Url::parse(&with_scheme).with_context(|| format!("--connect: invalid URL `{connect}`"))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("--connect: the daemon's UI is an http:// or https:// URL, not `{connect}`");
    }
    url.set_fragment(None);
    url.set_query(None);
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// The base URL of the local daemon at `addr` (a daemon on every interface is reached on loopback).
fn local_base(addr: SocketAddr) -> anyhow::Result<Url> {
    Ok(Url::parse(&format!("http://{}/", reachable(addr)))?)
}

/// The URL a window opens: the token in the fragment (never sent to the server, PRT-002), `local=1` when the window
/// runs on the daemon's machine (the app then uses omp's `live_start` for live voice, S-09), and `thread=<id>` to
/// select a thread (UI-218).
#[must_use]
pub fn window_url(base: &Url, token: Option<&str>, local: bool, thread: Option<&str>) -> Url {
    let parameters: Vec<String> = token
        .map(|token| format!("token={token}"))
        .into_iter()
        .chain(local.then(|| "local=1".to_owned()))
        .chain(thread.map(|thread| format!("thread={}", fragment_value(thread))))
        .collect();
    let mut url = base.clone();
    url.set_fragment(
        (!parameters.is_empty())
            .then(|| parameters.join("&"))
            .as_deref(),
    );
    url
}

/// A fragment parameter's value with `%`, `&`, `#` and `=` escaped (thread ids are plain, but they come from argv).
fn fragment_value(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('&', "%26")
        .replace('#', "%23")
        .replace('=', "%3D")
}

/// Whether and what to push of the theme file of this machine (UI-091). A local daemon follows that file itself
/// (DMN-075); a daemon reached with `--connect` cannot see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemePush {
    /// Follow `file` and send it to `target` with `theme.set`.
    Push { target: Target, file: PathBuf },
    /// `theme.set` needs the daemon's token.
    NeedsToken,
    /// Neither `XDG_CONFIG_HOME` nor `HOME` is set.
    NoThemeFile,
}

/// The theme push of a window opened with `--connect` on `base`.
#[must_use]
pub fn theme_push(base: &Url, token: Option<&str>, env: &Env) -> ThemePush {
    let Some(token) = token else {
        return ThemePush::NeedsToken;
    };
    match default_theme_file(env.xdg_config_home.clone(), env.home.clone()) {
        Some(file) => ThemePush::Push {
            target: Target {
                url: websocket_url(base.as_str()),
                token: token.to_owned(),
            },
            file,
        },
        None => ThemePush::NoThemeFile,
    }
}

/// Follows `file` (checked every `period` once it settled, DMN-091) and sends every change to `target`'s
/// `theme.set`, a removed file as no theme; runs until the process ends. Failures are reported and the next change
/// is tried again.
pub async fn push_theme(target: Target, file: PathBuf, period: Duration) {
    let mut watched = WatchedFile::new(&file);
    let first = watch::first_read(&mut watched);
    let (sender, mut changes) = tokio::sync::mpsc::unbounded_channel();
    if let Some(change) = first {
        // The receiver is alive: it is dropped only when this function returns.
        let _ = sender.send(change);
    }
    let _watcher = watch::watch(watched, period, sender.clone());
    while let Some(css) = changes.recv().await {
        if let Err(error) = set_theme(&target, css).await {
            eprintln!(
                "omfp desktop: cannot push the theme {} to {}: {error:#}",
                file.display(),
                target.url
            );
        }
    }
}

async fn set_theme(target: &Target, css: Option<String>) -> anyhow::Result<()> {
    let mut connection = Connection::open(target).await?;
    connection.call::<ThemeSet>(Theme { css }).await?;
    connection.close().await
}

/// How to find or start the daemon of the data directory (ARC-003): the daemon's own command line and config file
/// decide its data directory and bind address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDaemon {
    /// Absolute data directory.
    pub data_dir: PathBuf,
    /// The configured or default address (DMN-002).
    pub bind: SocketAddr,
    /// `--bind` was passed after `--`: never replaced.
    pub explicit_bind: bool,
    /// Arguments for `omfp daemon` (without the subcommand).
    pub daemon_args: Vec<OsString>,
}

impl LocalDaemon {
    /// Resolves the daemon's settings like `omfp daemon` would; `data_dir` is `omfp desktop --data-dir`.
    pub fn plan(
        data_dir: &Option<PathBuf>,
        daemon_args: &[String],
        env: &Env,
    ) -> anyhow::Result<Self> {
        let mut cli = Cli::try_parse_from(
            std::iter::once("daemon").chain(daemon_args.iter().map(String::as_str)),
        )
        .map_err(|error| anyhow::anyhow!("daemon arguments after `--`: {error}"))?;
        if data_dir.is_some() && cli.data_dir.is_some() {
            bail!("--data-dir is given twice: pass it before `--` only");
        }
        let mut args: Vec<OsString> = Vec::new();
        if let Some(dir) = data_dir {
            cli.data_dir = Some(dir.clone());
        }
        let explicit_bind = cli.bind.is_some();
        cli.config = cli
            .config
            .or_else(|| default_config_file(env.xdg_config_home.clone(), env.home.clone()));
        let config = Config::from_cli(cli, env.xdg_data_home.clone(), env.home.clone())
            .map_err(anyhow::Error::msg)?;
        let data_dir = std::path::absolute(&config.data_dir)?;
        args.extend(["--data-dir".into(), data_dir.clone().into_os_string()]);
        args.extend(
            daemon_args
                .iter()
                .scan(false, |skip, arg| {
                    // The data directory goes first, resolved: drop its `--data-dir` from the passed arguments.
                    let keep = !*skip && arg != "--data-dir" && !arg.starts_with("--data-dir=");
                    *skip = arg == "--data-dir";
                    Some(keep.then(|| OsString::from(arg)))
                })
                .flatten(),
        );
        Ok(Self {
            data_dir,
            bind: config.bind,
            explicit_bind,
            daemon_args: args,
        })
    }

    /// The config of the smoke test's in-process daemon (TST-033): on a random port unless `--bind` was passed.
    fn in_process_config(&self) -> anyhow::Result<Config> {
        let mut cli = Cli::try_parse_from(
            std::iter::once(OsString::from("daemon")).chain(self.daemon_args.iter().cloned()),
        )?;
        if !self.explicit_bind {
            cli.bind = Some(SocketAddr::new(self.bind.ip(), 0));
        }
        let env = Env::current();
        Config::from_cli(cli, env.xdg_data_home, env.home).map_err(anyhow::Error::msg)
    }

    /// The command line of the background daemon: in the tray (DMN-128).
    #[must_use]
    pub fn command_line(&self, bind: Option<SocketAddr>) -> Vec<OsString> {
        let mut line = vec![OsString::from("daemon"), OsString::from("--tray")];
        if let Some(bind) = bind {
            line.extend(["--bind".into(), bind.to_string().into()]);
        }
        line.extend(self.daemon_args.iter().cloned());
        line
    }

    /// The address of the running daemon, after starting one in the background when none runs: on the configured
    /// address first, so the window's origin (and its `localStorage`) stays the same across restarts, else on a
    /// random port (ARC-003).
    pub fn find_or_start(&self) -> anyhow::Result<SocketAddr> {
        if let Some(info) = omfp_daemon::discover(&self.data_dir)? {
            eprintln!(
                "omfp desktop: found the daemon (pid {}) on {}",
                info.pid, info.addr
            );
            return Ok(info.addr);
        }
        let mut bind = None;
        loop {
            match self.start(bind)? {
                Started::Ready(addr) => return Ok(addr),
                Started::Exited(status) => {
                    if bind.is_none()
                        && let Some(fallback) =
                            fallback_bind(self.bind, self.explicit_bind, port_in_use(self.bind))
                    {
                        eprintln!(
                            "omfp desktop: {} is taken, starting the daemon on {fallback}",
                            self.bind
                        );
                        bind = Some(fallback);
                        continue;
                    }
                    bail!(
                        "omfp daemon exited ({status}), see {}",
                        self.log().display()
                    );
                }
            }
        }
    }

    fn log(&self) -> PathBuf {
        self.data_dir.join("daemon.log")
    }

    /// Starts `omfp daemon` detached (D-16): its own process group (no terminal signals), stdin `/dev/null`, output
    /// appended to `<data-dir>/daemon.log`, reaped by a thread of this process while it lives. Waits until a daemon
    /// runs for the data directory (this one, or another that won a start race).
    fn start(&self, bind: Option<SocketAddr>) -> anyhow::Result<Started> {
        std::fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("cannot create {}", self.data_dir.display()))?;
        let path = self.log();
        let log = File::options()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        let mut child = Command::new(std::env::current_exe()?)
            .args(self.command_line(bind))
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .process_group(0)
            .spawn()
            .context("cannot start omfp daemon")?;
        eprintln!(
            "omfp desktop: started the daemon (pid {}), log in {}",
            child.id(),
            path.display()
        );
        let started = self.wait_ready(&mut child);
        std::thread::spawn(move || child.wait());
        started
    }

    fn wait_ready(&self, child: &mut Child) -> anyhow::Result<Started> {
        let deadline = Instant::now() + START_DEADLINE;
        loop {
            if let Some(info) = omfp_daemon::discover(&self.data_dir)? {
                return Ok(Started::Ready(info.addr));
            }
            // A child that lost a start race exits while the winner, which holds the lock, is still starting.
            if let Some(status) = child.try_wait()?
                && !lock_held(&self.data_dir)?
            {
                return Ok(Started::Exited(status));
            }
            if Instant::now() > deadline {
                bail!(
                    "omfp daemon did not start in {} s, see {}",
                    START_DEADLINE.as_secs(),
                    self.log().display()
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

enum Started {
    Ready(SocketAddr),
    Exited(ExitStatus),
}

/// Where to retry a background daemon that exited without starting (ARC-003): on a random port of the same
/// interface when the configured port is taken; never when `--bind` was given or the port was already random.
#[must_use]
pub fn fallback_bind(bind: SocketAddr, explicit: bool, in_use: bool) -> Option<SocketAddr> {
    (!explicit && bind.port() != 0 && in_use).then(|| SocketAddr::new(bind.ip(), 0))
}

fn port_in_use(bind: SocketAddr) -> bool {
    TcpListener::bind(bind).is_err_and(|error| error.kind() == std::io::ErrorKind::AddrInUse)
}

/// Whether a daemon holds the data directory's lock (DMN-007).
fn lock_held(data_dir: &Path) -> std::io::Result<bool> {
    let lock = match File::open(data_dir.join("daemon.lock")) {
        Ok(lock) => lock,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    match lock.try_lock_shared() {
        Ok(()) => Ok(false),
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// The page state the smoke check polls: the shell is connected and shows a view (as e2e's `connected()`), and the
/// color scheme the page sees.
const PROBE: &str = r#"(() => {
  const q = (s) => document.querySelector(s);
  return {
    ready: !!q("[data-testid=view]") && !q("[data-testid=connection-banner]"),
    scheme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light",
    href: location.href,
    rtcPeerConnection: typeof RTCPeerConnection,
    notification: typeof Notification === "function" ? Notification.permission : "undefined",
  };
})()"#;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME: &str = "color-scheme";

/// The window theme for the settings portal's `org.freedesktop.appearance color-scheme` (UI-195): 1 prefers dark,
/// 2 prefers light, 0 (no preference) and unknown values leave the toolkit's default.
#[must_use]
pub fn portal_theme(color_scheme: u32) -> Option<WindowTheme> {
    match color_scheme {
        1 => Some(WindowTheme::Dark),
        2 => Some(WindowTheme::Light),
        _ => None,
    }
}

/// The `u` of a portal value however it is wrapped: `ReadOne` answers `(v)`, the older `Read` `(v)` holding a `v`,
/// `SettingChanged` carries a `v`.
#[must_use]
pub fn portal_value(value: &Variant) -> Option<u32> {
    if let Some(number) = value.get::<u32>() {
        return Some(number);
    }
    if let Some(inner) = value.as_variant() {
        return portal_value(&inner);
    }
    (value.is_container() && value.n_children() == 1)
        .then(|| portal_value(&value.child_value(0)))
        .flatten()
}

/// Follows the session's color scheme through the settings portal (UI-195): `WebKitGTK` 4.1 (GTK3) never reads it, so
/// the page's `prefers-color-scheme` stayed light under a dark desktop. The window's theme (GTK's
/// `gtk-application-prefer-dark-theme`, which `WebKitGTK` maps to `prefers-color-scheme`) is set from the portal's value
/// at startup and on every `SettingChanged`. Without a session bus or portal nothing changes. Runs on the GTK main
/// thread, whose main loop delivers the replies and signals; the subscription lasts as long as the returned value.
fn follow_color_scheme(window: &WebviewWindow) -> Option<ColorScheme> {
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()?;
    // Subscribed before reading, so no change between the read and the subscription is lost; a change that arrives
    // before the read's answer wins over it.
    let changed = Rc::new(Cell::new(false));
    let subscription = connection.signal_subscribe(
        Some(PORTAL),
        Some(PORTAL_SETTINGS),
        Some("SettingChanged"),
        Some(PORTAL_PATH),
        Some(APPEARANCE),
        gio::DBusSignalFlags::NONE,
        {
            let window = window.clone();
            let changed = Rc::clone(&changed);
            move |_, _, _, _, _, parameters| {
                if let Some((namespace, key, value)) = parameters.get::<(String, String, Variant)>()
                    && namespace == APPEARANCE
                    && key == COLOR_SCHEME
                {
                    changed.set(true);
                    let _ = window.set_theme(portal_value(&value).and_then(portal_theme));
                }
            }
        },
    );
    read_color_scheme(&connection, "ReadOne", {
        let window = window.clone();
        let connection = connection.clone();
        move |value| match value {
            Some(value) => apply_read(&window, &changed, &value),
            // Portals before version 2 only have `Read`.
            None => read_color_scheme(&connection, "Read", move |value| {
                if let Some(value) = value {
                    apply_read(&window, &changed, &value);
                }
            }),
        }
    });
    Some(ColorScheme {
        connection,
        subscription: Some(subscription),
    })
}

fn read_color_scheme(
    connection: &gio::DBusConnection,
    method: &str,
    done: impl FnOnce(Option<Variant>) + 'static,
) {
    connection.call(
        Some(PORTAL),
        PORTAL_PATH,
        PORTAL_SETTINGS,
        method,
        Some(&(APPEARANCE, COLOR_SCHEME).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        move |reply| done(reply.ok()),
    );
}

fn apply_read(window: &WebviewWindow, changed: &Cell<bool>, value: &Variant) {
    if !changed.get()
        && let Some(theme) = portal_value(value).and_then(portal_theme)
    {
        let _ = window.set_theme(Some(theme));
    }
}

/// The portal subscription of [`follow_color_scheme`].
struct ColorScheme {
    connection: gio::DBusConnection,
    subscription: Option<gio::SignalSubscriptionId>,
}

impl Drop for ColorScheme {
    fn drop(&mut self) {
        if let Some(subscription) = self.subscription.take() {
            self.connection.signal_unsubscribe(subscription);
        }
    }
}

/// Opens the window and runs the event loop until it closes.
fn open_window(url: Url, webview_data: &Path, smoke: Option<Smoke>) -> anyhow::Result<i32> {
    let app = tauri::Builder::default().build(tauri::generate_context!())?;
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::External(url))
        .title("omfp")
        .inner_size(1280.0, 800.0)
        // Per data directory: two daemons on the same port never share the window's storage.
        .data_directory(webview_data.to_path_buf())
        // WebKitGTK denies requests nobody handles: allow the UI's thread notifications.
        .on_permission_request(|_, kind| match kind {
            PermissionKind::Notifications => PermissionResponse::Allow,
            _ => PermissionResponse::Default,
        })
        .build()?;
    let _color_scheme = follow_color_scheme(&window);
    if smoke.is_some() {
        smoke_check(window, std::env::var("OMFP_SMOKE_SCHEME").ok());
    }
    Ok(app.run_return(|_, _| {}))
}

/// Polls [`PROBE`] every 100 ms; once the shell is ready (and the page's color scheme is `scheme`, when given:
/// `OMFP_SMOKE_SCHEME`) prints `OMFP_SMOKE_READY <state>` and closes the window like the user would (exit 0), else
/// exits 1 after [`SMOKE_DEADLINE`] with the last state.
fn smoke_check(window: WebviewWindow, scheme: Option<String>) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + SMOKE_DEADLINE;
        let mut last = String::new();
        while Instant::now() < deadline {
            let (sender, receiver) = mpsc::channel();
            if window
                .eval_with_callback(PROBE, move |json| {
                    let _ = sender.send(json);
                })
                .is_err()
            {
                break;
            }
            if let Ok(json) = receiver.recv_timeout(Duration::from_secs(2)) {
                let ready = serde_json::from_str::<serde_json::Value>(&json).is_ok_and(|state| {
                    state["ready"] == true
                        && scheme
                            .as_ref()
                            .is_none_or(|scheme| state["scheme"] == **scheme)
                });
                last = json;
                if ready {
                    println!("OMFP_SMOKE_READY {last}");
                    if window.close().is_ok() {
                        return;
                    }
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        eprintln!("omfp desktop: the shell was not ready: {last}");
        window.app_handle().exit(1);
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use omfp_daemon::agent::FakeLauncher;
    use omfp_daemon::git::FakeGit;
    use omfp_daemon::{Ports, start_with};
    use omfp_protocol::Empty;
    use omfp_protocol::method::ThemeGet;

    use super::*;

    #[derive(Debug, clap::Parser)]
    struct Line {
        #[command(flatten)]
        args: Args,
    }

    fn parse(line: &[&str]) -> Args {
        Line::try_parse_from(std::iter::once("desktop").chain(line.iter().copied()))
            .unwrap()
            .args
    }

    fn env(home: &Path) -> Env {
        Env {
            xdg_data_home: None,
            xdg_config_home: None,
            home: Some(home.to_path_buf()),
        }
    }

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    // spec: ARC-003, DMN-128
    #[test]
    fn arguments_after_the_separator_go_to_the_daemon() {
        let args = parse(&[
            "--data-dir",
            "/d",
            "--",
            "--static-dir",
            "/web",
            "--bind",
            "127.0.0.1:0",
        ]);
        assert_eq!(args.data_dir, Some(PathBuf::from("/d")));
        assert_eq!(
            args.daemon_args,
            ["--static-dir", "/web", "--bind", "127.0.0.1:0"]
        );
        let plan = LocalDaemon::plan(
            &args.data_dir,
            &args.daemon_args,
            &env(Path::new("/home/u")),
        )
        .unwrap();
        assert_eq!(plan.data_dir, PathBuf::from("/d"));
        assert!(plan.explicit_bind);
        assert_eq!(
            strings(&plan.command_line(None)),
            [
                "daemon",
                "--tray",
                "--data-dir",
                "/d",
                "--static-dir",
                "/web",
                "--bind",
                "127.0.0.1:0"
            ]
        );
        assert!(LocalDaemon::plan(&None, &["--no-such-flag".into()], &Env::default()).is_err());
    }

    // spec: ARC-003, DMN-002
    #[test]
    fn the_daemon_is_planned_like_omfp_daemon_resolves_it() {
        let home = tempfile::tempdir().unwrap();
        let plan = LocalDaemon::plan(&None, &[], &env(home.path())).unwrap();
        assert_eq!(plan.data_dir, home.path().join(".local/share/omfp"));
        assert_eq!(plan.bind, omfp_daemon::config::DEFAULT_BIND);
        assert!(!plan.explicit_bind);

        // The daemon's data directory may come after `--` instead, and the config file sets the bind address.
        let config = home.path().join(".config/omfp");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("config.toml"),
            "[daemon]\nbind = \"127.0.0.1:7500\"\n",
        )
        .unwrap();
        let plan = LocalDaemon::plan(
            &None,
            &["--data-dir".into(), "/other".into()],
            &env(home.path()),
        )
        .unwrap();
        assert_eq!(plan.data_dir, PathBuf::from("/other"));
        assert_eq!(plan.bind, "127.0.0.1:7500".parse().unwrap());
        assert!(!plan.explicit_bind);
        assert_eq!(
            strings(&plan.command_line(Some("127.0.0.1:0".parse().unwrap()))),
            [
                "daemon",
                "--tray",
                "--bind",
                "127.0.0.1:0",
                "--data-dir",
                "/other"
            ]
        );

        let twice = LocalDaemon::plan(
            &Some("/d".into()),
            &["--data-dir".into(), "/e".into()],
            &env(home.path()),
        );
        assert!(twice.unwrap_err().to_string().contains("twice"));
    }

    // spec: ARC-003
    #[test]
    fn a_taken_configured_port_falls_back_to_a_random_one() {
        let default = omfp_daemon::config::DEFAULT_BIND;
        assert_eq!(
            fallback_bind(default, false, true),
            Some("127.0.0.1:0".parse().unwrap())
        );
        assert_eq!(
            fallback_bind("0.0.0.0:7420".parse().unwrap(), false, true),
            Some("0.0.0.0:0".parse().unwrap())
        );
        // Another reason to exit, an explicit `--bind`, or a random port already: no retry.
        assert_eq!(fallback_bind(default, false, false), None);
        assert_eq!(fallback_bind(default, true, true), None);
        assert_eq!(
            fallback_bind("127.0.0.1:0".parse().unwrap(), false, true),
            None
        );

        let taken = TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(port_in_use(taken.local_addr().unwrap()));
        let free = taken.local_addr().unwrap();
        drop(taken);
        assert!(!port_in_use(free));
    }

    // spec: ARC-003, UI-004
    #[test]
    fn window_urls_carry_the_token_and_the_local_flag_in_the_fragment() {
        let local = local_base("127.0.0.1:7420".parse().unwrap()).unwrap();
        assert_eq!(
            window_url(&local, Some("t0k"), true, None).as_str(),
            "http://127.0.0.1:7420/#token=t0k&local=1"
        );
        assert_eq!(
            window_url(&local, Some("t0k"), false, None).as_str(),
            "http://127.0.0.1:7420/#token=t0k"
        );
        assert_eq!(
            local_base("0.0.0.0:7420".parse().unwrap())
                .unwrap()
                .as_str(),
            "http://127.0.0.1:7420/"
        );

        for (given, expected) in [
            ("https://omfp.example.com", "https://omfp.example.com/"),
            ("http://host:7420/", "http://host:7420/"),
            ("host:7420", "http://host:7420/"),
            (
                "https://example.com/proxy?x=1#token=old",
                "https://example.com/proxy/",
            ),
        ] {
            assert_eq!(connect_base(given).unwrap().as_str(), expected, "{given}");
        }
        let remote = connect_base("https://omfp.example.com").unwrap();
        assert_eq!(
            window_url(&remote, Some("t0k"), false, None).as_str(),
            "https://omfp.example.com/#token=t0k"
        );
        assert_eq!(
            window_url(&remote, None, false, None).as_str(),
            "https://omfp.example.com/"
        );
        assert!(connect_base("ws://host:7420").is_err());
        assert!(connect_base("http://").is_err());
    }

    // spec: UI-218
    #[test]
    fn a_window_on_a_thread_names_it_in_the_fragment() {
        let local = local_base("127.0.0.1:7420".parse().unwrap()).unwrap();
        assert_eq!(
            window_url(&local, Some("t0k"), true, Some("th-1")).as_str(),
            "http://127.0.0.1:7420/#token=t0k&local=1&thread=th-1"
        );
        assert_eq!(
            window_url(&local, None, false, Some("a&b=c#d%")).as_str(),
            "http://127.0.0.1:7420/#thread=a%26b%3Dc%23d%25"
        );
        let args = parse(&["--thread", "th-1", "--data-dir", "/d"]);
        assert_eq!(args.thread.as_deref(), Some("th-1"));
    }

    // spec: UI-195
    #[test]
    fn the_portal_color_scheme_picks_the_window_theme() {
        assert_eq!(portal_theme(1), Some(WindowTheme::Dark));
        assert_eq!(portal_theme(2), Some(WindowTheme::Light));
        // No preference, or a value the portal may add later: the toolkit's default.
        assert_eq!(portal_theme(0), None);
        assert_eq!(portal_theme(3), None);

        // `ReadOne` answers `(v)`, `Read` `(v)` holding a `v`, `SettingChanged` carries the `v`.
        let read_one = (1u32.to_variant(),).to_variant();
        let read = (Variant::from_variant(&2u32.to_variant()),).to_variant();
        assert_eq!(portal_value(&read_one), Some(1));
        assert_eq!(portal_value(&read), Some(2));
        assert_eq!(
            portal_value(&Variant::from_variant(&0u32.to_variant())),
            Some(0)
        );
        assert_eq!(portal_value(&"dark".to_variant()), None);
        assert_eq!(portal_value(&(1u32, 2u32).to_variant()), None);
    }

    // spec: TST-033
    #[test]
    fn the_smoke_variable_picks_the_daemon() {
        assert_eq!(Smoke::parse(None).unwrap(), None);
        assert_eq!(Smoke::parse(Some("1")).unwrap(), Some(Smoke::InProcess));
        assert_eq!(Smoke::parse(Some("local")).unwrap(), Some(Smoke::Local));
        assert!(Smoke::parse(Some("yes")).is_err());
    }

    // spec: UI-091
    #[test]
    fn only_a_connected_window_with_a_token_pushes_the_theme_file() {
        let base = connect_base("https://omfp.example.com").unwrap();
        let env = Env {
            xdg_config_home: Some("/xdg".into()),
            ..env(Path::new("/home/u"))
        };
        assert_eq!(
            theme_push(&base, Some("t0k"), &env),
            ThemePush::Push {
                target: Target {
                    url: "wss://omfp.example.com/api/ws".into(),
                    token: "t0k".into()
                },
                file: "/xdg/omfp/theme.css".into(),
            }
        );
        assert_eq!(theme_push(&base, None, &env), ThemePush::NeedsToken);
        assert_eq!(
            theme_push(&base, Some("t0k"), &Env::default()),
            ThemePush::NoThemeFile
        );
    }

    // spec: UI-091
    #[tokio::test]
    async fn the_theme_file_follows_to_the_connected_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let cli = Cli::try_parse_from([
            "daemon",
            "--bind",
            "127.0.0.1:0",
            "--data-dir",
            dir.path().join("data").to_str().unwrap(),
            "--workspaces-root",
            dir.path().join("workspaces").to_str().unwrap(),
        ])
        .unwrap();
        let ports = Ports {
            launcher: Arc::new(FakeLauncher::new()),
            git: Arc::new(FakeGit::new()),
            omp_program: "omp".into(),
            omp_version: omfp_core::OMP_VERSION.into(),
            omp_env: Vec::new(),
            warnings: Vec::new(),
        };
        let daemon = start_with(Config::from_cli(cli, None, None).unwrap(), ports)
            .await
            .unwrap();
        let target = Target {
            url: format!("ws://{}/api/ws", daemon.addr()),
            token: daemon.token().expose().to_owned(),
        };
        let file = dir.path().join("theme.css");
        std::fs::write(&file, ":root { --omfp-primary: #123456; }").unwrap();
        let pusher = tokio::spawn(push_theme(
            target.clone(),
            file.clone(),
            Duration::from_millis(20),
        ));

        let theme = |css: Option<&'static str>| {
            let target = target.clone();
            async move {
                loop {
                    let mut connection = Connection::open(&target).await.unwrap();
                    let theme = connection.call::<ThemeGet>(Empty {}).await.unwrap();
                    if theme.css.as_deref() == css {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        };
        let within = |future| tokio::time::timeout(Duration::from_secs(10), future);
        within(theme(Some(":root { --omfp-primary: #123456; }")))
            .await
            .unwrap();
        std::fs::write(&file, ":root { --omfp-primary: #654321; }").unwrap();
        within(theme(Some(":root { --omfp-primary: #654321; }")))
            .await
            .unwrap();
        std::fs::remove_file(&file).unwrap();
        within(theme(None)).await.unwrap();

        pusher.abort();
        daemon.shutdown().await.unwrap();
    }
}
