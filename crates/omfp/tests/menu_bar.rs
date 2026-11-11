//! The daemon's menu bar item on macOS (TST-091): `omfp daemon --tray` in the logged-in user's GUI session (from an
//! SSH session too) shows its `NSStatusItem` as an accessory app, keeps serving, and stops cleanly on SIGTERM. The menu
//! is the tray's shared model (unit tests in `src/tray.rs`); clicks need a human (`docs/specs/07-roadmap.md`, M11).
#![cfg(all(feature = "desktop", target_os = "macos"))]

use std::io::{BufRead as _, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process};

const DEADLINE: Duration = Duration::from_secs(60);

/// `omfp daemon --tray` on a temporary data directory, its output lines collected; stopped on drop.
struct Daemon {
    child: Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
    data: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

impl Daemon {
    fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let mut child = Command::new(env!("CARGO_BIN_EXE_omfp"))
            .args(["daemon", "--tray", "--bind", "127.0.0.1:0", "--data-dir"])
            .arg(&data)
            .arg("--workspaces-root")
            .arg(dir.path().join("workspaces"))
            .env("HOME", dir.path())
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (sender, lines) = mpsc::channel();
        for stream in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let sender = sender.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let _ = sender.send(line);
                }
            });
        }
        Self {
            child,
            lines,
            seen: Vec::new(),
            data,
            _dir: dir,
        }
    }

    fn pid(&self) -> Pid {
        Pid::from_raw(i32::try_from(self.child.id()).unwrap()).unwrap()
    }

    /// Waits for an output line containing `text`.
    fn wait_line(&mut self, text: &str) {
        let deadline = Instant::now() + DEADLINE;
        while !self.seen.iter().any(|line| line.contains(text)) {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => self.seen.push(line),
                Err(error) => panic!(
                    "no `{text}` in the daemon's output ({error}):\n{}",
                    self.seen.join("\n")
                ),
            }
        }
    }

    fn output(&mut self) -> String {
        self.seen.extend(self.lines.try_iter());
        self.seen.join("\n")
    }

    /// Waits until the daemon exited and its output is read; returns whether it succeeded.
    fn wait_exit(&mut self) -> bool {
        let status = self.child.wait().unwrap();
        // Both readers end with the process: the channel disconnects once they are done.
        while let Ok(line) = self.lines.recv_timeout(DEADLINE) {
            self.seen.push(line);
        }
        status.success()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = kill_process(self.pid(), Signal::TERM);
            let _ = self.child.wait();
        }
    }
}

// spec: TST-091, DMN-171, UI-254
#[test]
fn the_menu_bar_item_shows_while_the_daemon_serves_and_goes_with_it() {
    let mut daemon = Daemon::start();
    daemon.wait_line("omfp daemon listening on");
    daemon.wait_line("omfp daemon: in the menu bar");
    let info = omfp_daemon::discover(&daemon.data)
        .unwrap()
        .expect("the daemon runs");
    assert_eq!(info.pid, daemon.child.id());
    // An accessory app (no Dock icon, no menu of its own): LaunchServices lists it as a UI element.
    let apps = Command::new("/usr/bin/lsappinfo")
        .arg("list")
        .output()
        .unwrap();
    let apps = String::from_utf8_lossy(&apps.stdout);
    let entry = format!("pid = {} type=\"UIElement\"", daemon.child.id());
    assert!(apps.contains(&entry), "no `{entry}` in:\n{apps}");

    kill_process(daemon.pid(), Signal::TERM).unwrap();
    assert!(daemon.wait_exit(), "{}", daemon.output());
    let output = daemon.output();
    assert!(!output.contains("no menu bar item"), "{output}");
    assert!(output.contains("daemon stopped"), "{output}");
    assert!(!output.contains("panicked"), "{output}");
    assert_eq!(omfp_daemon::discover(&daemon.data).unwrap(), None);
}
