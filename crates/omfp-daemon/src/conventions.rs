//! What the daemon tells the agent about its workspace (DMN-030): a short text passed with
//! `--append-system-prompt` — the workspace conventions, then a note on widgets (AGT-024) — followed by the user's own
//! `APPEND_SYSTEM.md`, which the flag would otherwise disable (S-10). Nothing is written into the workspace for it.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

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

/// The widgets note (AGT-024): where the views manifest, the workspace's widgets and the user library (`user_library`)
/// live, the host URIs of the built-in sources, what a widget may import, the tools that check and place a widget, an
/// example, and where to read more.
#[must_use]
pub fn widgets_note(path: &Path, user_library: &Path) -> String {
    let (path, library) = (path.display(), user_library.display());
    format!(
        "# omfp widgets\n\
         \n\
         The user's omfp window shows this workspace through React widgets that you can write and arrange too.\n\
         - Views: {path}/.omfp/views.json (schema omfp://schemas/views.json), the user's and yours to change. Keep the \
         `[\"tabs\", {{\"id\":\"main\"}}, …\"omfp:chat\"]` slot in every view you change: the user's files open as its tabs.\n\
         - Workspace widgets: {path}/.omfp/widgets/<name>/index.tsx, used as `./widgets/<name>`; a folder named like a \
         built-in (`chat` for `omfp:chat`) replaces it here.\n\
         - User library: {library}/<name>/, used as `user:<name>` in every workspace.\n\
         - Built-in widgets' sources: omfp://widgets/builtin and omfp://widgets/builtin/<name> (never search the disk).\n\
         - Imports: only react, react/jsx-runtime, react-dom, omfp/sdk, omfp/kit and files of the widget's folder. A \
         saved widget reloads at once; errors show in its place.\n\
         - After writing a widget call omfp_widget_check {{ name }} (compile errors, what open windows render); place it \
         with omfp_view_add {{ widget, where }} rather than rewriting views.json.\n\
         \n\
         ```tsx\n\
         import {{ Card, Log, Progress }} from \"omfp/kit\";\n\
         import {{ useJob }} from \"omfp/sdk\";\n\
         export default function Tests({{ command = \"make test\" }}: {{ command?: string }}) {{\n\
         \x20 const job = useJob(command); // status: pending | running | succeeded | failed; kept across reloads\n\
         \x20 return (\n\
         \x20   <Card title={{`Tests: ${{job.status}}`}}>\n\
         \x20     <Progress label=\"Tests\" value={{job.status === \"running\" ? undefined : 1}} max={{1}} />\n\
         \x20     <Log lines={{job.lines}} />\n\
         \x20   </Card>);\n\
         }}\n\
         ```\n\
         \n\
         Before writing widgets read omfp://docs/widgets; schema omfp://schemas/views.json."
    )
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

    // spec: AGT-024, AGT-028
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
            "import { Card, Log, Progress } from \"omfp/kit\";",
            "useJob(command)",
            "job.status",
            "read omfp://docs/widgets; schema omfp://schemas/views.json",
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
        assert!(text.lines().count() <= 24, "short: {text}");
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
