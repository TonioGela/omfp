//! Desktop tests (M7, M11) of the real `omfp desktop` window. On Linux under a virtual X server, never the user's
//! display: they need `xvfb-run`, and `just test-desktop` runs them one at a time (each `xvfb-run -a` picks a free
//! display, which races when two start at once). On macOS (TST-090) the window opens on the logged-in user's screen,
//! from an SSH session too, and closes itself once ready. Both need the release web bundle (`just build-web`).
#![cfg(feature = "desktop")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rustix::process::{Pid, Signal, kill_process};

/// The web bundle built by `just build-web` (`ui/packages/app/dist`), or `OMFP_WEB_BUNDLE` (`just test-desktop` builds a
/// private one, which no other build can replace while the tests run).
fn web_bundle() -> PathBuf {
    std::env::var_os("OMFP_WEB_BUNDLE").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/packages/app/dist"),
        PathBuf::from,
    )
}

/// `omfp desktop <args>` with `OMFP_SMOKE=<smoke>` and a temporary home.
fn desktop(home: &Path, smoke: &str, args: &[&str]) -> Output {
    desktop_with(home, &[("OMFP_SMOKE", smoke)], args)
}

/// `omfp desktop <args>` (under `xvfb-run` on Linux) with `env` and a temporary home; it must print
/// `OMFP_SMOKE_READY`.
fn desktop_with(home: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("xvfb-run");
        command
            .args(["-a", "-s", "-screen 0 1280x800x24"])
            .arg(env!("CARGO_BIN_EXE_omfp"))
            // Never the user's session bus (the window reads the settings portal, the background daemon shows a tray
            // icon): a bus that does not exist unless the test passes its own.
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}", home.join("no-bus").display()),
            )
            // Never the user's display.
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("GDK_BACKEND", "x11");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = Command::new(env!("CARGO_BIN_EXE_omfp"));
    let output = command
        .arg("desktop")
        .args(args)
        .envs(env.iter().copied())
        .env("HOME", home)
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    #[cfg(target_os = "macos")]
    if let Some(data) = args
        .iter()
        .position(|arg| *arg == "--data-dir")
        .and_then(|at| args.get(at + 1))
    {
        forget_web_store(Path::new(data));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("OMFP_SMOKE_READY"),
        "omfp desktop failed ({}):\n{stdout}\n{stderr}",
        output.status
    );
    output
}

/// Deletes the window's `WKWebsiteDataStore` of the data directory `data` (UI-250): `WebKit` keeps it outside the test's
/// temporary directory, whatever the window's `HOME`, in the user's
/// `~/Library/WebKit/dev.omfp.desktop/WebsiteDataStore/<the store id as a UUID>` (about 0.7 MB per window).
#[cfg(target_os = "macos")]
fn forget_web_store(data: &Path) {
    use std::fmt::Write as _;
    let (Ok(id), Some(home)) = (
        std::fs::read(data.join("webview/store-id")),
        std::env::var_os("HOME"),
    ) else {
        return;
    };
    let hex = id.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    if hex.len() != 32 {
        return;
    }
    let uuid = format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    let _ = std::fs::remove_dir_all(
        Path::new(&home)
            .join("Library/WebKit/dev.omfp.desktop/WebsiteDataStore")
            .join(uuid),
    );
}

// spec: TST-033, TST-090, DMN-004
#[test]
fn the_desktop_window_renders_the_shell() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let workspaces = dir.path().join("workspaces");
    desktop(
        dir.path(),
        "1",
        &[
            "--data-dir",
            data.to_str().unwrap(),
            "--",
            "--static-dir",
            web_bundle().to_str().unwrap(),
            "--workspaces-root",
            workspaces.to_str().unwrap(),
        ],
    );
    // The in-process daemon stopped with the window.
    assert_eq!(omfp_daemon::discover(&data).unwrap(), None);
}

// spec: TST-090, UI-250, UI-251, UI-252, UI-253
#[cfg(target_os = "macos")]
#[test]
fn the_macos_window_has_webrtc_the_microphone_and_notifications() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let workspaces = dir.path().join("workspaces");
    let output = desktop(
        dir.path(),
        "1",
        &[
            "--data-dir",
            data.to_str().unwrap(),
            "--",
            "--static-dir",
            web_bundle().to_str().unwrap(),
            "--workspaces-root",
            workspaces.to_str().unwrap(),
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let state = stdout
        .lines()
        .find_map(|line| line.strip_prefix("OMFP_SMOKE_READY "))
        .expect("the ready line");
    let state: serde_json::Value = serde_json::from_str(state).unwrap();
    // WKWebView's WebRTC (live voice, UI-251), `getUserMedia` thanks to the embedded Info.plist, the window's
    // `Notification` (UI-252).
    assert_eq!(state["rtcPeerConnection"], "function", "{state}");
    assert_eq!(state["mediaDevices"], "object", "{state}");
    assert_eq!(state["notification"], "granted", "{state}");
    // UI-253: the page's color scheme is the system appearance's (`AppleInterfaceStyle` is set in Dark mode only).
    let dark = Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .unwrap();
    let system = if String::from_utf8_lossy(&dark.stdout).trim() == "Dark" {
        "dark"
    } else {
        "light"
    };
    assert_eq!(state["scheme"], system, "{state}");
    // UI-250: the web storage of the data directory.
    let id = std::fs::read(data.join("webview/store-id")).unwrap();
    assert_eq!(id.len(), 16);
}

/// Stops the background daemon of a test, also when the test fails.
struct Daemon(i32);

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(pid) = Pid::from_raw(self.0) {
            let _ = kill_process(pid, Signal::TERM);
        }
    }
}

// spec: ARC-003, UI-004, UI-005, DMN-128, DMN-180, DMN-181
#[test]
fn a_second_window_reuses_the_background_daemon_that_outlives_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let workspaces = dir.path().join("workspaces");
    let web = web_bundle();
    let args = [
        "--data-dir",
        data.to_str().unwrap(),
        "--",
        "--static-dir",
        web.to_str().unwrap(),
        "--workspaces-root",
        workspaces.to_str().unwrap(),
        // A random port: the default one may be the user's daemon.
        "--bind",
        "127.0.0.1:0",
    ];

    let first = desktop(dir.path(), "local", &args);
    // The first window closed; its background daemon still runs (D-16).
    let info = omfp_daemon::discover(&data)
        .unwrap()
        .expect("the daemon outlives the window");
    let daemon = Daemon(i32::try_from(info.pid).unwrap());
    let stderr = String::from_utf8_lossy(&first.stderr);
    assert!(
        stderr.contains(&format!("started the daemon (pid {})", info.pid)),
        "{stderr}"
    );
    let log = std::fs::read_to_string(data.join("daemon.log")).unwrap();
    assert!(log.contains("omfp daemon listening on"), "{log}");
    // DMN-180, DMN-181: neither the log nor its directory are open to others, and the log never holds the token.
    let token = std::fs::read_to_string(data.join("token")).unwrap();
    let token = token.trim();
    assert!(!log.contains(token), "{log}");
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&data), 0o700);
        assert_eq!(mode(&data.join("daemon.log")), 0o600);
    }
    // DMN-128: started with `--tray`; on Linux the test's session bus does not exist, on macOS the item is in the
    // menu bar (DMN-171).
    #[cfg(target_os = "linux")]
    assert!(log.contains("no tray icon: no session bus"), "{log}");
    #[cfg(target_os = "macos")]
    assert!(!log.contains("no menu bar item"), "{log}");
    // UI-004: the URL a browser opens on the same daemon; with the token only on a terminal, not this pipe (DMN-181).
    let url = format!(
        "omfp desktop: http://{}/ (add #token=<token> from {})",
        info.addr,
        data.join("token").display()
    );
    assert!(
        String::from_utf8_lossy(&first.stdout).contains(&url),
        "{url}"
    );

    let second = desktop(dir.path(), "local", &args);
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains(&format!("found the daemon (pid {})", info.pid)),
        "{stderr}"
    );
    assert!(!stderr.contains("started the daemon"), "{stderr}");
    assert!(String::from_utf8_lossy(&second.stdout).contains(&url));
    assert_eq!(omfp_daemon::discover(&data).unwrap(), Some(info));

    // SIGTERM stops it cleanly (DMN-005): its data directory's lock is free once it exited.
    drop(daemon);
    std::fs::File::open(data.join("daemon.lock"))
        .unwrap()
        .lock_shared()
        .unwrap();
    assert_eq!(omfp_daemon::discover(&data).unwrap(), None);
}

/// The settings portal's color scheme (UI-195, Linux only: on macOS the window follows `AppKit`'s appearance, UI-253).
#[cfg(target_os = "linux")]
mod portal {
    use std::io::{BufRead as _, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;

    use gio::glib::{self, ToVariant as _};

    use super::{desktop_with, web_bundle};

    const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
    const PORTAL_SETTINGS: &str = "org.freedesktop.portal.Settings";
    const PORTAL_XML: &str = r#"<node>
  <interface name="org.freedesktop.portal.Settings">
    <method name="ReadOne">
      <arg type="s" name="namespace" direction="in"/>
      <arg type="s" name="key" direction="in"/>
      <arg type="v" name="value" direction="out"/>
    </method>
    <signal name="SettingChanged">
      <arg type="s" name="namespace"/>
      <arg type="s" name="key"/>
      <arg type="v" name="value"/>
    </signal>
  </interface>
</node>"#;

    /// A private session bus (`dbus-daemon`, stopped on drop) holding a stub settings portal: it answers
    /// `org.freedesktop.appearance color-scheme` with `initial`, then, when `then` is given, announces that value with
    /// `SettingChanged` right after the answer, as a live switch on the desktop would.
    struct Portal {
        bus: Child,
        address: String,
    }

    impl Portal {
        fn start(initial: u32, then: Option<u32>) -> Self {
            let mut bus = Command::new("dbus-daemon")
                .arg(format!("--config-file={}", session_config().display()))
                .args(["--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut address = String::new();
            BufReader::new(bus.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            let address = address.trim().to_owned();
            let (ready, registered) = mpsc::channel();
            let bus_address = address.clone();
            std::thread::spawn(move || {
                let context = glib::MainContext::new();
                context
                    .with_thread_default(|| {
                        let connection = gio::DBusConnection::for_address_sync(
                            &bus_address,
                            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                            None,
                            None::<&gio::Cancellable>,
                        )
                        .unwrap();
                        let interface = gio::DBusNodeInfo::for_xml(PORTAL_XML)
                            .unwrap()
                            .lookup_interface(PORTAL_SETTINGS)
                            .unwrap();
                        let _registration = connection
                            .register_object(
                                PORTAL_PATH,
                                &interface,
                                move |connection, _, _, _, _, parameters, invocation| {
                                    let (namespace, key) =
                                        parameters.get::<(String, String)>().unwrap_or_default();
                                    if namespace != "org.freedesktop.appearance"
                                        || key != "color-scheme"
                                    {
                                        invocation.return_dbus_error(
                                            "org.freedesktop.portal.Error.NotFound",
                                            "no such setting",
                                        );
                                        return;
                                    }
                                    invocation
                                        .return_value(Some(&(initial.to_variant(),).to_variant()));
                                    if let Some(then) = then {
                                        connection
                                            .emit_signal(
                                                None,
                                                PORTAL_PATH,
                                                PORTAL_SETTINGS,
                                                "SettingChanged",
                                                Some(
                                                    &(namespace, key, then.to_variant())
                                                        .to_variant(),
                                                ),
                                            )
                                            .unwrap();
                                    }
                                },
                                |_, _, _, _, _| "".to_variant(),
                                |_, _, _, _, _, _| false,
                            )
                            .unwrap();
                        // Owned after the object is registered, so the window never sees the name without it.
                        connection
                            .call_sync(
                                Some("org.freedesktop.DBus"),
                                "/org/freedesktop/DBus",
                                "org.freedesktop.DBus",
                                "RequestName",
                                Some(&("org.freedesktop.portal.Desktop", 4u32).to_variant()),
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
            registered.recv().unwrap();
            Self { bus, address }
        }
    }

    impl Drop for Portal {
        fn drop(&mut self) {
            let _ = self.bus.kill();
            let _ = self.bus.wait();
        }
    }

    /// The session bus configuration installed with the `dbus-daemon` on `PATH` (`../share/dbus-1/session.conf`):
    /// `--session` reads `/etc/dbus-1/session.conf`, which not every host has (GitHub's runner after the disk purge).
    fn session_config() -> std::path::PathBuf {
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

    /// The page's color scheme in a smoke window on `portal`'s session bus (`OMFP_SMOKE_SCHEME=<expected>` waits for it).
    fn scheme_on(portal: &Portal, expected: &str) -> String {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let workspaces = dir.path().join("workspaces");
        let web = web_bundle();
        let output = desktop_with(
            dir.path(),
            &[
                ("OMFP_SMOKE", "1"),
                ("OMFP_SMOKE_SCHEME", expected),
                ("DBUS_SESSION_BUS_ADDRESS", &portal.address),
            ],
            &[
                "--data-dir",
                data.to_str().unwrap(),
                "--",
                "--static-dir",
                web.to_str().unwrap(),
                "--workspaces-root",
                workspaces.to_str().unwrap(),
            ],
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    // spec: UI-195
    #[test]
    fn the_window_takes_the_dark_color_scheme_of_the_settings_portal() {
        let portal = Portal::start(1, None);
        assert!(scheme_on(&portal, "dark").contains(r#""scheme":"dark""#));
    }

    // spec: UI-195
    #[test]
    fn the_window_follows_a_color_scheme_change_of_the_settings_portal() {
        // Dark at startup, then switched to light while the window is open.
        let portal = Portal::start(1, Some(2));
        assert!(scheme_on(&portal, "light").contains(r#""scheme":"light""#));
    }
}
