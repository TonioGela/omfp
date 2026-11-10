//! `cargo xtask <command>`: repository checks run by `just ci`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod spec_trace;
mod versions;

#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    /// List requirement citations and fail on broken traceability (TST-040).
    SpecTrace,
    /// Check that omp, omp-rpc, wasm-bindgen and dioxus versions agree (ENV-010, ENV-011, ENV-012).
    CheckVersions,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the repository root")
        .to_path_buf()
}

fn main() -> anyhow::Result<ExitCode> {
    let root = repo_root();
    match Cli::parse().command {
        Task::SpecTrace => {
            let report = spec_trace::run(&root)?;
            for (id, files) in &report.citations {
                let files: Vec<&str> = files.iter().map(String::as_str).collect();
                println!("{id:<8} {}", files.join(", "));
            }
            for (milestone, ids) in &report.pending {
                println!("pending in {milestone}: {}", ids.join(", "));
            }
            for error in &report.errors {
                eprintln!("error: {error}");
            }
            Ok(if report.errors.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Task::CheckVersions => {
            let observed = versions::observe(&root)?;
            let problems = versions::mismatches(&observed);
            for problem in &problems {
                eprintln!("error: {problem}");
            }
            if problems.is_empty() {
                println!(
                    "omp {} (flake {} / omp-rpc {}), wasm-bindgen {}, dioxus {} / dx {}",
                    observed.omp_version,
                    observed.flake_ref,
                    observed.cargo_tag,
                    observed.wasm_bindgen_crate,
                    observed.dioxus_crate,
                    observed.dx_cli
                );
            }
            Ok(if problems.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
    }
}
