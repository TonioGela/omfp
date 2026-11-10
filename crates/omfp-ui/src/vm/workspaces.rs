//! The Workspaces panel (UI-025): every workspace with its threads and its repositories with their git status; a
//! workspace several threads live in carries a warning (DMN-027). Its Repositories section lists the registered
//! repositories.

use std::collections::BTreeMap;

use omfp_protocol::{GitStatus, RepoKind, RepoStatus, Repository, ThreadSummary, Workspace};

use super::threads::{repo_kind, repo_label};

/// One row of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub id: String,
    pub name: String,
    pub path: String,
    /// The daemon created the folder.
    pub owned: bool,
    /// Non-archived threads living in it: `(id, title)`.
    pub threads: Vec<(String, String)>,
    /// More than one thread works in it (DMN-027).
    pub shared: bool,
    pub repos: Vec<RepoRow>,
}

/// One repository of a workspace row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRow {
    /// Entry name in the folder.
    pub name: String,
    /// `link`, `worktree` or `clone`.
    pub kind: &'static str,
    /// `name:branch`, or `name` on a detached HEAD.
    pub label: String,
    /// `main · ↑2 ↓1 · 3 files +10 −4`, when the status is known.
    pub git: Option<String>,
}

/// Rows in index order.
#[must_use]
pub fn workspace_rows(
    workspaces: &[Workspace],
    threads: &[ThreadSummary],
    git: &BTreeMap<String, Vec<RepoStatus>>,
) -> Vec<WorkspaceRow> {
    workspaces
        .iter()
        .map(|workspace| {
            let living: Vec<(String, String)> = threads
                .iter()
                .filter(|thread| {
                    thread.archived_at.is_none() && thread.workspace_id == workspace.id
                })
                .map(|thread| (thread.id.clone(), thread.title.clone()))
                .collect();
            let statuses = git.get(&workspace.id);
            WorkspaceRow {
                id: workspace.id.clone(),
                name: workspace.name.clone(),
                path: workspace.path.clone(),
                owned: workspace.owned,
                shared: living.len() > 1,
                threads: living,
                repos: workspace
                    .repos
                    .iter()
                    .map(|repo| RepoRow {
                        name: repo.name.clone(),
                        kind: repo_kind(&repo.kind),
                        label: repo_label(repo),
                        git: statuses
                            .and_then(|statuses| {
                                statuses.iter().find(|status| status.name == repo.name)
                            })
                            .map(|status| git_summary(&status.status)),
                    })
                    .collect(),
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
    /// Worktrees of it in workspaces.
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
        .map(|repository| {
            RepositoryRow {
            id: repository.id.clone(),
            name: repository.name.clone(),
            path: repository.path.clone(),
            default_base_ref: repository.default_base_ref.clone(),
            worktrees: workspaces
                .iter()
                .flat_map(|workspace| &workspace.repos)
                .filter(|repo| {
                    matches!(&repo.kind, RepoKind::Worktree { main } if *main == repository.path)
                })
                .count(),
        }
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
    use crate::test_support::{repo, repository, thread, workspace};

    // spec: UI-025, UI-011
    #[test]
    fn rows_list_threads_repositories_with_git_status_and_flag_shared_workspaces() {
        let workspaces = vec![
            workspace(
                "w1",
                vec![repo("w1", "fe", "feat"), repo("w1", "be", "main")],
            ),
            workspace("w2", Vec::new()),
        ];
        let mut archived = thread("t3", "old", "w2");
        archived.archived_at = Some(3);
        let threads = vec![
            thread("t1", "one", "w1"),
            thread("t2", "two", "w1"),
            archived,
        ];
        let status = GitStatus {
            branch: Some("feat".to_owned()),
            base_ref: Some("main".to_owned()),
            ahead: 2,
            behind: 0,
            dirty: true,
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
        let git = BTreeMap::from([(
            "w1".to_owned(),
            vec![RepoStatus {
                name: "fe".to_owned(),
                status,
            }],
        )]);
        let rows = workspace_rows(&workspaces, &threads, &git);
        assert_eq!(
            rows[0].threads,
            vec![
                ("t1".to_owned(), "one".to_owned()),
                ("t2".to_owned(), "two".to_owned())
            ]
        );
        assert!(rows[0].shared);
        assert!(rows[0].owned);
        assert_eq!(
            rows[0].repos,
            vec![
                RepoRow {
                    name: "fe".to_owned(),
                    kind: "worktree",
                    label: "fe:feat".to_owned(),
                    git: Some("feat · ↑2 ↓0 · 2 files +10 −1".to_owned()),
                },
                RepoRow {
                    name: "be".to_owned(),
                    kind: "worktree",
                    label: "be:main".to_owned(),
                    git: None,
                },
            ]
        );
        assert_eq!(rows[1].threads, Vec::new());
        assert!(!rows[1].shared);
        assert_eq!(rows[1].repos, Vec::new());
    }

    // spec: UI-025
    #[test]
    fn repository_rows_count_the_worktrees_of_each_repository() {
        let repositories = vec![repository("r-fe", "fe"), repository("r-be", "be")];
        let mut linked = repo("w2", "be", "main");
        linked.kind = RepoKind::Link {
            target: "/src/be".to_owned(),
        };
        let workspaces = vec![
            workspace("w1", vec![repo("w1", "fe", "a")]),
            workspace("w2", vec![repo("w2", "fe", "b"), linked]),
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
