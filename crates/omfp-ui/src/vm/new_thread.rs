//! The new-thread dialog (UI-023): the thread gets a new workspace folder named after its title, with any number of
//! repositories — new worktrees of a repository (registered or by path), linked directories — all optional, in one
//! form.

use omfp_protocol::RepoSource;

use crate::intent::{Intent, NewThread};

use super::header::parse_tags;

/// Title used when the form leaves it blank.
pub const DEFAULT_TITLE: &str = "New thread";

/// One repository line of the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoChoice {
    /// `repo` is a registered repository id or a path.
    NewWorktree {
        repo: String,
        branch: String,
        base_ref: String,
    },
    Link {
        path: String,
    },
}

impl RepoChoice {
    #[must_use]
    pub fn token(&self) -> &'static str {
        match self {
            Self::NewWorktree { .. } => "worktree",
            Self::Link { .. } => "link",
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
    pub repos: Vec<RepoChoice>,
}

impl NewThreadForm {
    /// The `CreateThread` intent, or the first problem with the form.
    pub fn to_intent(&self) -> Result<Intent, String> {
        let mut repos = Vec::with_capacity(self.repos.len());
        for (index, choice) in self.repos.iter().enumerate() {
            let line = index + 1;
            let source = match choice {
                RepoChoice::NewWorktree {
                    repo,
                    branch,
                    base_ref,
                } => RepoSource::Worktree {
                    repo: required(repo, line, "a repository or path")?,
                    branch: optional(branch),
                    base_ref: optional(base_ref),
                },
                RepoChoice::Link { path } => RepoSource::Link {
                    path: required(path, line, "a directory")?,
                },
            };
            if repos.contains(&source) {
                return Err(format!("Repository {line} is listed twice"));
            }
            repos.push(source);
        }
        let title = self.title.trim();
        Ok(Intent::CreateThread(NewThread {
            title: if title.is_empty() {
                DEFAULT_TITLE.to_owned()
            } else {
                title.to_owned()
            },
            tags: parse_tags(&self.tags),
            repos,
            prompt: optional(&self.prompt),
        }))
    }
}

fn required(value: &str, line: usize, what: &str) -> Result<String, String> {
    optional(value).ok_or_else(|| format!("Repository {line}: enter {what}"))
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
    fn a_thread_without_repositories_is_valid() {
        let intent = NewThreadForm::default().to_intent();
        assert_eq!(
            intent,
            Ok(Intent::CreateThread(NewThread {
                title: DEFAULT_TITLE.to_owned(),
                tags: Vec::new(),
                repos: Vec::new(),
                prompt: None,
            }))
        );
    }

    // spec: UI-023
    #[test]
    fn worktrees_and_links_combine_in_order() {
        let form = NewThreadForm {
            title: " checkout ".to_owned(),
            tags: "release, fe".to_owned(),
            prompt: "Fix it".to_owned(),
            repos: vec![
                RepoChoice::NewWorktree {
                    repo: "/src/be".to_owned(),
                    branch: String::new(),
                    base_ref: " main ".to_owned(),
                },
                RepoChoice::Link {
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
            thread.repos,
            vec![
                RepoSource::Worktree {
                    repo: "/src/be".to_owned(),
                    branch: None,
                    base_ref: Some("main".to_owned())
                },
                RepoSource::Link {
                    path: "/tmp/scratch".to_owned()
                },
            ]
        );
    }

    // spec: UI-023
    #[test]
    fn incomplete_or_duplicate_lines_are_refused() {
        let mut form = NewThreadForm {
            repos: vec![RepoChoice::NewWorktree {
                repo: " ".to_owned(),
                branch: String::new(),
                base_ref: String::new(),
            }],
            ..NewThreadForm::default()
        };
        assert_eq!(
            form.to_intent(),
            Err("Repository 1: enter a repository or path".to_owned())
        );
        form.repos = vec![
            RepoChoice::Link {
                path: "/tmp/x".to_owned(),
            },
            RepoChoice::Link {
                path: "/tmp/x".to_owned(),
            },
        ];
        assert_eq!(
            form.to_intent(),
            Err("Repository 2 is listed twice".to_owned())
        );
    }
}
