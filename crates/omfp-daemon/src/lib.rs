//! The omfp daemon (`omfp daemon`), see docs/specs/02-daemon.md.
//!
//! [`start`] runs a daemon in-process (DMN-004); [`run`] is everything `omfp daemon` does; [`discover`] finds the
//! daemon running for a data directory (DMN-007).

pub mod agent;
pub mod config;
pub mod conventions;
mod daemon;
pub mod event_log;
pub mod git;
pub mod host_tools;
pub mod names;
pub mod omp;
pub mod omp_binary;
pub mod paths;
pub mod server;
pub mod store;
pub mod supervisor;
pub mod threads;
pub mod token;
pub mod workspaces;

pub use daemon::{DaemonHandle, DaemonInfo, Ports, discover, run, start, start_with};

/// Version string reported by `omfp --version`: daemon version plus the omp release it speaks to.
#[must_use]
pub fn version_string() -> String {
    format!(
        "{} (omp {}, protocol {})",
        env!("CARGO_PKG_VERSION"),
        omfp_core::OMP_VERSION,
        omfp_protocol::PROTOCOL_VERSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_string_names_daemon_omp_and_protocol_versions() {
        assert_eq!(version_string(), "0.1.0 (omp 18.18.6, protocol 1)");
    }
}
