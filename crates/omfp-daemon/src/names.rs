//! Names the daemon derives: folder and branch slugs, and unique variants of a wanted name.

/// Turns free text into a directory/branch-friendly slug: lowercase ASCII letters and digits separated by single
/// dashes, at most 40 characters; `workspace` when nothing is left.
#[must_use]
pub fn slugify(text: &str) -> String {
    const MAX_LEN: usize = 40;
    let mut slug = String::new();
    for c in text.chars() {
        if slug.len() == MAX_LEN {
            break;
        }
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "workspace".to_owned()
    } else {
        slug.to_owned()
    }
}

/// The first of `base`, `base<sep>2`, `base<sep>3`, … that is not `taken`.
pub fn unique<E>(
    base: &str,
    sep: &str,
    mut taken: impl FnMut(&str) -> Result<bool, E>,
) -> Result<String, E> {
    let mut candidate = base.to_owned();
    let mut n = 1_u32;
    while taken(&candidate)? {
        n += 1;
        candidate = format!("{base}{sep}{n}");
    }
    Ok(candidate)
}

/// Whether `name` can be a top-level entry of a workspace folder: one non-empty path component, not hidden.
#[must_use]
pub fn is_entry_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\0'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_keep_ascii_words_separated_by_dashes() {
        assert_eq!(slugify("Fix the Cart total!"), "fix-the-cart-total");
        assert_eq!(slugify("  --  "), "workspace");
        assert_eq!(slugify("é"), "workspace");
        assert_eq!(slugify(&"a".repeat(60)).len(), 40);
    }

    #[test]
    fn unique_appends_the_first_free_number() {
        let taken = ["app", "app-2"];
        let name = unique("app", "-", |name| Ok::<_, ()>(taken.contains(&name))).unwrap();
        assert_eq!(name, "app-3");
        assert_eq!(unique("web", "-", |_| Ok::<_, ()>(false)).unwrap(), "web");
    }

    #[test]
    fn entry_names_are_single_visible_components() {
        assert!(is_entry_name("backend"));
        for bad in ["", ".", "..", ".git", "a/b", "a\0"] {
            assert!(!is_entry_name(bad), "{bad:?}");
        }
    }
}
