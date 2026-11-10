//! Requirement traceability (TST-040): every requirement ID in `docs/specs/*.md`, the sources citing it with
//! `spec: <ID>`, and the milestones in `docs/specs/milestones.toml` whose requirements must all be cited once done.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use regex::Regex;
use serde::Deserialize;

/// Status of a milestone in `docs/specs/milestones.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Planned,
    InProgress,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Milestone {
    pub status: Status,
    #[serde(default)]
    pub requirements: Vec<String>,
}

/// Requirement IDs defined in a spec document: first cell of a table row (`| DMN-012 | ... |`).
#[must_use]
pub fn defined_ids(markdown: &str) -> Vec<String> {
    let row = Regex::new(r"(?m)^\|\s*((?:ARC|ENV|DMN|PRT|UI|TRM|TST|AGT)-\d{3}|E2E-\d{2})\s*\|")
        .expect("valid regex");
    row.captures_iter(markdown)
        .map(|captures| captures[1].to_owned())
        .collect()
}

/// Requirement IDs cited in a source file through `spec: ID[, ID…]` (in a comment or recipe).
#[must_use]
pub fn cited_ids(source: &str) -> Vec<String> {
    let citation =
        Regex::new(r"spec:\s*([A-Z0-9]+-\d+(?:\s*,\s*[A-Z0-9]+-\d+)*)").expect("valid regex");
    citation
        .captures_iter(source)
        .flat_map(|captures| {
            captures[1]
                .split(',')
                .map(|id| id.trim().to_owned())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Result of tracing: which files cite each requirement, and what is wrong.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub citations: BTreeMap<String, BTreeSet<String>>,
    pub errors: Vec<String>,
    /// Requirements of milestones in progress that no test cites yet (informational).
    pub pending: BTreeMap<String, Vec<String>>,
}

/// Cross-checks definitions, citations and milestones.
#[must_use]
pub fn trace(
    defined: &BTreeSet<String>,
    citations: &BTreeMap<String, BTreeSet<String>>,
    milestones: &BTreeMap<String, Milestone>,
) -> Report {
    let mut report = Report {
        citations: citations.clone(),
        ..Report::default()
    };
    for (id, files) in citations {
        if !defined.contains(id) {
            let files: Vec<&str> = files.iter().map(String::as_str).collect();
            report.errors.push(format!(
                "{id} is cited in {} but defined in no spec",
                files.join(", ")
            ));
        }
    }
    for (name, milestone) in milestones {
        for id in &milestone.requirements {
            if !defined.contains(id) {
                report.errors.push(format!(
                    "milestone {name} lists {id}, which no spec defines"
                ));
                continue;
            }
            if citations.contains_key(id) {
                continue;
            }
            match milestone.status {
                Status::Done => report
                    .errors
                    .push(format!("milestone {name} is done but {id} has no test")),
                Status::InProgress => report
                    .pending
                    .entry(name.clone())
                    .or_default()
                    .push(id.clone()),
                Status::Planned => {}
            }
        }
    }
    report
}

/// Files scanned for citations: Rust and TypeScript (`.ts`, `.tsx`) sources and the justfile, outside `target/`, `node_modules/` and
/// hidden directories.
fn citing_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0
                || !(name.starts_with('.')
                    || name == "target"
                    || name == "node_modules"
                    || name == "docs"
                    || name == "fixtures")
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "rs" || ext == "ts" || ext == "tsx")
                || path.file_name().is_some_and(|n| n == "justfile")
        })
        .collect()
}

/// Traces the repository at `root`.
pub fn run(root: &Path) -> anyhow::Result<Report> {
    let specs = root.join("docs/specs");
    let mut defined = BTreeSet::new();
    for entry in std::fs::read_dir(&specs).context("reading docs/specs")? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "md") {
            defined.extend(defined_ids(&std::fs::read_to_string(&path)?));
        }
    }

    let mut citations: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in citing_files(root) {
        let source = std::fs::read_to_string(&path)?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        for id in cited_ids(&source) {
            citations.entry(id).or_default().insert(relative.clone());
        }
    }

    let milestones: BTreeMap<String, Milestone> = toml::from_str(
        &std::fs::read_to_string(specs.join("milestones.toml"))
            .context("reading milestones.toml")?,
    )
    .context("parsing milestones.toml")?;

    Ok(trace(&defined, &citations, &milestones))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn cited(pairs: &[(&str, &str)]) -> BTreeMap<String, BTreeSet<String>> {
        let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (id, file) in pairs {
            map.entry((*id).to_owned())
                .or_default()
                .insert((*file).to_owned());
        }
        map
    }

    fn milestone(status: Status, ids: &[&str]) -> Milestone {
        Milestone {
            status,
            requirements: ids.iter().map(|id| (*id).to_owned()).collect(),
        }
    }

    // spec: TST-040
    #[test]
    fn defined_ids_come_from_the_first_cell_of_table_rows_only() {
        let markdown = "\
| ID | Requirement |
| --- | --- |
| DMN-012 | Transcripts are not stored; see DMN-013. |
| E2E-04 | Reload mid-stream. |
Text mentioning ARC-001 is not a definition.
";
        assert_eq!(defined_ids(markdown), ["DMN-012", "E2E-04"]);
    }

    // spec: TST-040
    #[test]
    fn cited_ids_accept_single_and_comma_separated_citations() {
        // `@` stands for the citation marker, so this file does not itself cite these IDs.
        let source = "// @ TST-020\n#[test]\n// @ ENV-010, ENV-011\n# @ ARC-010\nlet s = \"spec\";"
            .replace('@', "spec:");
        assert_eq!(
            cited_ids(&source),
            ["TST-020", "ENV-010", "ENV-011", "ARC-010"]
        );
    }

    // spec: TST-040
    #[test]
    fn citing_an_undefined_requirement_is_an_error() {
        let report = trace(
            &set(&["DMN-001"]),
            &cited(&[("DMN-999", "a.rs")]),
            &BTreeMap::new(),
        );
        assert_eq!(
            report.errors,
            ["DMN-999 is cited in a.rs but defined in no spec"]
        );
    }

    // spec: TST-040
    #[test]
    fn a_done_milestone_fails_on_uncited_requirements_while_in_progress_only_reports_them() {
        let defined = set(&["ENV-010", "TST-020", "UI-001"]);
        let citations = cited(&[("ENV-010", "versions.rs")]);
        let mut milestones = BTreeMap::new();
        milestones.insert(
            "M0".to_owned(),
            milestone(Status::Done, &["ENV-010", "TST-020"]),
        );
        milestones.insert("M1".to_owned(), milestone(Status::InProgress, &["UI-001"]));
        milestones.insert("M2".to_owned(), milestone(Status::Planned, &["UI-001"]));

        let report = trace(&defined, &citations, &milestones);

        assert_eq!(
            report.errors,
            ["milestone M0 is done but TST-020 has no test"]
        );
        assert_eq!(report.pending.get("M1"), Some(&vec!["UI-001".to_owned()]));
        assert!(!report.pending.contains_key("M2"));
    }

    // spec: TST-040
    #[test]
    fn a_milestone_listing_an_undefined_requirement_is_an_error() {
        let mut milestones = BTreeMap::new();
        milestones.insert("M0".to_owned(), milestone(Status::Planned, &["XYZ-001"]));

        let report = trace(&set(&[]), &BTreeMap::new(), &milestones);

        assert_eq!(
            report.errors,
            ["milestone M0 lists XYZ-001, which no spec defines"]
        );
    }

    // spec: TST-040
    #[test]
    fn citations_are_read_from_rust_typescript_and_the_justfile_outside_build_and_dependency_dirs()
    {
        let root = tempfile::tempdir().expect("temp dir");
        let files = [
            "justfile",
            "crates/a/src/lib.rs",
            "ui/packages/core/src/reducer.ts",
            "ui/packages/core/test/reducer.test.ts",
            "ui/packages/app/test/widgets/threads.test.tsx",
            "ui/node_modules/typescript/lib/lib.d.ts",
            "ui/packages/core/node_modules/dep/index.ts",
            "target/debug/build.rs",
            "docs/specs/README.md",
            ".git/hooks/x.rs",
            "ui/package.json",
        ];
        for file in files {
            let path = root.path().join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(&path, "").expect("file");
        }

        let mut found: Vec<String> = citing_files(root.path())
            .iter()
            .map(|path| {
                path.strip_prefix(root.path())
                    .expect("under root")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();

        assert_eq!(
            found,
            [
                "crates/a/src/lib.rs",
                "justfile",
                "ui/packages/app/test/widgets/threads.test.tsx",
                "ui/packages/core/src/reducer.ts",
                "ui/packages/core/test/reducer.test.ts",
            ]
        );
    }
}
