//! The `omfp` binary (D-15): `omfp daemon` runs the daemon (02-daemon), `omfp theme set` sets the UI theme (UI-091),
//! `omfp desktop` opens a desktop window (feature `desktop`, M7).

use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

/// omfp: many omp agent threads in parallel, from the browser or the desktop.
#[derive(Debug, Parser)]
#[command(name = "omfp")]
struct Omfp {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Runs the daemon: owns threads, workspaces and omp processes, serves the UI.
    Daemon(omfp_daemon::config::Cli),
    /// The UI theme of a daemon (UI-091).
    #[command(subcommand)]
    Theme(omfp::theme::Command),
    /// Opens a desktop window on the local daemon, starting it in the background if needed (D-15, D-16), or with
    /// `--connect` on another daemon.
    #[cfg(feature = "desktop")]
    Desktop(omfp::desktop::Args),
}

fn main() -> ExitCode {
    let matches = Omfp::command()
        .version(omfp_daemon::version_string())
        .get_matches();
    let omfp = Omfp::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    match omfp.command {
        Command::Daemon(cli) => report("omfp daemon", omfp_daemon::run(cli).map(|()| None)),
        Command::Theme(command) => {
            report("omfp theme", block_on(omfp::theme::run(command)).map(Some))
        }
        #[cfg(feature = "desktop")]
        Command::Desktop(args) => match omfp::desktop::run(&args) {
            Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
            Err(error) => report("omfp desktop", Err(error)),
        },
    }
}

/// Runs a client subcommand on a single-threaded runtime.
fn block_on<T>(future: impl Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(future)
}

fn report(command: &str, result: anyhow::Result<Option<String>>) -> ExitCode {
    match result {
        Ok(message) => {
            if let Some(message) = message {
                println!("{message}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{command}: {error:#}");
            ExitCode::FAILURE
        }
    }
}
