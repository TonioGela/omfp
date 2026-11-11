//! The `omfp` command line (D-15): `omfp daemon` runs the daemon (02-daemon); the other subcommands talk to a running
//! daemon through the protocol (03-protocol) with [`remote::Connection`]; `omfp desktop` (feature `desktop`, M7) opens
//! a window on a daemon, and `omfp daemon --tray` shows the daemon it starts in the system tray. The UI is the
//! TypeScript web app in `ui/` (04-ui.md), served by the daemon.

#[cfg(feature = "desktop")]
pub mod desktop;
pub mod remote;
pub mod theme;
#[cfg(feature = "desktop")]
pub mod tray;
