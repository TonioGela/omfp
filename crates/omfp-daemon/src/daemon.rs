//! Starting and stopping a daemon (02-daemon, "Startup and configuration"): in-process with [`start`] (DMN-004), or
//! as `omfp daemon` with [`run`]; one daemon per data directory, published in `daemon.json` (DMN-007).

use std::ffi::OsString;
use std::fs::{File, TryLockError};
use std::io::Write as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::agent::{AgentLauncher, OmpLauncher};
use crate::config::{Cli, Config, default_config_file, web_bundle};
use crate::conventions;
use crate::git::{Git, GitCli};
use crate::jobs::Jobs;
use crate::omp_binary;
use crate::paths::DataDir;
use crate::server::{self, AppState, Services};
use crate::store::Store;
use crate::supervisor::{Supervisor, SupervisorConfig};
use crate::theme::default_theme_file;
use crate::threads::Threads;
use crate::token::Token;
use crate::widgets::Widgets;
use crate::workspaces::Workspaces;

/// The system dependencies a daemon runs on (ARC-012).
#[derive(Clone)]
pub struct Ports {
    pub launcher: Arc<dyn AgentLauncher>,
    pub git: Arc<dyn Git>,
    /// omp executable given to the launcher (DMN-006).
    pub omp_program: PathBuf,
    /// omp version reported in `system.hello` (PRT-005).
    pub omp_version: String,
    /// Environment overrides for every omp process.
    pub omp_env: Vec<(OsString, Option<OsString>)>,
    /// Startup warnings shown to clients (DMN-001).
    pub warnings: Vec<String>,
}

impl std::fmt::Debug for Ports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ports")
            .field("git", &self.git)
            .field("omp_program", &self.omp_program)
            .field("omp_version", &self.omp_version)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

impl Ports {
    /// The real ports: omp resolved as `--omp-bin`, bundled, then `PATH` (DMN-006) and version-checked (DMN-001);
    /// git on `PATH`.
    pub fn resolve(config: &Config) -> anyhow::Result<Self> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let program = omp_binary::resolve(
            config.omp_bin.as_deref(),
            exe_dir.as_deref(),
            std::env::var_os("PATH").as_deref(),
        )
        .context("cannot find omp: pass --omp-bin or put omp on PATH")?;
        info!(omp = %program.display(), "using omp");
        let version = omp_binary::version(&program)?;
        let warning = omp_binary::accept(
            &program.display().to_string(),
            &version,
            omfp_core::OMP_VERSION,
            config.allow_omp_mismatch,
        )?;
        if let Some(warning) = &warning {
            warn!("{warning}");
        }
        Ok(Self {
            launcher: Arc::new(OmpLauncher),
            git: Arc::new(GitCli::new("git")),
            omp_program: program,
            omp_version: version,
            omp_env: Vec::new(),
            warnings: warning.into_iter().collect(),
        })
    }
}

/// What `<data-dir>/daemon.json` says about the running daemon (DMN-007, D-16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonInfo {
    /// Address the daemon listens on.
    pub addr: SocketAddr,
    pub pid: u32,
}

const INFO_FILE: &str = "daemon.json";
const LOCK_FILE: &str = "daemon.lock";

/// The daemon running for `data_dir`, if any: `daemon.json` is read and trusted only while its daemon still holds
/// the data directory's lock (the process is alive).
pub fn discover(data_dir: &Path) -> std::io::Result<Option<DaemonInfo>> {
    let text = match std::fs::read_to_string(data_dir.join(INFO_FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let lock = match File::open(data_dir.join(LOCK_FILE)) {
        Ok(lock) => lock,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    match lock.try_lock_shared() {
        // Nobody holds the lock: `daemon.json` is stale.
        Ok(()) => Ok(None),
        Err(TryLockError::WouldBlock) => serde_json::from_str(&text)
            .map(Some)
            .map_err(std::io::Error::other),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// Takes the data directory's exclusive lock (DMN-007).
fn lock_data_dir(data_dir: &Path) -> anyhow::Result<File> {
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data_dir.join(LOCK_FILE))
        .with_context(|| format!("cannot open the lock file in {}", data_dir.display()))?;
    match lock.try_lock() {
        Ok(()) => Ok(lock),
        Err(TryLockError::WouldBlock) => {
            let running = std::fs::read_to_string(data_dir.join(INFO_FILE))
                .ok()
                .and_then(|text| serde_json::from_str::<DaemonInfo>(&text).ok())
                .map(|info| format!(" (pid {}, listening on {})", info.pid, info.addr))
                .unwrap_or_default();
            bail!(
                "another omfp daemon is already running for {}{running}",
                data_dir.display()
            )
        }
        Err(TryLockError::Error(error)) => Err(error).context("cannot lock the data directory"),
    }
}

fn write_info(data_dir: &Path, info: &DaemonInfo) -> std::io::Result<()> {
    let temporary = data_dir.join(format!("{INFO_FILE}.tmp"));
    let mut file = File::create(&temporary)?;
    file.write_all(serde_json::to_string_pretty(info)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::rename(temporary, data_dir.join(INFO_FILE))
}

/// A running daemon.
#[derive(Debug)]
pub struct DaemonHandle {
    addr: SocketAddr,
    token: Token,
    data_dir: PathBuf,
    supervisor: Supervisor,
    jobs: Jobs,
    stop: watch::Sender<bool>,
    server: JoinHandle<std::io::Result<()>>,
    background: Vec<JoinHandle<()>>,
    _lock: File,
}

impl DaemonHandle {
    /// The bound address.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The access token clients present in `system.hello`.
    #[must_use]
    pub fn token(&self) -> &Token {
        &self.token
    }

    /// The UI's URL with the token in the fragment (never sent to the server, PRT-002).
    #[must_use]
    pub fn url(&self) -> String {
        let mut addr = self.addr;
        if addr.ip().is_unspecified() {
            addr.set_ip(IpAddr::V4(Ipv4Addr::LOCALHOST));
        }
        format!("http://{addr}/#token={}", self.token.expose())
    }

    /// Stops the daemon (DMN-005): connections close, every omp process and every job stops (at most 10 s),
    /// `daemon.json` is removed and the data directory unlocked.
    pub async fn shutdown(self) -> anyhow::Result<()> {
        let _ = self.stop.send(true);
        tokio::join!(self.supervisor.shutdown(), self.jobs.shutdown());
        for task in &self.background {
            task.abort();
        }
        let served = self.server.await;
        if let Err(error) = std::fs::remove_file(self.data_dir.join(INFO_FILE))
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!(%error, "cannot remove daemon.json");
        }
        served.context("the server task failed")??;
        info!("daemon stopped");
        Ok(())
    }
}

/// Starts a daemon in-process with the real ports (DMN-004), serving the web bundle shipped next to the executable
/// unless `--static-dir` names one (DMN-008).
pub async fn start(mut config: Config) -> anyhow::Result<DaemonHandle> {
    let ports = Ports::resolve(&config)?;
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    config.static_dir = web_bundle(config.static_dir.as_deref(), exe_dir.as_deref());
    if let Some(dir) = &config.static_dir {
        info!(web = %dir.display(), "serving the web UI");
    } else {
        warn!("no web UI bundle: pass --static-dir to serve one");
    }
    start_with(config, ports).await
}

/// Starts a daemon in-process on the given ports.
#[allow(clippy::too_many_lines)] // Startup wires every part in order.
pub async fn start_with(config: Config, ports: Ports) -> anyhow::Result<DaemonHandle> {
    let data_dir = std::path::absolute(&config.data_dir)?;
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("cannot create {}", data_dir.display()))?;
    let lock = lock_data_dir(&data_dir)?;
    let data = DataDir::new(&data_dir);
    let token = Token::load_or_create(&data.token()).context("cannot read or create the token")?;
    let workspaces_root = std::path::absolute(&config.workspaces_root)?;
    let store =
        Arc::new(Store::open(&data.db(), &workspaces_root).context("cannot open the database")?);
    let threads = Arc::new(Threads::new(Arc::clone(&store), data.clone()));
    let workspaces = Arc::new(Workspaces::new(
        Arc::clone(&store),
        ports.git,
        workspaces_root,
    ));
    let agent_dir = conventions::omp_agent_dir(&ports.omp_env, std::env::var_os("HOME").as_deref());
    let widgets = Arc::new(Widgets::new(data.widgets(), config.static_dir.as_deref()));
    let supervisor = Supervisor::new(
        ports.launcher,
        Arc::clone(&threads),
        Arc::clone(&workspaces),
        data.clone(),
        SupervisorConfig {
            omp_env: ports.omp_env,
            max_processes: config.max_processes,
            idle_timeout: config.idle_timeout,
            event_capacity: config.event_buffer,
            agent_dir,
            title_generation: config.title_generation,
            builtin_widgets: widgets.builtin_dir().map(Path::to_path_buf),
            ..SupervisorConfig::new(ports.omp_program)
        },
    );
    let services = Services::new(store, threads, workspaces, supervisor.clone(), widgets)
        .await
        .map_err(|error| anyhow::anyhow!(error.message))?;
    let jobs = services.jobs.clone();
    let mut background = vec![
        supervisor.spawn_reaper(),
        services.spawn_index_updates(),
        services.spawn_rescans(),
        services.spawn_polls(),
    ];
    if let Some(path) = config.theme_file.clone() {
        info!(theme = %path.display(), "following the theme file");
        background.push(
            services
                .watch_theme_file(path)
                .await
                .map_err(|error| anyhow::anyhow!(error.message))?,
        );
    }
    if let Some(path) = config.config_file.clone() {
        info!(config = %path.display(), "following the config file");
        background.push(
            services
                .follow_config_file(path, config.file_options.clone())
                .await
                .map_err(|error| anyhow::anyhow!(error.message))?,
        );
    }

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("cannot listen on {}", config.bind))?;
    let addr = listener.local_addr()?;
    write_info(
        &data_dir,
        &DaemonInfo {
            addr,
            pid: std::process::id(),
        },
    )
    .context("cannot write daemon.json")?;

    let (stop, stopping) = watch::channel(false);
    let mut widget_key = [0_u8; 16];
    getrandom::fill(&mut widget_key).map_err(|error| anyhow::anyhow!("no randomness: {error}"))?;
    let state = Arc::new(AppState {
        token: token.clone(),
        omp_version: ports.omp_version,
        warnings: ports.warnings,
        allowed_origins: config.allowed_origins,
        static_dir: config.static_dir,
        widget_base: format!("/widgets/{}", hex::encode(widget_key)),
        services,
        shutdown: stopping.clone(),
    });
    let mut stopped = stopping;
    let server = tokio::spawn(async move {
        axum::serve(listener, server::router(state))
            .with_graceful_shutdown(async move {
                let _ = stopped.wait_for(|stopping| *stopping).await;
            })
            .await
    });
    info!(%addr, data_dir = %data_dir.display(), "daemon listening");
    Ok(DaemonHandle {
        addr,
        token,
        data_dir,
        supervisor,
        jobs,
        stop,
        server,
        background,
        _lock: lock,
    })
}

/// `omfp daemon`: starts the daemon, prints its URL, and stops on SIGINT/SIGTERM (DMN-005).
pub fn run(mut cli: Cli) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    cli.config = cli
        .config
        .or_else(|| default_config_file(xdg_config_home.clone(), home.clone()));
    let mut config = Config::from_cli(
        cli,
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        home.clone(),
    )
    .map_err(anyhow::Error::msg)?;
    config.theme_file = config
        .theme_file
        .or_else(|| default_theme_file(xdg_config_home, home));
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let daemon = start(config).await?;
            println!("omfp daemon listening on {}", daemon.url());
            wait_for_signal().await?;
            info!("shutting down");
            daemon.shutdown().await
        })
}

async fn wait_for_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}
