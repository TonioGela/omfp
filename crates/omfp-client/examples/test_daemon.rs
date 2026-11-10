//! The daemon `just test-wasm` runs the web transport suite against: an in-process daemon on `127.0.0.1:0` with the
//! fake omp launcher and fake git (as in the native L2 tests) and the data directory given as the only argument.
//!
//! It reserves a free port for `wasm-bindgen-test-runner` and allows that origin (PRT-003), prints
//! `<websocket url> <token> <runner address>` on one line, and serves until SIGINT or SIGTERM.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::Arc;

use clap::Parser;
use omfp_daemon::agent::FakeLauncher;
use omfp_daemon::config::{Cli, Config};
use omfp_daemon::git::FakeGit;
use omfp_daemon::{Ports, start_with};
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = std::env::args()
        .nth(1)
        .ok_or("usage: test_daemon <data-dir>")?;
    // Released at once: the test runner binds it next.
    let runner = TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let origin = format!("http://{runner}");
    let cli = Cli::try_parse_from([
        "daemon",
        "--bind",
        "127.0.0.1:0",
        "--data-dir",
        &data_dir,
        "--allow-origin",
        &origin,
    ])?;
    let ports = Ports {
        launcher: Arc::new(FakeLauncher::new()),
        git: Arc::new(FakeGit::new()),
        omp_program: "omp".into(),
        omp_version: omfp_core::OMP_VERSION.into(),
        omp_env: Vec::new(),
        warnings: Vec::new(),
    };
    let daemon = start_with(Config::from_cli(cli, None, None)?, ports).await?;
    let mut stdout = std::io::stdout();
    writeln!(
        stdout,
        "ws://{}/api/ws {} {runner}",
        daemon.addr(),
        daemon.token().expose()
    )?;
    stdout.flush()?;

    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _ = terminate.recv() => {}
    }
    daemon.shutdown().await?;
    Ok(())
}
