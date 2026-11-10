//! The `omfp` command line (D-15): `omfp daemon` runs the daemon (02-daemon); the other subcommands talk to a running
//! daemon through the protocol (03-protocol) with [`remote::Connection`]. The UI is the TypeScript web app in `ui/`
//! (04-ui.md), served by the daemon.

pub mod remote;
pub mod theme;
