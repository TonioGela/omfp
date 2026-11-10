//! `fake-llm --script script.json [--port N]`: serves a script until interrupted, printing the base URL.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use fake_llm::{FakeLlm, Script};

#[derive(Debug, Parser)]
#[command(
    name = "fake-llm",
    about = "Scripted OpenAI-compatible server for tests"
)]
struct Cli {
    /// JSON script: `{ "model": "...", "steps": [ { "when"?: {...}, "reply": {...} } ] }`.
    #[arg(long)]
    script: PathBuf,
    /// Port on 127.0.0.1 (0 picks a free one).
    #[arg(long, default_value_t = 0)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let script: Script = serde_json::from_str(&std::fs::read_to_string(&cli.script)?)?;
    let server = FakeLlm::start_on(script, SocketAddr::from(([127, 0, 0, 1], cli.port))).await?;
    println!("{}", server.base_url());
    tokio::signal::ctrl_c().await?;
    for failure in server.failures() {
        eprintln!("unexpected request: {failure}");
    }
    Ok(())
}
