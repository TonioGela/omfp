//! The config file (DMN-009): `[daemon]` holds the `omfp daemon` options (merged in [`crate::config::Config`]),
//! `[ui]` and `[keymap]` the UI settings (UI-097) the daemon serves with `settings.get` and writes back with
//! `settings.set`, keeping comments and the other keys ([`write_back`]).
//!
//! ```toml
//! [daemon]
//! bind = "127.0.0.1:7420"
//! title_generation = false
//!
//! [ui]
//! density = "normal"            # compact | normal
//! notifications = "background"  # background | always | never
//!
//! [ui.fonts]
//! prose = "Literata, serif"
//!
//! [keymap]                      # normal mode
//! "Space t" = "theme.set"
//! "g g" = ""                    # removes the default binding
//!
//! [keymap.insert]
//! "Ctrl-j" = "nav.next"
//! ```

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use omfp_protocol::{
    Density, FontSettings, KeymapEntry, KeymapMode, NotificationMode, SetSettingsParams, Settings,
    UiSettings,
};
use serde::Deserialize;
use toml_edit::{DocumentMut, Item, Table};

/// The `[daemon]` table: every `omfp daemon` option under the flag's name in snake case. Paths may start with `~`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonOptions {
    pub bind: Option<SocketAddr>,
    pub data_dir: Option<PathBuf>,
    pub workspaces_root: Option<PathBuf>,
    pub omp_bin: Option<PathBuf>,
    pub static_dir: Option<PathBuf>,
    pub allow_omp_mismatch: Option<bool>,
    pub idle_timeout_minutes: Option<u64>,
    pub max_processes: Option<usize>,
    pub event_buffer: Option<usize>,
    pub allow_origin: Option<Vec<String>>,
    pub theme_file: Option<PathBuf>,
    /// `false` is `--no-title-generation`.
    pub title_generation: Option<bool>,
}

impl DaemonOptions {
    /// The options with `~` in paths replaced by `home`.
    #[must_use]
    pub fn expanded(self, home: Option<&Path>) -> Self {
        let expand = |path: Option<PathBuf>| path.map(|path| expand_home(path, home));
        Self {
            data_dir: expand(self.data_dir),
            workspaces_root: expand(self.workspaces_root),
            omp_bin: expand(self.omp_bin),
            static_dir: expand(self.static_dir),
            theme_file: expand(self.theme_file),
            ..self
        }
    }
}

/// `~` and `~/…` start at `home`; other paths (and every path without a home) stay as they are.
#[must_use]
pub fn expand_home(path: PathBuf, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return path;
    };
    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path,
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    daemon: DaemonOptions,
    #[serde(default)]
    ui: RawUi,
    #[serde(default)]
    keymap: toml::Table,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUi {
    density: Option<Density>,
    notifications: Option<NotificationMode>,
    fonts: Option<RawFonts>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFonts {
    prose: Option<String>,
    ui: Option<String>,
    code: Option<String>,
}

/// A parsed config file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigFile {
    pub daemon: DaemonOptions,
    pub ui: UiSettings,
    /// `[keymap]` (normal mode), then `[keymap.insert]`, then `[keymap.cmdline]`, each sorted by keys.
    pub keymap: Vec<KeymapEntry>,
}

impl ConfigFile {
    /// Parses the file's text; an unknown key or a wrong value is an error naming it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let raw: RawFile = toml::from_str(text).map_err(|error| error.to_string())?;
        let fonts = raw.ui.fonts.map(|fonts| FontSettings {
            prose: fonts.prose,
            ui: fonts.ui,
            code: fonts.code,
        });
        Ok(Self {
            daemon: raw.daemon,
            ui: UiSettings {
                density: raw.ui.density,
                notifications: raw.ui.notifications,
                fonts,
            },
            keymap: keymap(&raw.keymap)?,
        })
    }

    /// Reads and parses `path`; `None` when it does not exist. Errors name the file.
    pub fn read(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text)
                .map(Some)
                .map_err(|error| format!("{}: {error}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("{}: {error}", path.display())),
        }
    }

    /// The UI settings the file holds, without error.
    #[must_use]
    pub fn settings(&self) -> Settings {
        Settings {
            ui: self.ui.clone(),
            keymap: self.keymap.clone(),
            error: None,
        }
    }
}

const MODE_TABLES: [(&str, KeymapMode); 2] = [
    ("insert", KeymapMode::Insert),
    ("cmdline", KeymapMode::Cmdline),
];

/// The bindings of `[keymap]`: string values are normal-mode bindings, `insert` and `cmdline` sub-tables the other
/// modes.
fn keymap(table: &toml::Table) -> Result<Vec<KeymapEntry>, String> {
    let mut entries = Vec::new();
    let mut modes = Vec::new();
    for (keys, value) in table {
        match value {
            toml::Value::String(command) => entries.push(KeymapEntry {
                mode: KeymapMode::Normal,
                keys: keys.clone(),
                command: command.clone(),
            }),
            toml::Value::Table(bindings) => {
                let Some((_, mode)) = MODE_TABLES.iter().find(|(name, _)| name == keys) else {
                    return Err(format!(
                        "unknown keymap table `keymap.{keys}`, expected `keymap.insert` or `keymap.cmdline`"
                    ));
                };
                modes.push((*mode, keys, bindings));
            }
            _ => {
                return Err(format!("`keymap.\"{keys}\"` must be a command id string"));
            }
        }
    }
    for (mode, name, bindings) in modes {
        for (keys, value) in bindings {
            let Some(command) = value.as_str() else {
                return Err(format!(
                    "`keymap.{name}.\"{keys}\"` must be a command id string"
                ));
            };
            entries.push(KeymapEntry {
                mode,
                keys: keys.clone(),
                command: command.to_owned(),
            });
        }
    }
    Ok(entries)
}

/// `update` merged into the config file's `text` (empty when there is none): only the given values change; comments,
/// other keys and their order stay. The result is checked to parse.
pub fn write_back(text: &str, update: &SetSettingsParams) -> Result<String, String> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|error| format!("the config file is not valid TOML: {error}"))?;
    if let Some(ui) = &update.ui {
        let table = child(doc.as_table_mut(), "ui")?;
        if let Some(density) = ui.density {
            set(table, "density", density_name(density));
        }
        if let Some(notifications) = ui.notifications {
            set(table, "notifications", notifications_name(notifications));
        }
        if let Some(fonts) = &ui.fonts {
            let table = child(table, "fonts")?;
            for (key, value) in [
                ("prose", &fonts.prose),
                ("ui", &fonts.ui),
                ("code", &fonts.code),
            ] {
                if let Some(value) = value {
                    set(table, key, value);
                }
            }
        }
    }
    for entry in update.keymap.iter().flatten() {
        if entry.keys.trim().is_empty() {
            return Err("a keymap entry needs keys".into());
        }
        let table = child(doc.as_table_mut(), "keymap")?;
        let table = match entry.mode {
            KeymapMode::Normal => table,
            KeymapMode::Insert => child(table, "insert")?,
            KeymapMode::Cmdline => child(table, "cmdline")?,
        };
        set(table, &entry.keys, &entry.command);
    }
    let text = doc.to_string();
    ConfigFile::parse(&text)?;
    Ok(text)
}

/// The sub-table `key` of `parent`, created (as a `[header]` once it holds values) when missing.
fn child<'a>(parent: &'a mut Table, key: &str) -> Result<&'a mut Table, String> {
    parent
        .entry(key)
        .or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        })
        .as_table_mut()
        .ok_or_else(|| format!("`{key}` in the config file is not a table"))
}

/// Sets `key` to the string `value`, keeping the comments around an existing value.
fn set(table: &mut Table, key: &str, value: &str) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(existing) => {
            let decor = existing.decor().clone();
            *existing = value.into();
            *existing.decor_mut() = decor;
        }
        None => table[key] = toml_edit::value(value),
    }
}

fn density_name(density: Density) -> &'static str {
    match density {
        Density::Compact => "compact",
        Density::Normal => "normal",
    }
}

fn notifications_name(mode: NotificationMode) -> &'static str {
    match mode {
        NotificationMode::Background => "background",
        NotificationMode::Always => "always",
        NotificationMode::Never => "never",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"# My omfp.
[daemon]
max_processes = 4 # enough

[ui]
# Roomier rows.
density = "compact" # for now
[ui.fonts]
prose = "Literata, serif"

[keymap]
"Space t" = "theme.set"
"g g" = ""

[keymap.insert]
"Ctrl-j" = "nav.next"
"#;

    fn entry(mode: KeymapMode, keys: &str, command: &str) -> KeymapEntry {
        KeymapEntry {
            mode,
            keys: keys.into(),
            command: command.into(),
        }
    }

    // spec: DMN-009, UI-097
    #[test]
    fn the_file_holds_daemon_options_ui_settings_and_keymap_entries() {
        let file = ConfigFile::parse(FILE).unwrap();

        assert_eq!(file.daemon.max_processes, Some(4));
        assert_eq!(
            file.ui,
            UiSettings {
                density: Some(Density::Compact),
                notifications: None,
                fonts: Some(FontSettings {
                    prose: Some("Literata, serif".into()),
                    ui: None,
                    code: None,
                }),
            }
        );
        assert_eq!(
            file.keymap,
            [
                entry(KeymapMode::Normal, "Space t", "theme.set"),
                entry(KeymapMode::Normal, "g g", ""),
                entry(KeymapMode::Insert, "Ctrl-j", "nav.next"),
            ]
        );
        assert_eq!(ConfigFile::parse("").unwrap(), ConfigFile::default());
    }

    // spec: DMN-009
    #[test]
    fn unknown_keys_and_wrong_values_are_errors_naming_them() {
        for (text, named) in [
            ("[daemon]\nbnid = \"x\"", "bnid"),
            ("[colors]\na = 1", "colors"),
            ("[ui]\ndensty = \"normal\"", "densty"),
            ("[ui]\ndensity = \"huge\"", "huge"),
            ("[ui.fonts]\nserif = \"x\"", "serif"),
            ("[keymap.visual]\nj = \"nav.next\"", "keymap.visual"),
            ("[keymap]\nj = 1", "keymap.\"j\""),
            ("[keymap.insert]\nj = true", "keymap.insert.\"j\""),
            ("[daemon]\nbind = \"not an address\"", "bind"),
        ] {
            let error = ConfigFile::parse(text).unwrap_err();
            assert!(error.contains(named), "{text}: {error}");
        }
    }

    // spec: DMN-009
    #[test]
    fn a_tilde_starts_at_home() {
        let home = Path::new("/home/u");
        assert_eq!(
            expand_home("~/data".into(), Some(home)),
            PathBuf::from("/home/u/data")
        );
        assert_eq!(
            expand_home("~".into(), Some(home)),
            PathBuf::from("/home/u")
        );
        assert_eq!(
            expand_home("/abs".into(), Some(home)),
            PathBuf::from("/abs")
        );
        assert_eq!(
            expand_home("~x/a".into(), Some(home)),
            PathBuf::from("~x/a")
        );
        assert_eq!(expand_home("~/data".into(), None), PathBuf::from("~/data"));
    }

    // spec: DMN-009, UI-097
    #[test]
    fn write_back_changes_only_the_given_values_and_keeps_comments() {
        let update = SetSettingsParams {
            ui: Some(UiSettings {
                density: Some(Density::Normal),
                notifications: Some(NotificationMode::Never),
                fonts: Some(FontSettings {
                    prose: None,
                    ui: None,
                    code: Some("\"Iosevka\", monospace".into()),
                }),
            }),
            keymap: Some(vec![
                entry(KeymapMode::Normal, "Space t", "density.set"),
                entry(KeymapMode::Cmdline, "Ctrl-p", "nav.prev"),
            ]),
        };

        let text = write_back(FILE, &update).unwrap();

        assert_eq!(
            text,
            r#"# My omfp.
[daemon]
max_processes = 4 # enough

[ui]
# Roomier rows.
density = "normal" # for now
notifications = "never"
[ui.fonts]
prose = "Literata, serif"
code = '"Iosevka", monospace'

[keymap]
"Space t" = "density.set"
"g g" = ""

[keymap.insert]
"Ctrl-j" = "nav.next"

[keymap.cmdline]
Ctrl-p = "nav.prev"
"#
        );
    }

    // spec: DMN-009
    #[test]
    fn write_back_creates_the_tables_of_an_empty_file_and_refuses_broken_ones() {
        let update = SetSettingsParams {
            ui: Some(UiSettings {
                density: Some(Density::Compact),
                ..UiSettings::default()
            }),
            keymap: Some(vec![entry(KeymapMode::Insert, "Ctrl-j", "")]),
        };
        let text = write_back("", &update).unwrap();
        let file = ConfigFile::parse(&text).unwrap();
        assert_eq!(file.ui.density, Some(Density::Compact));
        assert_eq!(file.keymap, [entry(KeymapMode::Insert, "Ctrl-j", "")]);

        assert!(write_back("[ui", &update).is_err());
        assert!(write_back("ui = 1", &update).is_err());
        let no_keys = SetSettingsParams {
            ui: None,
            keymap: Some(vec![entry(KeymapMode::Normal, " ", "x")]),
        };
        assert!(write_back("", &no_keys).is_err());
    }
}
