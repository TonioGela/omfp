use clap::{CommandFactory, FromArgMatches, Parser};

/// pmo daemon: owns threads, workspaces and omp processes.
#[derive(Debug, Parser)]
#[command(name = "pmod")]
struct Cli {}

fn main() {
    let matches = Cli::command()
        .version(pmo_daemon::version_string())
        .get_matches();
    let _cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
}
