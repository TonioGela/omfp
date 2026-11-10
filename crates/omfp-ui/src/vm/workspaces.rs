//! The Workspaces panel (UI-025): every workspace with its threads, git status and PR link; a workspace attached to
//! several threads carries a warning (DMN-027). Its Repositories section lists the registered repositories.

use std::collections::BTreeMap;

use omfp_protocol::{GitStatus, Repository, ThreadSummary, Workspace, WorkspaceKind};

use super::threads::workspace_label;

/// One row of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub id: String,
    pub label: String,
    pub path: String,
    /// Managed worktrees can be removed; adopted directories are only forgotten by the daemon.
    pub managed: bool,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    /// Non-archived threads attached to it: `(id, title)`.
    pub threads: Vec<(String, String)>,
    /// More than one thread works in it (DMN-027).
    pub shared: bool,
    /// `main · ↑2 ↓1 · 3 files +10 −4`, when the status is known.
    pub git: Option<String>,
}

/// Rows in index order.
#[must_use]
pub fn workspace_rows(
    workspaces: &[Workspace],
    repositories: &[Repository],
    threads: &[ThreadSummary],
    git: &BTreeMap<String, GitStatus>,
) -> Vec<WorkspaceRow> {
    workspaces
        .iter()
        .map(|workspace| {
            let attached: Vec<(String, String)> = threads
                .iter()
                .filter(|thread| {
                    thread.archived_at.is_none() && thread.workspace_ids.contains(&workspace.id)
                })
                .map(|thread| (thread.id.clone(), thread.title.clone()))
                .collect();
            WorkspaceRow {
                id: workspace.id.clone(),
                label: workspace_label(workspace, repositories),
                path: workspace.path.clone(),
                managed: workspace.kind == WorkspaceKind::ManagedWorktree,
                pr_number: workspace.pr_number,
                pr_url: workspace.pr_url.clone(),
                shared: attached.len() > 1,
                threads: attached,
                git: git.get(&workspace.id).map(git_summary),
            }
        })
        .collect()
}

/// One registered repository in the panel's Repositories section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub default_base_ref: String,
    /// Live managed worktrees created from it.
    pub worktrees: usize,
}

/// Repository rows in index order.
#[must_use]
pub fn repository_rows(
    repositories: &[Repository],
    workspaces: &[Workspace],
) -> Vec<RepositoryRow> {
    repositories
        .iter()
        .map(|repository| RepositoryRow {
            id: repository.id.clone(),
            name: repository.name.clone(),
            path: repository.path.clone(),
            default_base_ref: repository.default_base_ref.clone(),
            worktrees: workspaces
                .iter()
                .filter(|workspace| {
                    workspace.kind == WorkspaceKind::ManagedWorktree
                        && workspace.repository_id.as_deref() == Some(repository.id.as_str())
                })
                .count(),
        })
        .collect()
}

/// One-line git summary.
#[must_use]
pub fn git_summary(status: &GitStatus) -> String {
    let mut parts = vec![
        status
            .branch
            .clone()
            .unwrap_or_else(|| "detached".to_owned()),
    ];
    if status.ahead > 0 || status.behind > 0 {
        parts.push(format!("↑{} ↓{}", status.ahead, status.behind));
    }
    if status.files.is_empty() {
        parts.push("clean".to_owned());
    } else {
        let additions: u32 = status.files.iter().map(|file| file.additions).sum();
        let deletions: u32 = status.files.iter().map(|file| file.deletions).sum();
        let files = status.files.len();
        let noun = if files == 1 { "file" } else { "files" };
        parts.push(format!("{files} {noun} +{additions} −{deletions}"));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use omfp_protocol::{FileChange, FileStatus};

    use super::*;
    use crate::test_support::{repository, thread, workspace};

    // spec: UI-025, UI-011
    #[test]
    fn rows_list_threads_git_status_pr_and_flag_shared_workspaces() {
        let repositories = vec![repository("r", "fe")];
        let mut pr = workspace("w1", Some("r"), Some("feat"));
        pr.pr_number = Some(7);
        pr.pr_url = Some("https://example.test/pull/7".to_owned());
        let workspaces = vec![pr, workspace("w2", None, None)];
        let mut archived = thread("t3", "old", &["w2"]);
        archived.archived_at = Some(3);
        let threads = vec![
            thread("t1", "one", &["w1"]),
            thread("t2", "two", &["w1", "w2"]),
            archived,
        ];
        let status = GitStatus {
            branch: Some("feat".to_owned()),
            base_ref: Some("main".to_owned()),
            ahead: 2,
            behind: 0,
            files: vec![
                FileChange {
                    path: "a".to_owned(),
                    old_path: None,
                    status: FileStatus::Modified,
                    additions: 3,
                    deletions: 1,
                },
                FileChange {
                    path: "b".to_owned(),
                    old_path: None,
                    status: FileStatus::Added,
                    additions: 7,
                    deletions: 0,
                },
            ],
        };
        let git = BTreeMap::from([("w1".to_owned(), status)]);
        let rows = workspace_rows(&workspaces, &repositories, &threads, &git);
        assert_eq!(
            rows[0].threads,
            vec![
                ("t1".to_owned(), "one".to_owned()),
                ("t2".to_owned(), "two".to_owned())
            ]
        );
        assert!(rows[0].shared);
        assert!(rows[0].managed);
        assert_eq!(rows[0].pr_number, Some(7));
        assert_eq!(
            rows[0].git.as_deref(),
            Some("feat · ↑2 ↓0 · 2 files +10 −1")
        );
        assert_eq!(rows[1].threads, vec![("t2".to_owned(), "two".to_owned())]);
        assert!(!rows[1].shared);
        assert!(!rows[1].managed);
        assert_eq!(rows[1].git, None);
    }

    // spec: UI-025
    #[test]
    fn repository_rows_count_the_live_worktrees_of_each_repository() {
        let repositories = vec![repository("r-fe", "fe"), repository("r-be", "be")];
        let mut adopted = workspace("w-in-fe", None, None);
        adopted.repository_id = Some("r-fe".to_owned());
        let workspaces = vec![
            workspace("w1", Some("r-fe"), Some("a")),
            workspace("w2", Some("r-fe"), Some("b")),
            adopted,
        ];
        let rows = repository_rows(&repositories, &workspaces);
        assert_eq!(
            rows,
            vec![
                RepositoryRow {
                    id: "r-fe".to_owned(),
                    name: "fe".to_owned(),
                    path: "/src/fe".to_owned(),
                    default_base_ref: "main".to_owned(),
                    worktrees: 2,
                },
                RepositoryRow {
                    id: "r-be".to_owned(),
                    name: "be".to_owned(),
                    path: "/src/be".to_owned(),
                    default_base_ref: "main".to_owned(),
                    worktrees: 0,
                },
            ]
        );
    }
}
