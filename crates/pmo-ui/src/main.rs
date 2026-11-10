//! The `pmo` binary (D-15). Natively it is the command line: `pmo daemon` runs the daemon (02-daemon). Built for the
//! browser with `dx build -p pmo-ui --platform web` it is the UI (04-ui.md).

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

    /// pmo: many omp agent threads in parallel, from the browser or the desktop.
    #[derive(Debug, Parser)]
    #[command(name = "pmo")]
    struct Pmo {
        #[command(subcommand)]
        command: Command,
    }

    #[derive(Debug, Subcommand)]
    enum Command {
        /// Runs the daemon: owns threads, workspaces and omp processes, serves the UI.
        Daemon(pmo_daemon::config::Cli),
    }

    let matches = Pmo::command()
        .version(pmo_daemon::version_string())
        .get_matches();
    let pmo = Pmo::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    match pmo.command {
        Command::Daemon(cli) => match pmo_daemon::run(cli) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("pmo daemon: {error:#}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(ui::App);
}

#[cfg(target_arch = "wasm32")]
mod ui {
    use dioxus::prelude::*;

    #[component]
    pub fn App() -> Element {
        rsx! {
            main { "data-testid": "app", "pmo" }
        }
    }
}
