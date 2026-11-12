//! The daemon's tray icon (TST-061): `omfp daemon --tray` on a private session bus (`dbus-daemon`, never the user's)
//! with a stub `org.kde.StatusNotifierWatcher`: the item registers, its menu is read over dbusmenu `GetLayout`, and
//! `Event clicked` on `Quit daemon` stops the daemon. No display is needed.
#![cfg(all(feature = "desktop", target_os = "linux"))]

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gio::glib::{self, ToVariant as _, Variant};
use rustix::process::{Pid, Signal, kill_process};

const DEADLINE: Duration = Duration::from_secs(60);

/// A private session bus, stopped on drop.
struct Bus {
    child: Child,
    address: String,
}

impl Bus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", session_config().display()))
            .args(["--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().to_owned(),
        }
    }

    fn connect(&self) -> gio::DBusConnection {
        gio::DBusConnection::for_address_sync(
            &self.address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            None::<&gio::Cancellable>,
        )
        .unwrap()
    }
}

/// The session bus configuration installed with the `dbus-daemon` on `PATH` (`../share/dbus-1/session.conf`):
/// `--session` reads `/etc/dbus-1/session.conf`, which not every host has (GitHub's runner after the disk purge).
fn session_config() -> PathBuf {
    let daemon = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("dbus-daemon"))
        .find(|path| path.is_file())
        .expect("dbus-daemon on PATH");
    let daemon = daemon.canonicalize().unwrap();
    daemon
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("share/dbus-1/session.conf")
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const WATCHER_XML: &str = r#"<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem"><arg type="s" name="service" direction="in"/></method>
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <property name="ProtocolVersion" type="i" access="read"/>
  </interface>
</node>"#;

/// A stub `org.kde.StatusNotifierWatcher` on `bus`: every `RegisterStatusNotifierItem` arrives as `(sender, service)`.
fn watcher(bus: &Bus) -> mpsc::Receiver<(String, String)> {
    let (registered, registrations) = mpsc::channel();
    let (ready, started) = mpsc::channel();
    let connection = bus.connect();
    std::thread::spawn(move || {
        let context = glib::MainContext::new();
        context
            .with_thread_default(|| {
                let interface = gio::DBusNodeInfo::for_xml(WATCHER_XML)
                    .unwrap()
                    .lookup_interface("org.kde.StatusNotifierWatcher")
                    .unwrap();
                let _registration = connection
                    .register_object(
                        "/StatusNotifierWatcher",
                        &interface,
                        move |_, sender, _, _, _, parameters, invocation| {
                            let (service,) = parameters.get::<(String,)>().unwrap();
                            invocation.return_value(None);
                            let _ = registered.send((sender.to_owned(), service));
                        },
                        |_, _, _, _, property| match property {
                            "ProtocolVersion" => 0i32.to_variant(),
                            _ => true.to_variant(),
                        },
                        |_, _, _, _, _, _| false,
                    )
                    .unwrap();
                connection
                    .call_sync(
                        Some("org.freedesktop.DBus"),
                        "/org/freedesktop/DBus",
                        "org.freedesktop.DBus",
                        "RequestName",
                        Some(&("org.kde.StatusNotifierWatcher", 4u32).to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        -1,
                        None::<&gio::Cancellable>,
                    )
                    .unwrap();
                ready.send(()).unwrap();
                glib::MainLoop::new(Some(&context), false).run();
            })
            .unwrap();
    });
    started.recv().unwrap();
    registrations
}

/// `omfp daemon --tray` with a temporary home and data directory on `bus_address`; its output lines are collected.
struct Daemon {
    child: Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
    data: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

impl Daemon {
    fn start(bus_address: &str) -> Self {
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
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("DBUS_SESSION_BUS_ADDRESS", bus_address)
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
        let mut daemon = Self {
            child,
            lines,
            seen: Vec::new(),
            data,
            _dir: dir,
        };
        daemon.wait_line("omfp daemon listening on");
        daemon
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

    /// Waits until the daemon exited; returns whether it succeeded.
    fn wait_exit(&mut self) -> bool {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.success();
            }
            assert!(
                Instant::now() < deadline,
                "the daemon did not stop:\n{}",
                self.output()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            if let Some(pid) = Pid::from_raw(i32::try_from(self.child.id()).unwrap()) {
                let _ = kill_process(pid, Signal::TERM);
            }
            let _ = self.child.wait();
        }
    }
}

fn call(
    connection: &gio::DBusConnection,
    service: &str,
    path: &str,
    interface: &str,
    method: &str,
    parameters: &Variant,
) -> Variant {
    connection
        .call_sync(
            Some(service),
            path,
            interface,
            method,
            Some(parameters),
            None,
            gio::DBusCallFlags::NONE,
            10_000,
            None::<&gio::Cancellable>,
        )
        .unwrap_or_else(|error| panic!("{interface}.{method}: {error}"))
}

fn property(connection: &gio::DBusConnection, service: &str, name: &str) -> Variant {
    call(
        connection,
        service,
        "/StatusNotifierItem",
        "org.freedesktop.DBus.Properties",
        "Get",
        &("org.kde.StatusNotifierItem", name).to_variant(),
    )
    .child_value(0)
    .as_variant()
    .unwrap()
}

/// The menu entries of a `GetLayout` reply as `(id, label)`, `---` for a separator and `(label)` for a disabled one.
fn entries(reply: &Variant) -> Vec<(i32, String)> {
    let root = reply.child_value(1);
    assert_eq!(root.child_value(0).get::<i32>(), Some(0));
    root.child_value(2)
        .iter()
        .map(|child| {
            let child = child.as_variant().unwrap();
            let id = child.child_value(0).get::<i32>().unwrap();
            let properties = child
                .child_value(1)
                .get::<HashMap<String, Variant>>()
                .unwrap();
            let label = if properties.get("type").and_then(Variant::get::<String>)
                == Some("separator".into())
            {
                "---".to_owned()
            } else {
                let label = properties["label"].get::<String>().unwrap();
                if properties.get("enabled").and_then(Variant::get::<bool>) == Some(false) {
                    format!("({label})")
                } else {
                    label
                }
            };
            (id, label)
        })
        .collect()
}

// spec: TST-061, DMN-128, DMN-130, UI-217, UI-218
#[test]
fn the_tray_registers_shows_its_menu_and_quits_the_daemon() {
    let bus = Bus::start();
    let registrations = watcher(&bus);
    let mut daemon = Daemon::start(&bus.address);
    let (sender, service) = registrations
        .recv_timeout(DEADLINE)
        .unwrap_or_else(|_| panic!("no registration:\n{}", daemon.output()));
    assert_eq!(
        service,
        format!("org.kde.StatusNotifierItem-{}-1", daemon.child.id())
    );
    assert!(sender.starts_with(':'), "{sender}");

    let connection = bus.connect();
    assert_eq!(
        property(&connection, &service, "Status").get::<String>(),
        Some("Active".into())
    );
    assert_eq!(
        property(&connection, &service, "Title").get::<String>(),
        Some("omfp".into())
    );
    let pixmaps = property(&connection, &service, "IconPixmap");
    assert_eq!(pixmaps.type_().as_str(), "a(iiay)");
    assert_eq!(pixmaps.n_children(), 2);
    let tooltip = property(&connection, &service, "ToolTip");
    assert_eq!(
        tooltip.child_value(3).get::<String>(),
        Some("0 threads · 0 running · 0 need you".into())
    );
    let menu = property(&connection, &service, "Menu");
    let menu = menu.str().unwrap().to_owned();

    let layout = call(
        &connection,
        &service,
        &menu,
        "com.canonical.dbusmenu",
        "GetLayout",
        &(0i32, -1i32, Vec::<String>::new()).to_variant(),
    );
    let entries = entries(&layout);
    let labels: Vec<&str> = entries.iter().map(|(_, label)| label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "New window",
            "Open in browser",
            "---",
            "(0 threads · 0 running · 0 need you)",
            "---",
            "Quit daemon"
        ]
    );

    let quit = entries
        .iter()
        .find(|(_, label)| label == "Quit daemon")
        .unwrap()
        .0;
    call(
        &connection,
        &service,
        &menu,
        "com.canonical.dbusmenu",
        "Event",
        &Variant::tuple_from_iter([
            quit.to_variant(),
            "clicked".to_variant(),
            Variant::from_variant(&0i32.to_variant()),
            0u32.to_variant(),
        ]),
    );
    assert!(daemon.wait_exit(), "{}", daemon.output());
    let output = daemon.output();
    assert!(output.contains("quit from the tray"), "{output}");
    assert!(output.contains("daemon stopped"), "{output}");
    assert_eq!(omfp_daemon::discover(&daemon.data).unwrap(), None);
}

// spec: TST-061, DMN-128
#[test]
fn without_a_watcher_the_daemon_says_so_once_and_registers_when_one_appears() {
    let bus = Bus::start();
    let mut daemon = Daemon::start(&bus.address);
    daemon.wait_line("no StatusNotifierWatcher on the session bus");

    let registrations = watcher(&bus);
    let (_, service) = registrations
        .recv_timeout(DEADLINE)
        .unwrap_or_else(|_| panic!("no registration:\n{}", daemon.output()));
    assert_eq!(
        service,
        format!("org.kde.StatusNotifierItem-{}-1", daemon.child.id())
    );
    let output = daemon.output();
    assert_eq!(
        output.matches("no StatusNotifierWatcher").count(),
        1,
        "{output}"
    );
}

// spec: DMN-128
#[test]
fn without_a_session_bus_the_daemon_runs_without_a_tray() {
    let dir = tempfile::tempdir().unwrap();
    let missing = format!("unix:path={}", dir.path().join("no-bus").display());
    let mut daemon = Daemon::start(&missing);
    daemon.wait_line("no tray icon: no session bus");
    // The daemon still serves: it stops cleanly on SIGTERM.
    kill_process(
        Pid::from_raw(i32::try_from(daemon.child.id()).unwrap()).unwrap(),
        Signal::TERM,
    )
    .unwrap();
    assert!(daemon.wait_exit(), "{}", daemon.output());
    assert_eq!(omfp_daemon::discover(&daemon.data).unwrap(), None);
}
