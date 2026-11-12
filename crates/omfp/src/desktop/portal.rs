//! The session's color scheme through the settings portal (UI-195, Linux).

use std::cell::Cell;
use std::rc::Rc;

use gio::glib::{ToVariant as _, Variant, VariantTy};
use tauri::Theme as WindowTheme;
use tauri::WebviewWindow;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME: &str = "color-scheme";

/// The window theme for the settings portal's `org.freedesktop.appearance color-scheme` (UI-195): 1 prefers dark,
/// 2 prefers light, 0 (no preference) and unknown values leave the toolkit's default.
#[must_use]
pub fn portal_theme(color_scheme: u32) -> Option<WindowTheme> {
    match color_scheme {
        1 => Some(WindowTheme::Dark),
        2 => Some(WindowTheme::Light),
        _ => None,
    }
}

/// The `u` of a portal value however it is wrapped: `ReadOne` answers `(v)`, the older `Read` `(v)` holding a `v`,
/// `SettingChanged`'s `(ssv)` carries a `v` ([`setting_changed`]). Only a `v` is opened as one: `g_variant_get_variant`
/// on anything else is a `GLib-CRITICAL`.
#[must_use]
pub fn portal_value(value: &Variant) -> Option<u32> {
    if let Some(number) = value.get::<u32>() {
        return Some(number);
    }
    if value.is_type(VariantTy::VARIANT) {
        return value.as_variant().and_then(|inner| portal_value(&inner));
    }
    value
        .is_type(VariantTy::new("(v)").expect("a valid type string"))
        .then(|| portal_value(&value.child_value(0)))
        .flatten()
}

/// The color scheme a `SettingChanged(namespace, key, value)` signal (`(ssv)`) reports, if it is about it.
#[must_use]
pub fn setting_changed(parameters: &Variant) -> Option<Option<u32>> {
    let (namespace, key, value) = parameters.get::<(String, String, Variant)>()?;
    (namespace == APPEARANCE && key == COLOR_SCHEME).then(|| portal_value(&value))
}

/// Follows the session's color scheme through the settings portal (UI-195): `WebKitGTK` 4.1 (GTK3) never reads it, so
/// the page's `prefers-color-scheme` stayed light under a dark desktop. The window's theme (GTK's
/// `gtk-application-prefer-dark-theme`, which `WebKitGTK` maps to `prefers-color-scheme`) is set from the portal's value
/// at startup and on every `SettingChanged`. Without a session bus or portal nothing changes. Runs on the GTK main
/// thread, whose main loop delivers the replies and signals; the subscription lasts as long as the returned value.
pub(super) fn follow_color_scheme(window: &WebviewWindow) -> Option<ColorScheme> {
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()?;
    // Subscribed before reading, so no change between the read and the subscription is lost; a change that arrives
    // before the read's answer wins over it.
    let changed = Rc::new(Cell::new(false));
    let subscription = connection.signal_subscribe(
        Some(PORTAL),
        Some(PORTAL_SETTINGS),
        Some("SettingChanged"),
        Some(PORTAL_PATH),
        Some(APPEARANCE),
        gio::DBusSignalFlags::NONE,
        {
            let window = window.clone();
            let changed = Rc::clone(&changed);
            move |_, _, _, _, _, parameters| {
                if let Some(value) = setting_changed(parameters) {
                    changed.set(true);
                    let _ = window.set_theme(value.and_then(portal_theme));
                }
            }
        },
    );
    read_color_scheme(&connection, "ReadOne", {
        let window = window.clone();
        let connection = connection.clone();
        move |value| match value {
            Some(value) => apply_read(&window, &changed, &value),
            // Portals before version 2 only have `Read`.
            None => read_color_scheme(&connection, "Read", move |value| {
                if let Some(value) = value {
                    apply_read(&window, &changed, &value);
                }
            }),
        }
    });
    Some(ColorScheme {
        connection,
        subscription: Some(subscription),
    })
}

fn read_color_scheme(
    connection: &gio::DBusConnection,
    method: &str,
    done: impl FnOnce(Option<Variant>) + 'static,
) {
    connection.call(
        Some(PORTAL),
        PORTAL_PATH,
        PORTAL_SETTINGS,
        method,
        Some(&(APPEARANCE, COLOR_SCHEME).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        move |reply| done(reply.ok()),
    );
}

fn apply_read(window: &WebviewWindow, changed: &Cell<bool>, value: &Variant) {
    if !changed.get()
        && let Some(theme) = portal_value(value).and_then(portal_theme)
    {
        let _ = window.set_theme(Some(theme));
    }
}

/// The portal subscription of [`follow_color_scheme`].
pub(super) struct ColorScheme {
    connection: gio::DBusConnection,
    subscription: Option<gio::SignalSubscriptionId>,
}

impl Drop for ColorScheme {
    fn drop(&mut self) {
        if let Some(subscription) = self.subscription.take() {
            self.connection.signal_unsubscribe(subscription);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    // spec: UI-195
    #[test]
    fn the_portal_color_scheme_picks_the_window_theme() {
        assert_eq!(portal_theme(1), Some(WindowTheme::Dark));
        assert_eq!(portal_theme(2), Some(WindowTheme::Light));
        // No preference, or a value the portal may add later: the toolkit's default.
        assert_eq!(portal_theme(0), None);
        assert_eq!(portal_theme(3), None);

        assert_eq!(portal_value(&"dark".to_variant()), None);
        assert_eq!(portal_value(&(1u32, 2u32).to_variant()), None);
        assert_eq!(portal_value(&parsed("(s)", "('dark',)")), None);
    }

    /// A variant of `type_` from its text form.
    fn parsed(type_: &str, text: &str) -> Variant {
        Variant::parse(Some(VariantTy::new(type_).unwrap()), text).unwrap()
    }

    /// The `GLib-CRITICAL`s logged on this thread while `run` runs (a failed `g_return_if_fail`).
    fn glib_critical_logs<T>(run: impl FnOnce() -> T) -> (T, Vec<String>) {
        let logged = Arc::new(Mutex::new(Vec::new()));
        let thread = std::thread::current().id();
        let handler = gio::glib::log_set_handler(
            Some("GLib"),
            gio::glib::LogLevels::LEVEL_CRITICAL,
            false,
            false,
            {
                let logged = Arc::clone(&logged);
                move |_, _, message| {
                    if std::thread::current().id() == thread {
                        logged.lock().unwrap().push(message.to_owned());
                    }
                }
            },
        );
        let answer = run();
        gio::glib::log_remove_handler(Some("GLib"), handler);
        let logged = logged.lock().unwrap().clone();
        (answer, logged)
    }

    // spec: UI-195
    #[test]
    fn read_one_answers_a_v_in_a_tuple() {
        let (value, critical_logs) =
            glib_critical_logs(|| portal_value(&parsed("(v)", "(<uint32 1>,)")));
        assert_eq!(value, Some(1));
        assert_eq!(critical_logs, Vec::<String>::new());
    }

    // spec: UI-195
    #[test]
    fn read_answers_a_v_holding_a_v_in_a_tuple() {
        let (value, critical_logs) =
            glib_critical_logs(|| portal_value(&parsed("(v)", "(<<uint32 2>>,)")));
        assert_eq!(value, Some(2));
        assert_eq!(critical_logs, Vec::<String>::new());
    }

    // spec: UI-195
    #[test]
    fn setting_changed_carries_the_v_of_its_key() {
        let (values, critical_logs) = glib_critical_logs(|| {
            [
                "('org.freedesktop.appearance', 'color-scheme', <uint32 1>)",
                "('org.freedesktop.appearance', 'color-scheme', <<uint32 2>>)",
                "('org.freedesktop.appearance', 'color-scheme', <'dark'>)",
                "('org.freedesktop.appearance', 'accent-color', <uint32 1>)",
                "('org.gnome.desktop.interface', 'color-scheme', <uint32 1>)",
            ]
            .map(|text| setting_changed(&parsed("(ssv)", text)))
        });
        assert_eq!(
            values,
            [Some(Some(1)), Some(Some(2)), Some(None), None, None]
        );
        assert_eq!(critical_logs, Vec::<String>::new());
        assert_eq!(setting_changed(&parsed("(ss)", "('a', 'b')")), None);
    }
}
