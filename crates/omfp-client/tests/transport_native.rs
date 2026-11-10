//! The transport suite (TST-030, UI-001) for [`NativeTransport`] against a real daemon (L2): port 0, a temporary
//! data directory, the fixture-driven fake omp launcher and fake git.

#![cfg(not(target_arch = "wasm32"))]

mod suite;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use clap::Parser;
use omfp_client::model::{ConnectionStatus, StartMode, TopicSync};
use omfp_client::{NativeTransport, TokioTimer, TransportError};
use omfp_core::ProcessState;
use omfp_daemon::agent::FakeLauncher;
use omfp_daemon::config::{Cli, Config};
use omfp_daemon::git::FakeGit;
use omfp_daemon::{DaemonHandle, Ports, start_with};
use omfp_protocol::Topic;
use suite::{Harness, connect_plain, create_thread, until};
use tempfile::TempDir;

struct NativeHarness {
    daemon: DaemonHandle,
    dir: TempDir,
}

fn config(data_dir: &Path, bind: SocketAddr) -> Config {
    let cli = Cli::try_parse_from([
        "daemon",
        "--bind",
        &bind.to_string(),
        "--data-dir",
        data_dir.to_str().unwrap(),
    ])
    .unwrap();
    Config::from_cli(cli, None, None).unwrap()
}

async fn start(data_dir: &Path, bind: SocketAddr) -> DaemonHandle {
    let launcher = FakeLauncher::new();
    let ports = Ports {
        launcher: Arc::new(launcher),
        git: Arc::new(FakeGit::new()),
        omp_program: "omp".into(),
        omp_version: omfp_core::OMP_VERSION.into(),
        omp_env: Vec::new(),
        warnings: Vec::new(),
    };
    start_with(config(data_dir, bind), ports).await.unwrap()
}

impl NativeHarness {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let daemon = start(dir.path(), "127.0.0.1:0".parse().unwrap()).await;
        Self { daemon, dir }
    }

    /// Stops the daemon and starts a new one on the same data directory and address.
    async fn restart(self) -> Self {
        let addr = self.daemon.addr();
        self.daemon.shutdown().await.unwrap();
        let daemon = start(self.dir.path(), addr).await;
        Self {
            daemon,
            dir: self.dir,
        }
    }
}

impl Harness for NativeHarness {
    type Transport = NativeTransport;
    type Timer = TokioTimer;

    fn url(&self) -> String {
        format!("ws://{}/api/ws", self.daemon.addr())
    }

    fn token(&self) -> String {
        self.daemon.token().expose().to_owned()
    }

    fn timer(&self) -> TokioTimer {
        TokioTimer
    }

    fn open(url: String) -> impl Future<Output = Result<NativeTransport, TransportError>> {
        NativeTransport::connect(url)
    }
}

// spec: TST-030, UI-001, PRT-001, PRT-005
#[tokio::test(flavor = "multi_thread")]
async fn hello() {
    suite::hello(&NativeHarness::new().await).await;
}

// spec: TST-030, UI-001, PRT-002
#[tokio::test(flavor = "multi_thread")]
async fn auth_failure() {
    suite::auth_failure(&NativeHarness::new().await).await;
}

// spec: TST-030, UI-001, PRT-020, PRT-021
#[tokio::test(flavor = "multi_thread")]
async fn subscribe_snapshot() {
    Box::pin(suite::subscribe_snapshot(&NativeHarness::new().await)).await;
}

// spec: TST-030, UI-001, UI-003, PRT-020
#[tokio::test(flavor = "multi_thread")]
async fn replay() {
    Box::pin(suite::replay(&NativeHarness::new().await)).await;
}

// spec: TST-030, UI-001, UI-003
#[tokio::test(flavor = "multi_thread")]
async fn reconnect() {
    suite::reconnect(&NativeHarness::new().await).await;
}

// spec: TST-030, UI-003, PRT-020
#[tokio::test(flavor = "multi_thread")]
async fn a_restarted_daemon_is_reconnected_with_fresh_snapshots() {
    let harness = NativeHarness::new().await;
    let (client, driver) = connect_plain(&harness, "suite/restart");
    let driver = tokio::spawn(driver);
    let thread = create_thread(&client, "survives restarts").await;
    let topic = Topic::Thread(thread.clone());
    client.subscribe(Topic::Index);
    client.subscribe(topic.clone());
    let snapshots = |model: &omfp_client::Model| {
        matches!(model.connection(), ConnectionStatus::Connected(_))
            && [&Topic::Index, &topic].into_iter().all(|topic| {
                model
                    .topic(topic)
                    .is_some_and(|state| state.sync == TopicSync::Live(StartMode::Snapshot))
            })
    };
    until(&client, &TokioTimer, "the first snapshots", snapshots).await;
    let epoch = |model: &omfp_client::Model| {
        model
            .topic(&Topic::Index)
            .and_then(|state| state.position.clone())
            .map(|position| position.epoch)
    };
    let first_epoch = client.read(epoch);

    let harness = harness.restart().await;

    until(
        &client,
        &TokioTimer,
        "the snapshots of the new daemon",
        |model| {
            snapshots(model)
                && model.index().threads.contains_key(&thread)
                && epoch(model) != first_epoch
        },
    )
    .await;
    let process = client.read(|model| model.thread(&thread).unwrap().state.process.clone());
    assert_eq!(process, ProcessState::Stopped);
    client.close();
    driver.await.unwrap();
    harness.daemon.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_native_driver_runs_on_a_multi_threaded_runtime() {
    let harness = NativeHarness::new().await;
    let (client, driver) = connect_plain(&harness, "suite/spawned");

    // Compiles only if the driver is `Send`.
    let driver = tokio::spawn(driver);

    until(&client, &TokioTimer, "hello", |model| {
        matches!(model.connection(), ConnectionStatus::Connected(_))
    })
    .await;
    client.close();
    driver.await.unwrap();
}
