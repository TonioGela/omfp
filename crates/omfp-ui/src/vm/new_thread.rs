//! The new-thread dialog (UI-023): any combination of workspaces — none, existing ones, new worktrees from a
//! repository (registered or by path), from a PR, adopted directories — all optional, in one form.

use crate::intent::{Intent, NewThread, WorkspaceSource};

use super::header::parse_tags;

/// Title used when the form leaves it blank.
pub const DEFAULT_TITLE: &str = "New thread";

/// One workspace line of the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceChoice {
    Existing {
        workspace_id: String,
    },
    /// `repo` is a registered repository id or a path.
    NewWorktree {
        repo: String,
        branch: String,
        base_ref: String,
    },
    FromPr {
        pr: String,
        repo: String,
    },
    Adopt {
        path: String,
    },
}

impl WorkspaceChoice {
    #[must_use]
    pub fn token(&self) -> &'static str {
        match self {
            Self::Existing { .. } => "existing",
            Self::NewWorktree { .. } => "worktree",
            Self::FromPr { .. } => "pr",
            Self::Adopt { .. } => "adopt",
        }
    }
}

/// The dialog's fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewThreadForm {
    pub title: String,
    /// Comma-separated.
    pub tags: String,
    pub prompt: String,
    pub workspaces: Vec<WorkspaceChoice>,
}

impl NewThreadForm {
    /// The `CreateThread` intent, or the first problem with the form.
    pub fn to_intent(&self) -> Result<Intent, String> {
        let mut workspaces = Vec::with_capacity(self.workspaces.len());
        for (index, choice) in self.workspaces.iter().enumerate() {
            let line = index + 1;
            let source = match choice {
                WorkspaceChoice::Existing { workspace_id } if workspace_id.is_empty() => {
                    return Err(format!("Workspace {line}: pick a workspace"));
                }
                WorkspaceChoice::Existing { workspace_id } => WorkspaceSource::Existing {
                    workspace_id: workspace_id.clone(),
                },
                WorkspaceChoice::NewWorktree {
                    repo,
                    branch,
                    base_ref,
                } => WorkspaceSource::NewWorktree {
                    repo: required(repo, line, "a repository or path")?,
                    branch: optional(branch),
                    base_ref: optional(base_ref),
                },
                WorkspaceChoice::FromPr { pr, repo } => WorkspaceSource::FromPr {
                    pr: required(pr, line, "a PR")?,
                    repo: optional(repo),
                },
                WorkspaceChoice::Adopt { path } => WorkspaceSource::Adopt {
                    path: required(path, line, "a directory")?,
                },
            };
            if workspaces.contains(&source) {
                return Err(format!("Workspace {line} is listed twice"));
            }
            workspaces.push(source);
        }
        let title = self.title.trim();
        Ok(Intent::CreateThread(NewThread {
            title: if title.is_empty() {
                DEFAULT_TITLE.to_owned()
            } else {
                title.to_owned()
            },
            tags: parse_tags(&self.tags),
            workspaces,
            prompt: optional(&self.prompt),
        }))
    }
}

fn required(value: &str, line: usize, what: &str) -> Result<String, String> {
    optional(value).ok_or_else(|| format!("Workspace {line}: enter {what}"))
}

fn optional(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: UI-023
    #[test]
    fn a_thread_without_workspaces_is_valid() {
        let intent = NewThreadForm::default().to_intent();
        assert_eq!(
            intent,
            Ok(Intent::CreateThread(NewThread {
                title: DEFAULT_TITLE.to_owned(),
                tags: Vec::new(),
                workspaces: Vec::new(),
                prompt: None,
            }))
        );
    }

    // spec: UI-023
    #[test]
    fn every_kind_of_workspace_combines_in_order() {
        let form = NewThreadForm {
            title: " checkout ".to_owned(),
            tags: "release, fe".to_owned(),
            prompt: "Fix it".to_owned(),
            workspaces: vec![
                WorkspaceChoice::Existing {
                    workspace_id: "w1".to_owned(),
                },
                WorkspaceChoice::NewWorktree {
                    repo: "/src/be".to_owned(),
                    branch: String::new(),
                    base_ref: " main ".to_owned(),
                },
                WorkspaceChoice::FromPr {
                    pr: "acme/fe#412".to_owned(),
                    repo: String::new(),
                },
                WorkspaceChoice::Adopt {
                    path: "/tmp/scratch".to_owned(),
                },
            ],
        };
        let Ok(Intent::CreateThread(thread)) = form.to_intent() else {
            panic!("valid form")
        };
        assert_eq!(
            (thread.title.as_str(), thread.prompt.as_deref()),
            ("checkout", Some("Fix it"))
        );
        assert_eq!(thread.tags, vec!["release".to_owned(), "fe".to_owned()]);
        assert_eq!(
            thread.workspaces,
            vec![
                WorkspaceSource::Existing {
                    workspace_id: "w1".to_owned()
                },
                WorkspaceSource::NewWorktree {
                    repo: "/src/be".to_owned(),
                    branch: None,
                    base_ref: Some("main".to_owned())
                },
                WorkspaceSource::FromPr {
                    pr: "acme/fe#412".to_owned(),
                    repo: None
                },
                WorkspaceSource::Adopt {
                    path: "/tmp/scratch".to_owned()
                },
            ]
        );
    }

    // spec: UI-023
    #[test]
    fn incomplete_or_duplicate_lines_are_refused() {
        let mut form = NewThreadForm {
            workspaces: vec![WorkspaceChoice::NewWorktree {
                repo: " ".to_owned(),
                branch: String::new(),
                base_ref: String::new(),
            }],
            ..NewThreadForm::default()
        };
        assert_eq!(
            form.to_intent(),
            Err("Workspace 1: enter a repository or path".to_owned())
        );
        form.workspaces = vec![
            WorkspaceChoice::Existing {
                workspace_id: "w1".to_owned(),
            },
            WorkspaceChoice::Existing {
                workspace_id: "w1".to_owned(),
            },
        ];
        assert_eq!(
            form.to_intent(),
            Err("Workspace 2 is listed twice".to_owned())
        );
    }
}
