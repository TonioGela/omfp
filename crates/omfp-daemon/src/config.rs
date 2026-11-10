//! Daemon configuration from the command line (02-daemon, "Startup and configuration").

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;

/// Command line of `omfp daemon`.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(
    name = "daemon",
    about = "Runs the omfp daemon: owns threads, workspaces and omp processes, serves the UI"
)]
pub struct Cli {
    /// Address and port to listen on (DMN-002). Exposure beyond the machine is the deployer's job (D-13).
    #[arg(long, default_value = "127.0.0.1:7420")]
    pub bind: SocketAddr,
    /// Data directory (default: `$XDG_DATA_HOME/omfp`, else `~/.local/share/omfp`).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Where new workspace folders are made (DMN-020; default: `~/omfp/workspaces`).
    #[arg(long)]
    pub workspaces_root: Option<PathBuf>,
    /// omp executable; default: the bundled one, else `omp` on PATH (DMN-006).
    #[arg(long)]
    pub omp_bin: Option<PathBuf>,
    /// Directory with the web UI bundle to serve; default: the `web/` bundle next to the executable (DMN-008).
    #[arg(long)]
    pub static_dir: Option<PathBuf>,
    /// Run even if omp is not the pinned release (DMN-001); the UI shows a warning.
    #[arg(long)]
    pub allow_omp_mismatch: bool,
    /// Minutes before an idle, unwatched thread's omp is stopped (DMN-034).
    #[arg(long, default_value_t = 15)]
    pub idle_timeout_minutes: u64,
    /// Maximum number of omp processes running at once (DMN-038).
    #[arg(long, default_value_t = 8)]
    pub max_processes: usize,
    /// Events kept in memory per thread so reconnecting clients can catch up (DMN-041).
    #[arg(long, default_value_t = 2000)]
    pub event_buffer: usize,
    /// Extra allowed `Origin` values for WebSocket upgrades (PRT-003), e.g. behind a reverse proxy.
    #[arg(long = "allow-origin")]
    pub allowed_origins: Vec<String>,
}

/// Resolved daemon settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub workspaces_root: PathBuf,
    pub omp_bin: Option<PathBuf>,
    pub static_dir: Option<PathBuf>,
    pub allow_omp_mismatch: bool,
    pub idle_timeout: Duration,
    pub max_processes: usize,
    pub event_buffer: usize,
    pub allowed_origins: Vec<String>,
}

impl Config {
    /// Applies defaults that depend on the environment (`XDG_DATA_HOME`, `HOME`).
    pub fn from_cli(
        cli: Cli,
        xdg_data_home: Option<PathBuf>,
        home: Option<PathBuf>,
    ) -> Result<Self, String> {
        let data_dir = match cli.data_dir {
            Some(dir) => dir,
            None => xdg_data_home
                .filter(|dir| dir.is_absolute())
                .or_else(|| home.as_ref().map(|home| home.join(".local/share")))
                .map(|base| base.join("omfp"))
                .ok_or(
                    "cannot determine a data directory: set --data-dir, XDG_DATA_HOME or HOME",
                )?,
        };
        let workspaces_root = match cli.workspaces_root {
            Some(dir) => dir,
            None => home
                .map(|home| home.join("omfp").join("workspaces"))
                .ok_or("cannot determine the workspaces root: set --workspaces-root or HOME")?,
        };
        if cli.max_processes == 0 {
            return Err("--max-processes must be at least 1".into());
        }
        Ok(Self {
            bind: cli.bind,
            data_dir,
            workspaces_root,
            omp_bin: cli.omp_bin,
            static_dir: cli.static_dir,
            allow_omp_mismatch: cli.allow_omp_mismatch,
            idle_timeout: Duration::from_secs(cli.idle_timeout_minutes.saturating_mul(60)),
            max_processes: cli.max_processes,
            event_buffer: cli.event_buffer,
            allowed_origins: cli.allowed_origins,
        })
    }
}

/// Name of the web bundle directory shipped next to the `omfp` executable (packaging, DMN-008).
pub const BUNDLED_WEB_DIR: &str = "web";

/// The web UI to serve (DMN-008): `--static-dir`, else the bundle next to the executable when it holds an
/// `index.html`; `None` serves only the API.
#[must_use]
pub fn web_bundle(explicit: Option<&Path>, exe_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(explicit) = explicit {
        return Some(explicit.to_path_buf());
    }
    exe_dir
        .map(|dir| dir.join(BUNDLED_WEB_DIR))
        .filter(|bundle| bundle.join("index.html").is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("daemon").chain(args.iter().copied())).unwrap()
    }

    // spec: DMN-002
    #[test]
    fn listens_on_loopback_by_default_and_accepts_any_address() {
        assert_eq!(parse(&[]).bind, "127.0.0.1:7420".parse().unwrap());
        assert_eq!(
            parse(&["--bind", "0.0.0.0:9000"]).bind,
            "0.0.0.0:9000".parse().unwrap()
        );
    }

    #[test]
    fn data_dir_defaults_to_xdg_then_home() {
        let config = |xdg: Option<&str>, home: Option<&str>| {
            Config::from_cli(parse(&[]), xdg.map(PathBuf::from), home.map(PathBuf::from))
        };
        assert_eq!(
            config(Some("/xdg"), Some("/home/u")).unwrap().data_dir,
            PathBuf::from("/xdg/omfp")
        );
        assert_eq!(
            config(Some("relative"), Some("/home/u")).unwrap().data_dir,
            PathBuf::from("/home/u/.local/share/omfp")
        );
        assert!(config(None, None).is_err());
        let explicit = Config::from_cli(
            parse(&["--data-dir", "/d", "--workspaces-root", "/w"]),
            None,
            None,
        )
        .unwrap();
        assert_eq!(explicit.data_dir, PathBuf::from("/d"));
        assert_eq!(explicit.workspaces_root, PathBuf::from("/w"));
    }

    // spec: DMN-020
    #[test]
    fn workspace_folders_go_under_home_by_default() {
        let config = Config::from_cli(parse(&[]), None, Some("/home/u".into())).unwrap();
        assert_eq!(
            config.workspaces_root,
            PathBuf::from("/home/u/omfp/workspaces")
        );
        assert!(Config::from_cli(parse(&["--data-dir", "/d"]), None, None).is_err());
    }

    // spec: DMN-034, DMN-038, DMN-041
    #[test]
    fn idle_timeout_process_limit_and_event_buffer_have_spec_defaults() {
        let config = Config::from_cli(parse(&[]), None, Some("/h".into())).unwrap();
        assert_eq!(config.idle_timeout, Duration::from_mins(15));
        assert_eq!(config.max_processes, 8);
        assert_eq!(config.event_buffer, 2000);
        assert!(
            Config::from_cli(parse(&["--max-processes", "0"]), None, Some("/h".into())).is_err()
        );
    }

    // spec: DMN-008
    #[test]
    fn the_web_bundle_is_the_static_dir_else_the_one_next_to_the_executable() {
        let exe_dir = tempfile::tempdir().unwrap();
        let explicit = Path::new("/srv/omfp-web");
        assert_eq!(
            web_bundle(Some(explicit), Some(exe_dir.path())),
            Some(explicit.to_path_buf())
        );
        // No bundle (or an empty directory): the API only.
        assert_eq!(web_bundle(None, Some(exe_dir.path())), None);
        let bundle = exe_dir.path().join(BUNDLED_WEB_DIR);
        std::fs::create_dir(&bundle).unwrap();
        assert_eq!(web_bundle(None, Some(exe_dir.path())), None);
        std::fs::write(bundle.join("index.html"), "<!doctype html>").unwrap();
        assert_eq!(web_bundle(None, Some(exe_dir.path())), Some(bundle));
        assert_eq!(web_bundle(None, None), None);
    }
}
