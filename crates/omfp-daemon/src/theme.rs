//! The user's theme (DMN-075): CSS the daemon keeps as a setting, serves at `/theme.css` and pushes on the `index`
//! topic. Besides `theme.set`, a file on the daemon's host sets it: `$XDG_CONFIG_HOME/omfp/theme.css` (else
//! `~/.config/omfp/theme.css`), read at startup and polled for changes ([`crate::watch`]). The last change wins,
//! whether it came from the file or from `theme.set`.

use std::path::PathBuf;

/// Largest theme accepted, in bytes; bigger ones are `bad_request` (`theme.set`) or ignored (the file).
pub const MAX_THEME_BYTES: usize = 1024 * 1024;

/// The theme file's default location: `$XDG_CONFIG_HOME/omfp/theme.css` when absolute, else
/// `$HOME/.config/omfp/theme.css`.
#[must_use]
pub fn default_theme_file(
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    crate::config::config_dir(xdg_config_home, home).map(|dir| dir.join("theme.css"))
}

/// Why a theme is refused.
#[must_use]
pub fn too_large(css: &str) -> Option<String> {
    (css.len() > MAX_THEME_BYTES).then(|| {
        format!(
            "the theme is {} bytes; at most {MAX_THEME_BYTES} are accepted",
            css.len()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-075
    #[test]
    fn the_theme_file_lives_in_the_config_directory() {
        assert_eq!(
            default_theme_file(Some("/xdg".into()), Some("/home/u".into())),
            Some(PathBuf::from("/xdg/omfp/theme.css"))
        );
        assert_eq!(
            default_theme_file(Some("relative".into()), Some("/home/u".into())),
            Some(PathBuf::from("/home/u/.config/omfp/theme.css"))
        );
        assert_eq!(default_theme_file(None, None), None);
    }

    // spec: DMN-075
    #[test]
    fn a_theme_over_one_mebibyte_is_refused() {
        assert_eq!(too_large(&"a".repeat(MAX_THEME_BYTES)), None);
        assert!(too_large(&"a".repeat(MAX_THEME_BYTES + 1)).is_some());
    }
}
