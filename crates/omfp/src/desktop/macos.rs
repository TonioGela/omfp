//! The window's macOS parts (M11, S-15): its web storage per data directory (UI-250), the page's notifications
//! through the system's notification center (UI-252), only from the daemon's own page (DMN-182), and the app menu's
//! New Window and zoom items (UI-281, UI-282). The pure parts also build in Linux tests.

use std::fmt::Write as _;
use std::hash::{BuildHasher as _, Hasher as _, RandomState};
use std::path::Path;

#[cfg(target_os = "macos")]
use std::ffi::OsString;
#[cfg(target_os = "macos")]
use std::sync::{Mutex, PoisonError};

use tauri::Url;

use super::Zoom;
#[cfg(target_os = "macos")]
use super::zoomed;

/// The URL scheme of the page's notifications: the window cancels the navigation and posts the notification.
pub const NOTIFY_SCHEME: &str = "omfp-notify";

/// The file in the window's web storage directory (`<data-dir>/webview/`) that keeps its data store identifier.
const STORE_ID: &str = "store-id";

/// The `WKWebsiteDataStore` of a data directory (UI-250): 16 random bytes kept in `<webview_data>/store-id`, made on
/// first use, so the window's storage follows the data directory as `WebKitGTK`'s `data_directory` does on Linux
/// (UI-005). A file that is not 16 bytes is replaced.
pub fn data_store_identifier(webview_data: &Path) -> anyhow::Result<[u8; 16]> {
    let file = webview_data.join(STORE_ID);
    if let Ok(bytes) = std::fs::read(&file)
        && let Ok(id) = <[u8; 16]>::try_from(bytes.as_slice())
    {
        return Ok(id);
    }
    let id = random_bytes();
    std::fs::create_dir_all(webview_data)?;
    std::fs::write(&file, id)?;
    Ok(id)
}

/// 16 random bytes: each `RandomState` has its own random keys, so two hashes of nothing are random.
fn random_bytes() -> [u8; 16] {
    let random = || RandomState::new().build_hasher().finish().to_le_bytes();
    let mut id = [0; 16];
    id[..8].copy_from_slice(&random());
    id[8..].copy_from_slice(&random());
    id
}

/// The secret of a window's notifications (DMN-182): only its own [`notification_script`] knows it, so a frame of
/// another origin cannot post notifications under omfp's name by navigating to `omfp-notify:`.
#[must_use]
pub fn notify_nonce() -> String {
    random_bytes().iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// The page's `Notification` in the window (UI-252): `WKWebView` denies web notifications, so the window replaces the
/// API before the page's scripts run, in the top-level page of the daemon's `origin` only (DMN-182). The permission is
/// `granted`; each notification is a navigation of a hidden frame to `omfp-notify:?n=<nonce>&title=…&body=…&tag=…`,
/// which [`navigation`] cancels and posts.
#[must_use]
pub fn notification_script(origin: &str, nonce: &str) -> String {
    let quoted = |text: &str| serde_json::Value::from(text).to_string();
    format!(
        "(() => {{\n  const ORIGIN = {};\n  const NONCE = {};\n{NOTIFICATION_BODY}",
        quoted(origin),
        quoted(nonce)
    )
}

const NOTIFICATION_BODY: &str = r#"  if (window.top !== window || location.origin !== ORIGIN) return;
  class Notification extends EventTarget {
    static get permission() { return "granted"; }
    static requestPermission(callback) { callback?.("granted"); return Promise.resolve("granted"); }
    constructor(title, options = {}) {
      super();
      this.title = String(title);
      this.body = String(options.body ?? "");
      this.tag = String(options.tag ?? "");
      this.onclick = null;
      this.onclose = null;
      this.onerror = null;
      this.onshow = null;
      const frame = document.createElement("iframe");
      frame.hidden = true;
      frame.src =
        "omfp-notify:?" + new URLSearchParams({ n: NONCE, title: this.title, body: this.body, tag: this.tag });
      (document.body ?? document.documentElement).append(frame);
      setTimeout(() => frame.remove(), 1000);
    }
    close() {}
  }
  Object.defineProperty(window, "Notification", { value: Notification, configurable: true, writable: true });
})();"#;

/// A notification of the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    /// The page's `tag`: a notification with the same tag replaces the previous one (empty: none).
    pub tag: String,
}

/// What the window does with an `omfp-notify:` navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Navigation {
    /// A notification of [`notification_script`]: cancelled and posted.
    Notify(Notice),
    /// Without the window's nonce: not from the daemon's page, cancelled.
    Deny,
}

/// Sorts an `omfp-notify:` navigation of the window (UI-252, DMN-182): a notification when it carries `nonce`, `None`
/// for any other URL.
#[must_use]
pub fn navigation(url: &Url, nonce: &str) -> Option<Navigation> {
    if url.scheme() != NOTIFY_SCHEME {
        return None;
    }
    let mut notice = Notice {
        title: String::new(),
        body: String::new(),
        tag: String::new(),
    };
    let mut signed = false;
    for (key, value) in url.query_pairs() {
        match &*key {
            "n" => signed = value == nonce,
            "title" => notice.title = value.into_owned(),
            "body" => notice.body = value.into_owned(),
            "tag" => notice.tag = value.into_owned(),
            _ => {}
        }
    }
    Some(if signed {
        Navigation::Notify(notice)
    } else {
        Navigation::Deny
    })
}

/// The window's handling of `omfp-notify:` navigations: posts the daemon page's notifications and cancels them all
/// (`Some(false)`); `None` for any other URL.
#[cfg(target_os = "macos")]
pub fn on_notify(url: &Url, nonce: &str) -> Option<bool> {
    if let Navigation::Notify(notice) = navigation(url, nonce)? {
        post(&notice);
    }
    Some(false)
}

/// Posts `notice` to the notification center as the app of the embedded Info.plist (`dev.omfp.desktop`). It is
/// `NSUserNotificationCenter`, deprecated since macOS 11, because its successor `UNUserNotificationCenter` needs the
/// process to run from an app bundle, and `omfp desktop` runs as a plain executable (also from `omfp.app`, whose
/// executable starts the one next to `web/`, DMN-008). macOS asks the user once whether `omfp` may show notifications.
/// Without a delegate (which would need `unsafe`) a click does not reach the page.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn post(notice: &Notice) {
    use objc2_foundation::{NSString, NSUserNotification, NSUserNotificationCenter};
    let notification = NSUserNotification::new();
    notification.setTitle(Some(&NSString::from_str(&notice.title)));
    notification.setInformativeText(Some(&NSString::from_str(&notice.body)));
    if !notice.tag.is_empty() {
        notification.setIdentifier(Some(&NSString::from_str(&notice.tag)));
    }
    NSUserNotificationCenter::defaultUserNotificationCenter().deliverNotification(&notification);
}

/// The name `AppKit` looks the application's icon up under (`NSImageNameApplicationIcon`).
#[cfg(target_os = "macos")]
const APP_ICON: &str = "NSApplicationIcon";

/// Makes the desktop icon (`icons/icon.png`, UI-222) the app's icon in the Dock, the app switcher and the About panel.
/// `AppKit` takes it from the main bundle, which for `omfp desktop` is the executable's directory (also from `omfp.app`,
/// whose executable starts `bin/.omfp-wrapped`), so macOS showed a folder; an image registered under the icon's name
/// before the app starts wins (`setApplicationIconImage` would need `unsafe`). Returns whether it was registered.
#[cfg(target_os = "macos")]
pub fn register_app_icon() -> bool {
    use objc2::AllocAnyThread as _;
    use objc2_app_kit::NSImage;
    use objc2_foundation::{NSData, NSString};
    let png = NSData::with_bytes(include_bytes!("../../icons/icon.png"));
    NSImage::initWithData(NSImage::alloc(), &png)
        .is_some_and(|icon| icon.setName(Some(&NSString::from_str(APP_ICON))))
}

/// The app menu's own entries (UI-281, UI-282), by their menu ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    NewWindow,
    Zoom(Zoom),
}

/// The app menu's own entries: id, title, key (Tauri's accelerator) and action, in the File menu (New Window) and
/// the View menu (the zoom items, Safari's titles and keys).
pub const MENU_ENTRIES: [(&str, &str, &str, MenuAction); 4] = [
    (
        "new-window",
        "New Window",
        "CmdOrCtrl+N",
        MenuAction::NewWindow,
    ),
    (
        "zoom-reset",
        "Actual Size",
        "CmdOrCtrl+0",
        MenuAction::Zoom(Zoom::Reset),
    ),
    (
        "zoom-in",
        "Zoom In",
        "CmdOrCtrl+=",
        MenuAction::Zoom(Zoom::In),
    ),
    (
        "zoom-out",
        "Zoom Out",
        "CmdOrCtrl+-",
        MenuAction::Zoom(Zoom::Out),
    ),
];

/// The action of the app menu's entry `id`; `None` for Tauri's predefined items.
#[must_use]
pub fn menu_action(id: &str) -> Option<MenuAction> {
    MENU_ENTRIES
        .iter()
        .find(|(entry, ..)| *entry == id)
        .map(|(.., action)| *action)
}

/// `builder` with the window's app menu (UI-281, UI-282): Tauri's default macOS menu (the app menu, File, Edit,
/// View, Window, Help) plus New Window (Cmd-N, `omfp` with `new_window`) atop File and Actual Size, Zoom In, Zoom Out
/// (Cmd-0, Cmd-=, Cmd--) atop View, which set the page zoom of the window ([`zoomed`]).
#[cfg(target_os = "macos")]
pub fn with_app_menu(
    builder: tauri::Builder<tauri::Wry>,
    new_window: Vec<OsString>,
) -> tauri::Builder<tauri::Wry> {
    let zoom = Mutex::new(1.0);
    builder
        .menu(app_menu)
        .on_menu_event(move |app, event| match menu_action(event.id().as_ref()) {
            Some(MenuAction::NewWindow) => match std::env::current_exe() {
                Ok(exe) => crate::tray::spawn_detached(
                    std::process::Command::new(exe).args(&new_window),
                    "omfp desktop",
                    "omfp desktop",
                ),
                Err(error) => {
                    eprintln!("omfp desktop: no new window, cannot find this executable: {error}");
                }
            },
            Some(MenuAction::Zoom(step)) => {
                use tauri::Manager as _;
                let Some(window) = app.get_webview_window("main") else {
                    return;
                };
                let mut level = zoom.lock().unwrap_or_else(PoisonError::into_inner);
                let next = zoomed(*level, step);
                match window.set_zoom(next) {
                    Ok(()) => *level = next,
                    Err(error) => eprintln!("omfp desktop: cannot zoom: {error}"),
                }
            }
            None => {}
        })
}

/// Tauri's default macOS menu (the app menu, File, Edit, View, Window, Help) with the entries of [`MENU_ENTRIES`] atop
/// File and View.
#[cfg(target_os = "macos")]
fn app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{
        AboutMetadata, HELP_SUBMENU_ID, Menu, MenuItem, PredefinedMenuItem as Item, Submenu,
        WINDOW_SUBMENU_ID,
    };
    let [new_window, zoom_reset, zoom_in, zoom_out] =
        MENU_ENTRIES.map(|(id, title, key, _)| MenuItem::with_id(app, id, title, true, Some(key)));
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        ..AboutMetadata::default()
    };
    let separator = || Item::separator(app);
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                &info.name,
                true,
                &[
                    &Item::about(app, None, Some(about))?,
                    &separator()?,
                    &Item::services(app, None)?,
                    &separator()?,
                    &Item::hide(app, None)?,
                    &Item::hide_others(app, None)?,
                    &separator()?,
                    &Item::quit(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "File",
                true,
                &[&new_window?, &separator()?, &Item::close_window(app, None)?],
            )?,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &Item::undo(app, None)?,
                    &Item::redo(app, None)?,
                    &separator()?,
                    &Item::cut(app, None)?,
                    &Item::copy(app, None)?,
                    &Item::paste(app, None)?,
                    &Item::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "View",
                true,
                &[
                    &zoom_reset?,
                    &zoom_in?,
                    &zoom_out?,
                    &separator()?,
                    &Item::fullscreen(app, None)?,
                ],
            )?,
            &Submenu::with_id_and_items(
                app,
                WINDOW_SUBMENU_ID,
                "Window",
                true,
                &[
                    &Item::minimize(app, None)?,
                    &Item::maximize(app, None)?,
                    &separator()?,
                    &Item::close_window(app, None)?,
                ],
            )?,
            &Submenu::with_id_and_items(app, HELP_SUBMENU_ID, "Help", true, &[])?,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: UI-281, UI-282
    #[test]
    fn the_app_menu_entries_act_by_id_on_the_macos_keys() {
        let keys: Vec<(&str, Option<MenuAction>)> = MENU_ENTRIES
            .iter()
            .map(|(id, _, key, _)| (*key, menu_action(id)))
            .collect();
        assert_eq!(
            keys,
            [
                ("CmdOrCtrl+N", Some(MenuAction::NewWindow)),
                ("CmdOrCtrl+0", Some(MenuAction::Zoom(Zoom::Reset))),
                ("CmdOrCtrl+=", Some(MenuAction::Zoom(Zoom::In))),
                ("CmdOrCtrl+-", Some(MenuAction::Zoom(Zoom::Out))),
            ]
        );
        // Tauri's predefined items (Close Window, Quit, …) are not ours.
        assert_eq!(menu_action("quit"), None);
    }

    // spec: UI-250
    #[test]
    fn the_data_store_identifier_is_made_once_per_data_directory() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first/webview");
        let id = data_store_identifier(&first).unwrap();
        assert_eq!(std::fs::read(first.join(STORE_ID)).unwrap(), id.to_vec());
        assert_eq!(data_store_identifier(&first).unwrap(), id);
        let other = data_store_identifier(&dir.path().join("other/webview")).unwrap();
        assert_ne!(other, id);
        // A damaged file is replaced.
        std::fs::write(first.join(STORE_ID), b"short").unwrap();
        let replaced = data_store_identifier(&first).unwrap();
        assert_eq!(
            std::fs::read(first.join(STORE_ID)).unwrap(),
            replaced.to_vec()
        );
    }

    // spec: UI-252, DMN-182
    #[test]
    fn a_notify_navigation_with_the_windows_nonce_carries_the_pages_notification() {
        let nonce = notify_nonce();
        assert_eq!(nonce.len(), 32);
        assert_ne!(notify_nonce(), nonce);
        let url = Url::parse(&format!(
            "omfp-notify:?n={nonce}&title=Fix+the+tests&body=needs+you+%26+waits&tag=t1&other=x"
        ))
        .unwrap();
        assert_eq!(
            navigation(&url, &nonce),
            Some(Navigation::Notify(Notice {
                title: "Fix the tests".into(),
                body: "needs you & waits".into(),
                tag: "t1".into(),
            }))
        );
        assert_eq!(
            navigation(
                &Url::parse(&format!("omfp-notify:?n={nonce}")).unwrap(),
                &nonce
            ),
            Some(Navigation::Notify(Notice {
                title: String::new(),
                body: String::new(),
                tag: String::new(),
            }))
        );
        // A frame of another origin does not know the nonce.
        for forged in [
            "omfp-notify:?title=Phish",
            "omfp-notify:?n=0000&title=Phish",
        ] {
            assert_eq!(
                navigation(&Url::parse(forged).unwrap(), &nonce),
                Some(Navigation::Deny),
                "{forged}"
            );
        }
        let page = Url::parse("http://127.0.0.1:7420/#token=t").unwrap();
        assert_eq!(navigation(&page, &nonce), None);
    }

    // spec: UI-252, DMN-182
    #[test]
    fn the_notification_script_runs_only_in_the_daemons_top_level_page() {
        let script = notification_script("http://127.0.0.1:7420", "ab\"c");
        assert!(script.starts_with(
            "(() => {\n  const ORIGIN = \"http://127.0.0.1:7420\";\n  const NONCE = \"ab\\\"c\";\n"
        ));
        assert!(
            script.contains("if (window.top !== window || location.origin !== ORIGIN) return;")
        );
        assert!(script.contains("n: NONCE"));
        assert!(script.ends_with("})();"));
    }

    // spec: UI-222
    #[cfg(target_os = "macos")]
    #[test]
    fn the_app_icon_is_the_desktop_icon_not_the_executables_folder() {
        use objc2_app_kit::NSImage;
        use objc2_foundation::NSString;
        assert!(register_app_icon());
        let icon = NSImage::imageNamed(&NSString::from_str(APP_ICON)).expect("the app icon");
        // icons/icon.png is 512 px; the folder icon AppKit falls back to is 128 pt.
        let size = icon.size();
        assert!((size.width - 512.0).abs() < f64::EPSILON, "{size:?}");
        assert!((size.height - 512.0).abs() < f64::EPSILON, "{size:?}");
    }
}
