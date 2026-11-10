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
    dioxus::launch(web::Root);
}

#[cfg(target_arch = "wasm32")]
mod web {
    use dioxus::prelude::*;
    use omfp_ui::{App, ViewInput};

    /// The web root: the app over its view input. Connecting `omfp-client` to the input and mapping intents to
    /// protocol calls is the host glue's job (M2 glue phase); until then the app shows "Connecting…".
    #[component]
    pub fn Root() -> Element {
        let input = use_signal(ViewInput::default);
        rsx! {
            App { input, on_intent: move |_| {} }
        }
    }
}
