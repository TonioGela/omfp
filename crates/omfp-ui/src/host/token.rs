//! The access token on the web (UI-002): `omfp daemon` prints the UI's URL with `#token=…`; the page reads it, keeps it
//! in `localStorage`, and removes it from the address bar so it does not end up in history, bookmarks or screen
//! shares. Fragments never reach the server (PRT-002).

/// The `token` parameter of a URL fragment (`#token=…`, possibly among other `&`-separated parameters).
#[must_use]
pub fn fragment_token(fragment: &str) -> Option<String> {
    parameters(fragment)
        .find_map(|parameter| parameter.strip_prefix("token="))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

/// The fragment without its `token` parameter: `""` when nothing else is left, else `#` and the other parameters.
#[must_use]
pub fn fragment_without_token(fragment: &str) -> String {
    let rest: Vec<&str> = parameters(fragment)
        .filter(|parameter| *parameter != "token" && !parameter.starts_with("token="))
        .collect();
    if rest.is_empty() {
        String::new()
    } else {
        format!("#{}", rest.join("&"))
    }
}

fn parameters(fragment: &str) -> impl Iterator<Item = &str> {
    fragment
        .strip_prefix('#')
        .unwrap_or(fragment)
        .split('&')
        .filter(|parameter| !parameter.is_empty())
}

/// The token to connect with: a fresh one from the address bar wins over the stored one.
#[must_use]
pub fn choose(from_fragment: Option<String>, stored: Option<String>) -> Option<String> {
    from_fragment.or(stored.filter(|token| !token.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: UI-002
    #[test]
    fn the_token_is_read_from_the_fragment_and_removed_from_it() {
        assert_eq!(fragment_token("#token=abc123"), Some("abc123".into()));
        assert_eq!(fragment_without_token("#token=abc123"), "");
        assert_eq!(
            fragment_token("#thread=t1&token=abc123"),
            Some("abc123".into())
        );
        assert_eq!(
            fragment_without_token("#thread=t1&token=abc123"),
            "#thread=t1"
        );
        assert_eq!(fragment_token("#token="), None);
        assert_eq!(fragment_token(""), None);
        assert_eq!(fragment_without_token(""), "");
        assert_eq!(fragment_token("#tokens=abc"), None);
        assert_eq!(fragment_without_token("#tokens=abc"), "#tokens=abc");
    }

    // spec: UI-002
    #[test]
    fn a_token_from_the_address_bar_replaces_the_stored_one() {
        assert_eq!(
            choose(Some("new".into()), Some("old".into())),
            Some("new".into())
        );
        assert_eq!(choose(None, Some("old".into())), Some("old".into()));
        assert_eq!(choose(None, Some("  ".into())), None);
        assert_eq!(choose(None, None), None);
    }
}
