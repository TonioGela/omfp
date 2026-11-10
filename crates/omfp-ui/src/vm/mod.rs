//! View models: pure functions from [`crate::ViewInput`] and [`crate::UiState`] to display data (UI-011). No DOM, no
//! signals; components only render what these return.

pub mod composer;
pub mod connection;
pub mod header;
pub mod interaction;
pub mod new_thread;
pub mod notice;
pub mod queue;
pub mod threads;
pub mod tool;
pub mod transcript;
pub mod virtual_list;
pub mod workspaces;

/// `text` cut to `max` characters with an ellipsis, on one line.
#[must_use]
pub fn one_line(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut cut: String = flat.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// Pretty-printed JSON.
#[must_use]
pub fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_flattens_whitespace_and_cuts_with_an_ellipsis() {
        assert_eq!(one_line("a\n  b\tc", 10), "a b c");
        assert_eq!(one_line("abcdefghij", 5), "abcd…");
    }
}
