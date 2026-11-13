//! Requirement traceability (TST-040): every requirement ID in `docs/specs/*.md`, the sources citing it with
//! `spec: <ID>`, and the milestones in `docs/specs/milestones.toml` whose requirements must all be cited once done.
//! Only citations of what runs count (TST-114): tests not `#[ignore]`d (or skipped) and recipes `just ci` runs; the
//! others are listed apart.

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

/// Starts of a TypeScript test that never runs.
const SKIPPED: [&str; 5] = [
    "test.skip(",
    "it.skip(",
    "describe.skip(",
    "test.todo(",
    "it.todo(",
];

/// Requirement IDs cited in Rust or TypeScript `source`: `(run, ignored)`, the second those of tests that never run,
/// an `#[ignore]` among the attributes under the citation or a `.skip`/`.todo` test right under it.
#[must_use]
pub fn source_citations(source: &str) -> (Vec<String>, Vec<String>) {
    let lines: Vec<&str> = source.lines().collect();
    let (mut run, mut ignored) = (Vec::new(), Vec::new());
    for (index, line) in lines.iter().enumerate() {
        let ids = cited_ids(line);
        if ids.is_empty() {
            continue;
        }
        let mut never_runs = false;
        for next in lines[index + 1..].iter().map(|line| line.trim()) {
            if next.starts_with("//") {
                continue;
            }
            if next.starts_with("#[") {
                never_runs |= next.starts_with("#[ignore");
                continue;
            }
            never_runs |= SKIPPED.iter().any(|start| next.starts_with(start));
            break;
        }
        if never_runs { &mut ignored } else { &mut run }.extend(ids);
    }
    (run, ignored)
}

/// A recipe of the justfile: what it depends on and the requirements its comment block and body cite.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Recipe {
    pub dependencies: Vec<String>,
    pub cited: Vec<String>,
}

/// A justfile as `spec-trace` reads it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Justfile {
    pub recipes: BTreeMap<String, Recipe>,
    /// Requirements cited outside any recipe's comment block or body.
    pub loose: Vec<String>,
    /// The recipes `just ci` runs: the words of `ci_recipes := "…"`.
    pub ci_recipes: Vec<String>,
}

/// The recipes a dependency list names: each bare name, and the first name of each `(recipe args…)`; strings skipped.
fn dependencies(list: &str) -> Vec<String> {
    let mut names = Vec::new();
    let (mut depth, mut named, mut quote, mut word) = (0_u32, false, None, String::new());
    for c in list.chars().chain([' ']) {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }
            continue;
        }
        if c.is_alphanumeric() || c == '-' || c == '_' {
            word.push(c);
            continue;
        }
        if !word.is_empty() && (depth == 0 || (depth == 1 && !named)) {
            names.push(word.clone());
            named = depth == 1;
        }
        word.clear();
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => {
                named &= depth != 0;
                depth += 1;
            }
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    names
}

/// Reads the recipes of a justfile: a recipe's citations are those of the comment lines right above it (attributes
/// between them) and of its header and body.
#[must_use]
pub fn parse_justfile(source: &str) -> Justfile {
    let header = Regex::new(r"^@?([A-Za-z_][\w-]*)[^:]*:(.*)$").expect("valid regex");
    let assignment = Regex::new(r"^(?:export\s+|alias\s+)?[\w-]+\s*:=").expect("valid regex");
    let ci_recipes = Regex::new(r#"^ci_recipes\s*:=\s*"([^"]*)""#).expect("valid regex");
    let mut justfile = Justfile::default();
    let (mut pending, mut current): (Vec<String>, Option<String>) = (Vec::new(), None);
    for line in source.lines() {
        if line.starts_with([' ', '\t']) {
            match current
                .as_ref()
                .and_then(|name| justfile.recipes.get_mut(name))
            {
                Some(recipe) => recipe.cited.extend(cited_ids(line)),
                None => justfile.loose.extend(cited_ids(line)),
            }
            continue;
        }
        if line.trim().is_empty() {
            justfile.loose.append(&mut pending);
            continue;
        }
        current = None;
        if line.starts_with('#') {
            pending.extend(cited_ids(line));
            continue;
        }
        if line.starts_with('[') {
            continue;
        }
        if let Some(captures) = ci_recipes.captures(line) {
            justfile.ci_recipes = captures[1].split_whitespace().map(str::to_owned).collect();
        }
        match header.captures(line) {
            Some(captures) if !assignment.is_match(line) => {
                let name = captures[1].to_owned();
                pending.extend(cited_ids(line));
                let recipe = Recipe {
                    dependencies: dependencies(&captures[2]),
                    cited: std::mem::take(&mut pending),
                };
                justfile.recipes.insert(name.clone(), recipe);
                current = Some(name);
            }
            _ => {
                justfile.loose.append(&mut pending);
                justfile.loose.extend(cited_ids(line));
            }
        }
    }
    justfile.loose.append(&mut pending);
    justfile
}

impl Justfile {
    /// The recipes `just ci` runs: `ci`, `ci_recipes` and everything they depend on.
    #[must_use]
    pub fn run_by_ci(&self) -> BTreeSet<String> {
        let mut run = BTreeSet::new();
        let mut pending: Vec<String> = self.ci_recipes.clone();
        pending.push("ci".to_owned());
        while let Some(name) = pending.pop() {
            if let Some(recipe) = self.recipes.get(&name)
                && run.insert(name)
            {
                pending.extend(recipe.dependencies.iter().cloned());
            }
        }
        run
    }

    /// The requirements cited: `(run, not_run)`, the second with the recipe `just ci` does not run (or "no recipe").
    #[must_use]
    pub fn citations(&self) -> (Vec<String>, Vec<(String, String)>) {
        let ci = self.run_by_ci();
        let (mut run, mut not_run) = (Vec::new(), Vec::new());
        for (name, recipe) in &self.recipes {
            if ci.contains(name) {
                run.extend(recipe.cited.iter().cloned());
            } else {
                let why = format!("recipe {name}, not run by `just ci`");
                not_run.extend(recipe.cited.iter().map(|id| (id.clone(), why.clone())));
            }
        }
        not_run.extend(
            self.loose
                .iter()
                .map(|id| (id.clone(), "no recipe".to_owned())),
        );
        (run, not_run)
    }
}

/// Result of tracing: which files cite each requirement, and what is wrong.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub citations: BTreeMap<String, BTreeSet<String>>,
    /// Citations that are no coverage: tests that never run, recipes `just ci` does not run (TST-114).
    pub not_run: BTreeMap<String, BTreeSet<String>>,
    pub errors: Vec<String>,
    /// Requirements of milestones in progress that no test cites yet (informational).
    pub pending: BTreeMap<String, Vec<String>>,
}

/// " (cited only where nothing runs: …)" when `id` has such citations.
fn not_run_note(not_run: &BTreeMap<String, BTreeSet<String>>, id: &str) -> String {
    not_run.get(id).map_or_else(String::new, |places| {
        let places: Vec<&str> = places.iter().map(String::as_str).collect();
        format!(" (cited only where nothing runs: {})", places.join(", "))
    })
}

/// Cross-checks definitions, citations and milestones; `not_run` citations count as none.
#[must_use]
pub fn trace(
    defined: &BTreeSet<String>,
    citations: &BTreeMap<String, BTreeSet<String>>,
    not_run: &BTreeMap<String, BTreeSet<String>>,
    milestones: &BTreeMap<String, Milestone>,
) -> Report {
    let mut report = Report {
        citations: citations.clone(),
        not_run: not_run.clone(),
        ..Report::default()
    };
    let mut cited: BTreeMap<&String, BTreeSet<&String>> = BTreeMap::new();
    for (id, files) in citations.iter().chain(not_run) {
        cited.entry(id).or_default().extend(files);
    }
    for (id, files) in cited {
        if !defined.contains(id) {
            let files: Vec<&str> = files.into_iter().map(String::as_str).collect();
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
                Status::Done => report.errors.push(format!(
                    "milestone {name} is done but {id} has no test{}",
                    not_run_note(not_run, id)
                )),
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

/// Files scanned for citations: Rust and TypeScript (`.ts`, `.tsx`) sources and the justfile, outside `target/`,
/// `node_modules/`, `docs/`, `fixtures/` and hidden directories other than `.github/` (the workflow check's tests).
fn citing_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0
                || !((name.starts_with('.') && name != ".github")
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

type Citations = BTreeMap<String, BTreeSet<String>>;

/// The citations of the files under `root`: `(run, not_run)`, each requirement with its files (`not_run` with why).
fn citations(root: &Path) -> anyhow::Result<(Citations, Citations)> {
    let (mut run, mut not_run) = (Citations::new(), Citations::new());
    for path in citing_files(root) {
        let source = std::fs::read_to_string(&path)?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let (cited, never) = if path.file_name().is_some_and(|name| name == "justfile") {
            let (cited, never) = parse_justfile(&source).citations();
            let never = never
                .into_iter()
                .map(|(id, why)| (id, format!("{relative} ({why})")))
                .collect::<Vec<_>>();
            (cited, never)
        } else {
            let (cited, ignored) = source_citations(&source);
            let why = format!("{relative} (a test that never runs)");
            (
                cited,
                ignored.into_iter().map(|id| (id, why.clone())).collect(),
            )
        };
        for id in cited {
            run.entry(id).or_default().insert(relative.clone());
        }
        for (id, place) in never {
            not_run.entry(id).or_default().insert(place);
        }
    }
    Ok((run, not_run))
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
    let (run, not_run) = citations(root)?;
    let milestones: BTreeMap<String, Milestone> = toml::from_str(
        &std::fs::read_to_string(specs.join("milestones.toml"))
            .context("reading milestones.toml")?,
    )
    .context("parsing milestones.toml")?;

    Ok(trace(&defined, &run, &not_run, &milestones))
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

    /// `text` with `@` standing for the citation marker, so this file does not itself cite the IDs in it.
    fn marked(text: &str) -> String {
        text.replace('@', "spec:")
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
        let source =
            marked("// @ TST-020\n#[test]\n// @ ENV-010, ENV-011\n# @ ARC-010\nlet s = \"spec\";");
        assert_eq!(
            cited_ids(&source),
            ["TST-020", "ENV-010", "ENV-011", "ARC-010"]
        );
    }

    // spec: TST-114
    #[test]
    fn citations_of_ignored_and_skipped_tests_are_apart() {
        let rust = marked(
            "// @ DMN-001\n#[test]\nfn runs() {}\n\
             // @ DMN-002\n#[tokio::test(flavor = \"multi_thread\")]\n#[ignore = \"benchmark\"]\nasync fn bench() {}\n\
             // @ DMN-003\n// a note\n#[ignore]\n#[test]\nfn ignored() {}\n",
        );
        assert_eq!(
            source_citations(&rust),
            (
                vec!["DMN-001".to_owned()],
                vec!["DMN-002".to_owned(), "DMN-003".to_owned()]
            )
        );
        let typescript = marked(
            "  // @ UI-001\n  test(\"runs\", () => {});\n  // @ UI-002\n  test.skip(\"skipped\", () => {});\n\
             // @ UI-003\nit.todo(\"later\");\n",
        );
        assert_eq!(
            source_citations(&typescript),
            (
                vec!["UI-001".to_owned()],
                vec!["UI-002".to_owned(), "UI-003".to_owned()]
            )
        );
    }

    // spec: TST-114
    #[test]
    fn justfile_citations_count_only_in_recipes_just_ci_runs() {
        let justfile = parse_justfile(&marked(
            "# Development tasks.\n\n\
             ci_recipes := \"check test-e2e\"\n\n\
             # @ TST-001 — the checks\ncheck: lint (spec-trace \"x y\")\n\n\
             # Formatting\n# @ TST-002\nlint:\n    cargo fmt --check  # @ TST-003\n\n\
             spec-trace:\n    cargo xtask spec-trace\n\n\
             # @ TST-004\ntest-e2e: (with-web-bundle 'run ${A:-$(b)}; c')\n\n\
             # @ TST-005\n[private]\nwith-web-bundle command:\n    {{ command }}\n\n\
             # @ ENV-062 — release only\npackage-linux out=\"dist\": build-web\n    sh {{ out }}\n\n\
             build-web out=\"\":\n    bun run build\n\n\
             ci: (with-web-bundle just + \" \" + ci_recipes)\n\n\
             # @ TST-006, a note about nothing\n\nexport X := \"1\"\n",
        ));
        assert_eq!(
            justfile
                .recipes
                .get("check")
                .map(|recipe| &recipe.dependencies),
            Some(&vec!["lint".to_owned(), "spec-trace".to_owned()])
        );
        assert_eq!(
            justfile.run_by_ci(),
            set(&[
                "check",
                "ci",
                "lint",
                "spec-trace",
                "test-e2e",
                "with-web-bundle"
            ])
        );
        let (run, not_run) = justfile.citations();
        assert_eq!(run, ["TST-001", "TST-002", "TST-003", "TST-004", "TST-005"]);
        assert_eq!(
            not_run,
            [
                (
                    "ENV-062".to_owned(),
                    "recipe package-linux, not run by `just ci`".to_owned()
                ),
                ("TST-006".to_owned(), "no recipe".to_owned()),
            ]
        );
    }

    // spec: TST-040
    #[test]
    fn citing_an_undefined_requirement_is_an_error() {
        let report = trace(
            &set(&["DMN-001"]),
            &cited(&[("DMN-999", "a.rs")]),
            &cited(&[("DMN-998", "b.rs (a test that never runs)")]),
            &BTreeMap::new(),
        );
        assert_eq!(
            report.errors,
            [
                "DMN-998 is cited in b.rs (a test that never runs) but defined in no spec",
                "DMN-999 is cited in a.rs but defined in no spec",
            ]
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

        let report = trace(&defined, &citations, &BTreeMap::new(), &milestones);

        assert_eq!(
            report.errors,
            ["milestone M0 is done but TST-020 has no test"]
        );
        assert_eq!(report.pending.get("M1"), Some(&vec!["UI-001".to_owned()]));
        assert!(!report.pending.contains_key("M2"));
    }

    // spec: TST-114
    #[test]
    fn a_requirement_cited_only_where_nothing_runs_has_no_test() {
        let mut milestones = BTreeMap::new();
        milestones.insert("M0".to_owned(), milestone(Status::Done, &["TST-100"]));
        let not_run = cited(&[("TST-100", "e2e/tests/perf.rs (a test that never runs)")]);

        let report = trace(&set(&["TST-100"]), &BTreeMap::new(), &not_run, &milestones);

        assert_eq!(
            report.errors,
            [
                "milestone M0 is done but TST-100 has no test (cited only where nothing runs: \
                 e2e/tests/perf.rs (a test that never runs))"
            ]
        );
        assert_eq!(report.not_run, not_run);
    }

    // spec: TST-040
    #[test]
    fn a_milestone_listing_an_undefined_requirement_is_an_error() {
        let mut milestones = BTreeMap::new();
        milestones.insert("M0".to_owned(), milestone(Status::Planned, &["XYZ-001"]));

        let report = trace(&set(&[]), &BTreeMap::new(), &BTreeMap::new(), &milestones);

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
            ".github/check-workflow.test.ts",
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
                ".github/check-workflow.test.ts",
                "crates/a/src/lib.rs",
                "justfile",
                "ui/packages/app/test/widgets/threads.test.tsx",
                "ui/packages/core/src/reducer.ts",
                "ui/packages/core/test/reducer.test.ts",
            ]
        );
    }
}
