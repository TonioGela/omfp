//! Native form controls in a widget's sources (AGT-089): `<select>`, `<input type="checkbox|radio|range|date|color">`
//! and `<datalist>` open lists and pickers the platform draws (a GTK menu in `WebKitGTK`), not the theme, so
//! `omfp_widget_check` warns about each one with the kit component to use instead. A cheap text scan of JSX tags, not
//! a parse: a tag inside a string or comment counts too.

use std::path::Path;

use super::compile::display;
use super::folders::files;

/// A native form control found in a widget file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeControl {
    /// The file relative to the widget folder, `/`-separated.
    pub file: String,
    /// 1-based line of the tag's `<`.
    pub line: usize,
    /// The element as written in the warning: `<select>`, `<input type="checkbox">`, `<datalist>`.
    pub element: &'static str,
    /// What to use from `omfp/kit` instead.
    pub replacement: &'static str,
}

/// The native elements warned about by tag name, with their kit replacement.
const ELEMENTS: [(&str, &str, &str); 2] = [
    ("select", "<select>", "Select from omfp/kit"),
    (
        "datalist",
        "<datalist>",
        "SearchField filtering a ListBox, or Select, from omfp/kit",
    ),
];

/// The `<input>` types warned about, with their kit replacement; other types (text, number, …) are left alone.
const INPUT_TYPES: [(&str, &str, &str); 5] = [
    (
        "checkbox",
        "<input type=\"checkbox\">",
        "Checkbox from omfp/kit",
    ),
    (
        "radio",
        "<input type=\"radio\">",
        "RadioGroup with Radio from omfp/kit",
    ),
    (
        "range",
        "<input type=\"range\">",
        "a Select of the steps from omfp/kit (Progress to only show a value)",
    ),
    (
        "date",
        "<input type=\"date\">",
        "a TextField from omfp/kit taking YYYY-MM-DD",
    ),
    (
        "color",
        "<input type=\"color\">",
        "a Select of theme colors from omfp/kit",
    ),
];

/// Whether `path` is a script file the scan reads (the compiled files and plain JavaScript).
fn is_script(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("ts" | "tsx" | "mts" | "jsx" | "js" | "mjs")
    )
}

/// Every native form control of the script files of `folder`, in path then line order; unreadable files are skipped
/// (the compile check reports them).
#[must_use]
pub fn scan_folder(folder: &Path) -> Vec<NativeControl> {
    files(folder)
        .into_iter()
        .filter(|(relative, _)| is_script(relative))
        .flat_map(|(relative, _)| {
            std::fs::read_to_string(folder.join(&relative))
                .map(|source| scan(&display(&relative), &source))
                .unwrap_or_default()
        })
        .collect()
}

/// Every native form control of `source`, the file `file`, in source order.
#[must_use]
pub fn scan(file: &str, source: &str) -> Vec<NativeControl> {
    let mut found = Vec::new();
    let (mut line, mut counted) = (1, 0);
    for (at, _) in source.match_indices('<') {
        // A `<` right after an identifier is a type argument or a comparison (`Array<select>`, `a<b`), not a tag.
        if source[..at]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || matches!(before, '_' | '$'))
        {
            continue;
        }
        let rest = &source[at + 1..];
        let name_end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')))
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        let control = if name == "input" {
            input_type(&rest[name_end..]).and_then(|kind| {
                INPUT_TYPES
                    .iter()
                    .find(|(wanted, ..)| kind.eq_ignore_ascii_case(wanted))
            })
        } else {
            ELEMENTS.iter().find(|(wanted, ..)| name == *wanted)
        };
        if let Some(&(_, element, replacement)) = control {
            line += source[counted..at].matches('\n').count();
            counted = at;
            found.push(NativeControl {
                file: file.to_owned(),
                line,
                element,
                replacement,
            });
        }
    }
    found
}

/// The literal `type` of the `<input` tag whose attributes start `attributes`: `type="date"`, `type='date'`,
/// ``type={"date"}`` or ``type={`date`}``; `None` without one or when it is computed.
fn input_type(attributes: &str) -> Option<&str> {
    let tag = &attributes[..tag_end(attributes)];
    let mut from = 0;
    while let Some(found) = tag[from..].find("type") {
        let at = from + found;
        from = at + "type".len();
        if !tag[..at].ends_with(char::is_whitespace) {
            continue;
        }
        let Some(value) = tag[from..].trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let value = value.strip_prefix('{').map_or(value, str::trim_start);
        let Some(quote @ ('"' | '\'' | '`')) = value.chars().next() else {
            return None;
        };
        let value = &value[1..];
        return value.find(quote).map(|end| &value[..end]);
    }
    None
}

/// Where the tag whose attributes start `attributes` ends: its `>` outside braces (`onChange={(e) => …}`) and quotes,
/// or the end of the text.
fn tag_end(attributes: &str) -> usize {
    let (mut depth, mut quote) = (0_usize, None);
    for (at, c) in attributes.char_indices() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (None, '"' | '\'' | '`') => quote = Some(c),
            (None, '{') => depth += 1,
            (None, '}') => depth = depth.saturating_sub(1),
            (None, '>' | '<') if depth == 0 => return at,
            _ => {}
        }
    }
    attributes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(source: &str) -> Vec<(usize, &'static str)> {
        scan("index.tsx", source)
            .into_iter()
            .map(|control| (control.line, control.element))
            .collect()
    }

    // spec: AGT-089
    #[test]
    fn select_datalist_and_the_popup_input_types_are_found_with_their_line() {
        let source = "import { Card } from \"omfp/kit\";\n\
                      export default function W() {\n\
                      \x20 return (<Card>\n\
                      \x20   <select value={v} onChange={(e) => set(e.target.value)}><option>a</option></select>\n\
                      \x20   <input type=\"checkbox\" checked={on} />\n\
                      \x20   <input\n\
                      \x20     onChange={(e) => e.target.value > 2 && go()}\n\
                      \x20     type='radio' />\n\
                      \x20   <input type={\"range\"} min={0} />\n\
                      \x20   <input value={d} type = {`date`} />\n\
                      \x20   <input type=\"COLOR\" />\n\
                      \x20   <datalist id=\"names\" />\n\
                      \x20 </Card>);\n\
                      }\n";
        assert_eq!(
            found(source),
            vec![
                (4, "<select>"),
                (5, "<input type=\"checkbox\">"),
                (6, "<input type=\"radio\">"),
                (9, "<input type=\"range\">"),
                (10, "<input type=\"date\">"),
                (11, "<input type=\"color\">"),
                (12, "<datalist>"),
            ]
        );
    }

    // spec: AGT-089
    #[test]
    fn kit_components_other_inputs_and_lookalikes_are_not_native_controls() {
        let source = "<Select label=\"x\" items={[]} selectedKey={k} onSelectionChange={set} />\n\
                      <Checkbox isSelected={on}>On</Checkbox>\n\
                      <input type=\"text\" value={q} />\n\
                      <input value={q} />\n\
                      <input type={kind} />\n\
                      <input data-type=\"checkbox\" />\n\
                      <selection /> <select-box />\n\
                      const rows: Array<select> = []; if (a<select) {}\n";
        assert_eq!(found(source), Vec::new());
    }

    // spec: AGT-089
    #[test]
    fn every_control_names_its_file_and_kit_replacement() {
        let controls = scan("parts/filter.tsx", "<select>\n<input type=\"checkbox\"/>");
        assert_eq!(
            controls,
            vec![
                NativeControl {
                    file: "parts/filter.tsx".to_owned(),
                    line: 1,
                    element: "<select>",
                    replacement: "Select from omfp/kit",
                },
                NativeControl {
                    file: "parts/filter.tsx".to_owned(),
                    line: 2,
                    element: "<input type=\"checkbox\">",
                    replacement: "Checkbox from omfp/kit",
                },
            ]
        );
    }

    // spec: AGT-089
    #[test]
    fn the_folder_scan_reads_every_script_file_in_path_order() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("parts")).unwrap();
        std::fs::write(folder.path().join("index.tsx"), "\n<select />").unwrap();
        std::fs::write(folder.path().join("parts/pick.js"), "<datalist />").unwrap();
        std::fs::write(folder.path().join("notes.md"), "<select>").unwrap();
        let controls: Vec<(String, usize)> = scan_folder(folder.path())
            .into_iter()
            .map(|control| (control.file, control.line))
            .collect();
        assert_eq!(
            controls,
            vec![("index.tsx".to_owned(), 2), ("parts/pick.js".to_owned(), 1)]
        );
    }
}
