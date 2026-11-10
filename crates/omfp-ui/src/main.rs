//! The `omfp` binary (D-15). Natively it is the command line: `omfp daemon` runs the daemon (02-daemon). Built for the
//! browser with `dx build -p omfp-ui --platform web` it is the UI (04-ui.md).

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
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
    }

    let matches = Omfp::command()
        .version(omfp_daemon::version_string())
        .get_matches();
    let omfp = Omfp::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    match omfp.command {
        Command::Daemon(cli) => match omfp_daemon::run(cli) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("omfp daemon: {error:#}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(omfp_ui::host::web::Root);
}
