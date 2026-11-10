//! Types of the UI <-> daemon protocol (docs/specs/03-protocol.md).
//!
//! Shared by the daemon and every client; compiles to `wasm32-unknown-unknown` (ARC-010).

/// Protocol version announced in `system.hello` (PRT-004).
pub const PROTOCOL_VERSION: u32 = 1;
