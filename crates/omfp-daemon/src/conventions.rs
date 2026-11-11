//! What the daemon tells the agent about its workspace (DMN-030): a short text passed with
//! `--append-system-prompt` — the workspace conventions, then a note on widgets (AGT-024) — followed by the user's own
//! `APPEND_SYSTEM.md`, which the flag would otherwise disable (S-10). Nothing is written into the workspace for it.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::paths::DataDir;

/// The daemon's workspace conventions for the agent of a thread living in workspace `name` at `path`.
#[must_use]
pub fn workspace_conventions(name: &str, path: &Path) -> String {
    let path = path.display();
    format!(
        "# omfp workspace\n\
         \n\
         You work in the omfp workspace \"{name}\", the folder {path} (your working directory). Its repositories \
         are its top-level entries; omfp finds them there and adds each one as a workspace root.\n\
         \n\
         - Work on a project from elsewhere: link it with `ln -s <project> {path}/<name>`.\n\
         - Work on a separate branch: `git -C <repo> worktree add {path}/<name> -b <branch>`.\n\
         - Get a remote repository: `git clone <url> {path}/<name>`.\n\
         - Keep repositories at the top level of the workspace: omfp does not look deeper."
    )
}

/// The user's append prompt as omp would load it: `<workspace>/.omp/APPEND_SYSTEM.md` if present, else
/// `<agent dir>/APPEND_SYSTEM.md`; trimmed, `None` when absent or empty.
#[must_use]
pub fn user_append(workspace: &Path, agent_dir: Option<&Path>) -> Option<String> {
    let project = workspace.join(".omp").join("APPEND_SYSTEM.md");
    let file = if project.is_file() {
        project
    } else {
        agent_dir?.join("APPEND_SYSTEM.md")
    };
    let text = std::fs::read_to_string(file).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Where widgets live and what they may import, shared by the widgets note and the `widget-builder` agent so the two
/// never drift: `root` prefixes the workspace paths (`<workspace>/` for the main agent, empty for the subagent, whose
/// cwd is the workspace folder).
fn widget_facts(root: &str, user_library: &Path) -> String {
    let library = user_library.display();
    format!(
        "- Workspace widgets: {root}.omfp/widgets/<name>/index.tsx, used as `./widgets/<name>`; a folder named like a \
         built-in (`chat` for `omfp:chat`) replaces it here.\n\
         - User library: {library}/<name>/, used as `user:<name>` in every workspace.\n\
         - Built-in widgets' sources: omfp://widgets/builtin and omfp://widgets/builtin/<name> (never search the disk).\n\
         - Imports: only react, react/jsx-runtime, react-dom, omfp/sdk, omfp/kit and files of the widget's folder. A \
         saved widget reloads at once; errors show in its place."
    )
}

/// A ten-line example widget, shared like [`widget_facts`].
const EXAMPLE: &str = "```tsx\n\
    import { Card, Log, Progress } from \"omfp/kit\";\n\
    import { useJob } from \"omfp/sdk\";\n\
    export default function Tests({ command = \"make test\" }: { command?: string }) {\n\
    \x20 const job = useJob(command); // status: pending | running | succeeded | failed; kept across reloads\n\
    \x20 return (\n\
    \x20   <Card title={`Tests: ${job.status}`}>\n\
    \x20     <Progress label=\"Tests\" value={job.status === \"running\" ? undefined : 1} max={1} />\n\
    \x20     <Log lines={job.lines} />\n\
    \x20   </Card>);\n\
    }\n\
    ```";

/// Name of the daemon's subagent that builds widgets (AGT-029).
pub const WIDGET_BUILDER: &str = "widget-builder";

/// The widgets note (AGT-024): where the views manifest, the workspace's widgets and the user library (`user_library`)
/// live, the host URIs of the built-in sources, what a widget may import, the tools that check and place a widget, how
/// to delegate building one to the `widget-builder` agent (AGT-029), when to notify the user (AGT-032), an example, and
/// where to read more.
#[must_use]
pub fn widgets_note(path: &Path, user_library: &Path) -> String {
    let display = path.display();
    format!(
        "# omfp widgets\n\
         \n\
         The user's omfp window shows this workspace through React widgets that you can write and arrange too.\n\
         - Views: {display}/.omfp/views.json (schema omfp://schemas/views.json), the user's and yours to change. Keep the \
         `[\"tabs\", {{\"id\":\"main\"}}, …\"omfp:chat\"]` slot in every view you change: the user's files open as its tabs.\n\
         {facts}\n\
         - After writing a widget call omfp_widget_check {{ name }} (compile errors, what open windows render); place it \
         with omfp_view_add {{ widget, where }} rather than rewriting views.json.\n\
         - Keys: {display}/.omfp/keymap.json (schema omfp://schemas/keymap.json) binds keys to commands, built-in or a \
         widget's useCommand, in windows showing this workspace; give a widget a command and bind it there.\n\
         - Notify the user with omfp_notify {{ title, body }} when a long task finished or you need them; once per \
         occasion, never for progress.\n\
         - When something would read better as a visualization or controls (progress, a table, a chart, buttons), \
         propose a widget in one sentence and build it only after the user agrees.\n\
         - Once the user agrees, delegate the building: a `task` with `agent: \"{WIDGET_BUILDER}\"` and a complete brief \
         (what to show, its data sources, where it goes); keep talking, and place the widget with omfp_view_add when \
         the task's result arrives.\n\
         \n\
         {EXAMPLE}\n\
         \n\
         Before writing widgets read omfp://docs/widgets; schema omfp://schemas/views.json.",
        facts = widget_facts(&format!("{display}/"), user_library),
    )
}

/// The `widget-builder` agent definition (AGT-029), an omp agent file: frontmatter (its tools, the user's `smol` model
/// role, not blocking so the main agent keeps talking) and a body carrying the widgets knowledge, since a subagent gets
/// neither the main agent's appended prompt nor its host tools (S-14).
#[must_use]
pub fn widget_builder_definition(user_library: &Path) -> String {
    format!(
        "---\n\
         name: {WIDGET_BUILDER}\n\
         description: Builds or changes one omfp widget in the workspace and checks it until it is clean. Give it a \
         complete brief - what to show, its data sources, where it goes; place the widget yourself afterwards.\n\
         tools: read, write, edit, glob, grep\n\
         model: \"@smol\"\n\
         ---\n\
         # omfp widget builder\n\
         \n\
         You build or change one React widget of the user's omfp window, as the brief asks. Your working directory is \
         the workspace folder: use relative paths.\n\
         {facts}\n\
         - First read omfp://docs/widgets (the SDK and kit reference) and, for how views use widgets, \
         omfp://schemas/views.json; a built-in close to what you need is a good start (omfp://widgets/builtin/<name>).\n\
         - After every write or edit, read omfp://widgets/check/<name> (`user:<name>` for the user library): it \
         compiles the widget and says what open windows render. Fix what it reports and read it again until it is clean.\n\
         - Never edit .omfp/views.json: the main agent places the widget.\n\
         - Finish with a short summary naming the widget (`./widgets/<name>` or `user:<name>`) and its props.\n\
         \n\
         {EXAMPLE}\n",
        facts = widget_facts("", user_library),
    )
}

/// Writes the `widget-builder` agent (AGT-029) to `data.widget_builder()`, inside the omp extension root the daemon
/// passes with `--extension` (DMN-107), unless the file already holds this content: returns whether it wrote.
pub fn write_widget_builder(data: &DataDir) -> std::io::Result<bool> {
    let path = data.widget_builder();
    let content = widget_builder_definition(&data.widgets());
    if std::fs::read_to_string(&path).is_ok_and(|current| current == content) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)?;
    Ok(true)
}

/// What the daemon itself appends to the system prompt: the workspace conventions, then the widgets note.
#[must_use]
pub fn daemon_text(name: &str, path: &Path, user_library: &Path) -> String {
    format!(
        "{}\n\n{}",
        workspace_conventions(name, path),
        widgets_note(path, user_library)
    )
}

/// The `--append-system-prompt` text: the daemon's text, then the user's append prompt when there is one.
#[must_use]
pub fn append_system_prompt(
    name: &str,
    path: &Path,
    agent_dir: Option<&Path>,
    user_library: &Path,
) -> String {
    let text = daemon_text(name, path, user_library);
    match user_append(path, agent_dir) {
        Some(user) => format!("{text}\n\n{user}"),
        None => text,
    }
}

/// omp's user agent directory, `<HOME>/.omp/agent`, with `HOME` taken from the omp environment overrides when they
/// set it, else from `home`.
#[must_use]
pub fn omp_agent_dir(
    overrides: &[(OsString, Option<OsString>)],
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    let home = match overrides.iter().rev().find(|(key, _)| key == "HOME") {
        Some((_, value)) => value.as_deref(),
        None => home,
    }?;
    Some(Path::new(home).join(".omp").join("agent"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-030
    #[test]
    fn conventions_name_the_workspace_and_how_to_bring_repositories_in() {
        let text = workspace_conventions("checkout", Path::new("/ws/checkout"));
        assert!(text.starts_with("# omfp workspace\n"), "{text}");
        assert!(
            text.contains("\"checkout\", the folder /ws/checkout"),
            "{text}"
        );
        for rule in [
            "top-level entries",
            "ln -s <project> /ws/checkout/<name>",
            "worktree add /ws/checkout/<name>",
            "git clone <url> /ws/checkout/<name>",
            "does not look deeper",
        ] {
            assert!(text.contains(rule), "{rule:?} missing: {text}");
        }
        assert!(text.lines().count() <= 10, "short: {text}");
        assert!(!text.ends_with('\n'));
    }

    // spec: AGT-024, AGT-028, AGT-029, AGT-031, AGT-032
    #[test]
    fn the_widgets_note_says_where_widgets_live_what_they_import_and_how_to_check_and_place_them() {
        let library = Path::new("/data/omfp/widgets");
        let text = widgets_note(Path::new("/ws/checkout"), library);
        assert!(text.starts_with("# omfp widgets\n"), "{text}");
        for rule in [
            "/ws/checkout/.omfp/views.json",
            "/ws/checkout/.omfp/widgets/<name>/index.tsx",
            "`./widgets/<name>`",
            "/data/omfp/widgets/<name>/",
            "`user:<name>`",
            "omfp://widgets/builtin/<name>",
            "react, react/jsx-runtime, react-dom, omfp/sdk, omfp/kit",
            "omfp_widget_check { name }",
            "omfp_view_add { widget, where }",
            "propose a widget in one sentence and build it only after the user agrees",
            "a `task` with `agent: \"widget-builder\"` and a complete brief \
             (what to show, its data sources, where it goes); keep talking, and place the widget with omfp_view_add \
             when the task's result arrives",
            "import { Card, Log, Progress } from \"omfp/kit\";",
            "useJob(command)",
            "job.status",
            "read omfp://docs/widgets; schema omfp://schemas/views.json",
            "- Keys: /ws/checkout/.omfp/keymap.json (schema omfp://schemas/keymap.json) binds keys to commands",
            "- Notify the user with omfp_notify { title, body } when a long task finished or you need them; once per \
             occasion, never for progress.\n",
            "Keep the `[\"tabs\", {\"id\":\"main\"}, …\"omfp:chat\"]` slot",
        ] {
            assert!(text.contains(rule), "{rule:?} missing: {text}");
        }
        assert!(
            !text.contains("builtin-widgets") && !text.contains("absent"),
            "no bundle path, and views.json is always there: {text}"
        );
        let example = text.split("```").nth(1).unwrap();
        assert!(
            example.lines().count() <= 11,
            "a ten-line example: {example}"
        );
        assert!(text.lines().count() <= 28, "short: {text}");
        assert!(!text.ends_with('\n'));
    }

    // spec: DMN-030, AGT-024
    #[test]
    fn the_users_append_file_follows_the_conventions_and_the_widgets_note_and_the_workspace_one_wins()
     {
        let ws = tempfile::tempdir().unwrap();
        let agent = tempfile::tempdir().unwrap();
        let library = Path::new("/data/widgets");
        let prompt = |agent: &Path| append_system_prompt("w", ws.path(), Some(agent), library);
        let daemon = format!(
            "{}\n\n{}",
            workspace_conventions("w", ws.path()),
            widgets_note(ws.path(), library)
        );
        assert_eq!(daemon_text("w", ws.path(), library), daemon);
        assert_eq!(prompt(agent.path()), daemon);

        std::fs::write(agent.path().join("APPEND_SYSTEM.md"), "  user rule\n").unwrap();
        assert_eq!(prompt(agent.path()), format!("{daemon}\n\nuser rule"));

        std::fs::create_dir(ws.path().join(".omp")).unwrap();
        std::fs::write(ws.path().join(".omp/APPEND_SYSTEM.md"), "project rule").unwrap();
        assert_eq!(prompt(agent.path()), format!("{daemon}\n\nproject rule"));
    }

    // spec: AGT-029
    #[test]
    fn the_widget_builder_is_a_background_smol_agent_carrying_the_widgets_facts() {
        let library = Path::new("/data/omfp/widgets");
        let text = widget_builder_definition(library);
        let (frontmatter, body) = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .expect("frontmatter");
        let fields: Vec<&str> = frontmatter
            .lines()
            .map(|line| line.split_once(": ").unwrap().0)
            .collect();
        assert_eq!(fields, ["name", "description", "tools", "model"]);
        for field in [
            "name: widget-builder",
            "tools: read, write, edit, glob, grep",
            "model: \"@smol\"",
        ] {
            assert!(
                frontmatter.lines().any(|line| line == field),
                "{field}: {frontmatter}"
            );
        }
        assert!(!frontmatter.contains("blocking"), "runs in the background");
        assert!(
            body.contains(&widget_facts("", library)),
            "the same facts as the widgets note, with relative paths: {body}"
        );
        assert!(widgets_note(Path::new("/ws"), library).contains(&widget_facts("/ws/", library)));
        for rule in [
            "working directory is the workspace folder: use relative paths",
            "- Workspace widgets: .omfp/widgets/<name>/index.tsx",
            "read omfp://docs/widgets",
            "omfp://widgets/builtin/<name>",
            "omfp://schemas/views.json",
            "react, react/jsx-runtime, react-dom, omfp/sdk, omfp/kit and files of the widget's folder",
            "After every write or edit, read omfp://widgets/check/<name>",
            "until it is clean",
            "Never edit .omfp/views.json",
            "short summary naming the widget (`./widgets/<name>` or `user:<name>`) and its props",
            EXAMPLE,
        ] {
            assert!(body.contains(rule), "{rule:?} missing: {body}");
        }
        assert!(
            !body.contains("omfp_widget_check") && !body.contains("omfp_view_add"),
            "no host tools in a subagent"
        );
    }

    // spec: AGT-029, DMN-107
    #[test]
    fn the_widget_builder_is_written_under_the_data_dir_and_rewritten_only_when_it_changed() {
        let root = tempfile::tempdir().unwrap();
        let data = DataDir::new(root.path());
        let path = root.path().join("omp/agents/widget-builder.md");
        let expected = widget_builder_definition(&root.path().join("widgets"));

        assert!(write_widget_builder(&data).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        let entries: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries, ["omp"], "nothing else written");

        assert!(!write_widget_builder(&data).unwrap(), "same content");

        std::fs::write(&path, "an older definition").unwrap();
        assert!(write_widget_builder(&data).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    }

    #[test]
    fn the_agent_dir_follows_the_home_omp_runs_with() {
        let overrides = vec![(OsString::from("HOME"), Some(OsString::from("/isolated")))];
        assert_eq!(
            omp_agent_dir(&overrides, Some(OsStr::new("/home/u"))),
            Some(PathBuf::from("/isolated/.omp/agent"))
        );
        assert_eq!(
            omp_agent_dir(&[], Some(OsStr::new("/home/u"))),
            Some(PathBuf::from("/home/u/.omp/agent"))
        );
        assert_eq!(omp_agent_dir(&[], None), None);
    }
}
