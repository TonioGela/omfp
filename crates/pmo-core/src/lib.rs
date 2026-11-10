//! Domain model and pure reducer over omp events.
//!
//! This crate performs no I/O and compiles to `wasm32-unknown-unknown` (ARC-010), so the daemon and the UI share it.

/// omp RPC wire types, reused as-is instead of being redefined (ARC-011).
pub use omp_rpc::wire;

/// The omp release whose RPC protocol this build speaks.
///
/// Must equal the `omp-rpc` git tag in `Cargo.toml`, the `oh-my-pi` flake input, and `omp --version` (ENV-010).
pub const OMP_VERSION: &str = "18.18.6";
