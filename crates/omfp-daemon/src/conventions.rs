//! What the daemon tells the agent about its workspace (DMN-030): a short text passed with
//! `--append-system-prompt`, followed by the user's own `APPEND_SYSTEM.md`, which the flag would otherwise disable
//! (S-10). Nothing is written into the workspace.

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

/// The `--append-system-prompt` text: the conventions, then the user's append prompt when there is one.
#[must_use]
pub fn append_system_prompt(name: &str, path: &Path, agent_dir: Option<&Path>) -> String {
    let conventions = workspace_conventions(name, path);
    match user_append(path, agent_dir) {
        Some(user) => format!("{conventions}\n\n{user}"),
        None => conventions,
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

    // spec: DMN-030
    #[test]
    fn the_users_append_file_follows_the_conventions_and_the_workspace_one_wins() {
        let ws = tempfile::tempdir().unwrap();
        let agent = tempfile::tempdir().unwrap();
        let conventions = workspace_conventions("w", ws.path());
        assert_eq!(
            append_system_prompt("w", ws.path(), Some(agent.path())),
            conventions
        );

        std::fs::write(agent.path().join("APPEND_SYSTEM.md"), "  user rule\n").unwrap();
        assert_eq!(
            append_system_prompt("w", ws.path(), Some(agent.path())),
            format!("{conventions}\n\nuser rule")
        );

        std::fs::create_dir(ws.path().join(".omp")).unwrap();
        std::fs::write(ws.path().join(".omp/APPEND_SYSTEM.md"), "project rule").unwrap();
        assert_eq!(
            append_system_prompt("w", ws.path(), Some(agent.path())),
            format!("{conventions}\n\nproject rule")
        );
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
