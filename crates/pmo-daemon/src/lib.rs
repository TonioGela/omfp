//! The pmo daemon (`pmo daemon`), see docs/specs/02-daemon.md.

pub mod agent;
pub mod config;
pub mod event_log;
pub mod git;
pub mod omp;
pub mod omp_binary;
pub mod paths;
pub mod store;
pub mod threads;
pub mod token;
pub mod workspaces;

/// Version string reported by `pmo --version`: daemon version plus the omp release it speaks to.
#[must_use]
pub fn version_string() -> String {
    format!(
        "{} (omp {}, protocol {})",
        env!("CARGO_PKG_VERSION"),
        pmo_core::OMP_VERSION,
        pmo_protocol::PROTOCOL_VERSION
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
