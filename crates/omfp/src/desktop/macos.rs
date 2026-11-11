//! The window's macOS parts (M11, S-15): its web storage per data directory (UI-250) and the page's notifications
//! through the system's notification center (UI-252). The pure parts also build in Linux tests.

use std::hash::{BuildHasher as _, Hasher as _, RandomState};
use std::path::Path;

use tauri::Url;

/// The URL scheme of the page's notifications: the window cancels the navigation and posts the notification.
const NOTIFY_SCHEME: &str = "omfp-notify";

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
    // Each `RandomState` has its own random keys: two hashes of nothing are 16 random bytes.
    let random = || RandomState::new().build_hasher().finish().to_le_bytes();
    let mut id = [0; 16];
    id[..8].copy_from_slice(&random());
    id[8..].copy_from_slice(&random());
    std::fs::create_dir_all(webview_data)?;
    std::fs::write(&file, id)?;
    Ok(id)
}

/// The page's `Notification` in the window (UI-252): `WKWebView` denies web notifications, so the window replaces the
/// API before the page's scripts run. The permission is `granted`; each notification is a navigation of a hidden
/// frame to `omfp-notify:?title=…&body=…&tag=…`, which [`on_navigation`] cancels and posts.
#[cfg(target_os = "macos")]
pub const NOTIFICATION_SCRIPT: &str = r#"(() => {
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
      frame.src = "omfp-notify:?" + new URLSearchParams({ title: this.title, body: this.body, tag: this.tag });
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

/// What the window does with a navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Navigation {
    Allow,
    /// A notification of [`NOTIFICATION_SCRIPT`]: cancelled and posted.
    Notify(Notice),
}

/// Sorts a navigation of the window (UI-252): an `omfp-notify:` URL is a notification, everything else navigates.
#[must_use]
pub fn navigation(url: &Url) -> Navigation {
    if url.scheme() != NOTIFY_SCHEME {
        return Navigation::Allow;
    }
    let mut notice = Notice {
        title: String::new(),
        body: String::new(),
        tag: String::new(),
    };
    for (key, value) in url.query_pairs() {
        match &*key {
            "title" => notice.title = value.into_owned(),
            "body" => notice.body = value.into_owned(),
            "tag" => notice.tag = value.into_owned(),
            _ => {}
        }
    }
    Navigation::Notify(notice)
}

/// The window's navigation handler: posts the page's notifications, lets every other navigation through.
#[cfg(target_os = "macos")]
pub fn on_navigation(url: &Url) -> bool {
    match navigation(url) {
        Navigation::Allow => true,
        Navigation::Notify(notice) => {
            post(&notice);
            false
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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

    // spec: UI-252
    #[test]
    fn a_notify_navigation_carries_the_pages_notification() {
        let url =
            Url::parse("omfp-notify:?title=Fix+the+tests&body=needs+you+%26+waits&tag=t1&other=x")
                .unwrap();
        assert_eq!(
            navigation(&url),
            Navigation::Notify(Notice {
                title: "Fix the tests".into(),
                body: "needs you & waits".into(),
                tag: "t1".into(),
            })
        );
        assert_eq!(
            navigation(&Url::parse("omfp-notify:").unwrap()),
            Navigation::Notify(Notice {
                title: String::new(),
                body: String::new(),
                tag: String::new(),
            })
        );
        let page = Url::parse("http://127.0.0.1:7420/#token=t").unwrap();
        assert_eq!(navigation(&page), Navigation::Allow);
    }
}
