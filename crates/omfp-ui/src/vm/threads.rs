//! The thread list: state badges, workspace chips, grouping, filtering and the "next thread needing attention"
//! shortcut (UI-020..022). Workspaces and repositories are groupings, never parents.

use std::collections::BTreeMap;
use std::fmt;

use omfp_core::ProcessState;
use omfp_protocol::{Repository, ThreadSummary, Workspace};

/// What a thread's badge says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    /// Interactive requests wait for an answer.
    AwaitingInput,
    Failed,
    /// The agent is working (not settled).
    Working,
    Starting,
    Idle,
    Stopped,
    /// The session is owned by an interactive omp in a terminal.
    Terminal,
}

impl ThreadStatus {
    /// Of a thread summary: attention first, then the process, then omp's settled flag.
    #[must_use]
    pub fn of(thread: &ThreadSummary) -> Self {
        if thread.needs_attention {
            return Self::AwaitingInput;
        }
        match &thread.process {
            ProcessState::Failed { .. } => Self::Failed,
            ProcessState::Starting => Self::Starting,
            ProcessState::Terminal => Self::Terminal,
            ProcessState::Running if !thread.settled => Self::Working,
            ProcessState::Running => Self::Idle,
            ProcessState::Stopped => Self::Stopped,
        }
    }

    /// Human label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AwaitingInput => "awaiting input",
            Self::Failed => "failed",
            Self::Working => "running",
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Stopped => "stopped",
            Self::Terminal => "in terminal",
        }
    }

    /// Stable token for styling and `data-status`.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::AwaitingInput => "awaiting",
            Self::Failed => "failed",
            Self::Working => "running",
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Stopped => "stopped",
            Self::Terminal => "terminal",
        }
    }

    /// One-character glyph shown before the title (the token and label carry the meaning).
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            Self::AwaitingInput => "!",
            Self::Failed => "✕",
            Self::Working | Self::Starting => "◐",
            Self::Idle => "●",
            Self::Stopped => "○",
            Self::Terminal => "▣",
        }
    }
}

/// A workspace as shown next to a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceChip {
    pub workspace_id: String,
    /// `repo:branch`, `repo`, or the directory name of a workspace outside any repository.
    pub label: String,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    /// The first workspace is the thread's cwd.
    pub cwd: bool,
}

/// Chips of `workspace_ids` in order; unknown ids are skipped.
#[must_use]
pub fn workspace_chips(
    workspace_ids: &[String],
    workspaces: &[Workspace],
    repositories: &[Repository],
) -> Vec<WorkspaceChip> {
    workspace_ids
        .iter()
        .filter_map(|id| workspaces.iter().find(|workspace| workspace.id == *id))
        .enumerate()
        .map(|(index, workspace)| WorkspaceChip {
            workspace_id: workspace.id.clone(),
            label: workspace_label(workspace, repositories),
            pr_number: workspace.pr_number,
            pr_url: workspace.pr_url.clone(),
            cwd: index == 0,
        })
        .collect()
}

/// `repo:branch`, `repo`, or the last path component.
#[must_use]
pub fn workspace_label(workspace: &Workspace, repositories: &[Repository]) -> String {
    let repository =
        repository_of(workspace, repositories).map(|repository| repository.name.as_str());
    match (repository, workspace.branch.as_deref()) {
        (Some(repository), Some(branch)) => format!("{repository}:{branch}"),
        (Some(repository), None) => repository.to_owned(),
        (None, _) => basename(&workspace.path).to_owned(),
    }
}

fn repository_of<'a>(
    workspace: &Workspace,
    repositories: &'a [Repository],
) -> Option<&'a Repository> {
    let id = workspace.repository_id.as_deref()?;
    repositories.iter().find(|repository| repository.id == id)
}

/// Last non-empty path component.
#[must_use]
pub fn basename(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

/// A key threads can be grouped and filtered by (UI-022).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dimension {
    Tag,
    Workspace,
    Repository,
    Pr,
}

impl Dimension {
    pub const ALL: [Self; 4] = [Self::Tag, Self::Workspace, Self::Repository, Self::Pr];

    /// Token used in form values.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::Workspace => "workspace",
            Self::Repository => "repository",
            Self::Pr => "pr",
        }
    }

    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|dimension| dimension.token() == token)
    }

    /// Label of the group holding threads without any key of this dimension.
    fn none_label(self) -> &'static str {
        match self {
            Self::Tag => "Untagged",
            Self::Workspace => "No workspace",
            Self::Repository => "No repository",
            Self::Pr => "No PR",
        }
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.token())
    }
}

/// How the sidebar groups threads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Grouping {
    #[default]
    None,
    By(Dimension),
}

impl Grouping {
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::By(dimension) => dimension.token(),
        }
    }

    pub fn from_token(token: &str) -> Self {
        Dimension::from_token(token).map_or(Self::None, Self::By)
    }
}

/// Which threads the sidebar shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Case-insensitive match on title, tags and workspace chip labels.
    pub text: String,
    /// Only threads with this key.
    pub key: Option<(Dimension, String)>,
}

/// One grouping/filter key of a thread.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupKey {
    pub key: String,
    pub label: String,
}

/// The keys of `thread` along `dimension`, without duplicates, in workspace order (tags in tag order).
#[must_use]
pub fn thread_keys(
    thread: &ThreadSummary,
    dimension: Dimension,
    workspaces: &[Workspace],
    repositories: &[Repository],
) -> Vec<GroupKey> {
    let attached = thread
        .workspace_ids
        .iter()
        .filter_map(|id| workspaces.iter().find(|workspace| workspace.id == *id));
    let keys: Vec<GroupKey> = match dimension {
        Dimension::Tag => thread
            .tags
            .iter()
            .map(|tag| GroupKey {
                key: tag.clone(),
                label: tag.clone(),
            })
            .collect(),
        Dimension::Workspace => attached
            .map(|workspace| GroupKey {
                key: workspace.id.clone(),
                label: workspace_label(workspace, repositories),
            })
            .collect(),
        Dimension::Repository => attached
            .filter_map(|workspace| repository_of(workspace, repositories))
            .map(|repository| GroupKey {
                key: repository.id.clone(),
                label: repository.name.clone(),
            })
            .collect(),
        Dimension::Pr => attached
            .filter_map(|workspace| {
                let number = workspace.pr_number?;
                let prefix = repository_of(workspace, repositories).map_or_else(
                    || basename(&workspace.path).to_owned(),
                    |repository| repository.name.clone(),
                );
                let label = format!("{prefix}#{number}");
                Some(GroupKey {
                    key: label.clone(),
                    label,
                })
            })
            .collect(),
    };
    let mut unique: Vec<GroupKey> = Vec::with_capacity(keys.len());
    for key in keys {
        if !unique.iter().any(|seen| seen.key == key.key) {
            unique.push(key);
        }
    }
    unique
}

/// Every key along every dimension, for the filter selector, sorted per dimension.
#[must_use]
pub fn filter_options(
    threads: &[ThreadSummary],
    workspaces: &[Workspace],
    repositories: &[Repository],
) -> Vec<(Dimension, GroupKey)> {
    let mut options = Vec::new();
    for dimension in Dimension::ALL {
        let mut keys: Vec<GroupKey> = threads
            .iter()
            .filter(|thread| thread.archived_at.is_none())
            .flat_map(|thread| thread_keys(thread, dimension, workspaces, repositories))
            .collect();
        keys.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.key.cmp(&b.key)));
        keys.dedup_by(|a, b| a.key == b.key);
        options.extend(keys.into_iter().map(|key| (dimension, key)));
    }
    options
}

/// One section of the thread list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Empty for the single group of [`Grouping::None`] and for the "without key" group.
    pub key: String,
    pub label: String,
    pub thread_ids: Vec<String>,
}

/// The sidebar's sections: non-archived threads that pass `filter`, in index order within each group. With a grouping,
/// a thread appears under every group it has a key for; threads without any key come last in their own group.
#[must_use]
pub fn group_threads(
    threads: &[ThreadSummary],
    workspaces: &[Workspace],
    repositories: &[Repository],
    grouping: Grouping,
    filter: &Filter,
) -> Vec<Group> {
    let visible = threads
        .iter()
        .filter(|thread| thread.archived_at.is_none())
        .filter(|thread| matches(thread, filter, workspaces, repositories));
    let Grouping::By(dimension) = grouping else {
        return vec![Group {
            key: String::new(),
            label: "Threads".to_owned(),
            thread_ids: visible.map(|thread| thread.id.clone()).collect(),
        }];
    };
    let mut groups: Vec<Group> = Vec::new();
    let mut without = Vec::new();
    for thread in visible {
        let keys = thread_keys(thread, dimension, workspaces, repositories);
        if keys.is_empty() {
            without.push(thread.id.clone());
        }
        for key in keys {
            match groups.iter_mut().find(|group| group.key == key.key) {
                Some(group) => group.thread_ids.push(thread.id.clone()),
                None => groups.push(Group {
                    key: key.key,
                    label: key.label,
                    thread_ids: vec![thread.id.clone()],
                }),
            }
        }
    }
    groups.sort_by(|a, b| a.label.cmp(&b.label));
    if !without.is_empty() {
        groups.push(Group {
            key: String::new(),
            label: dimension.none_label().to_owned(),
            thread_ids: without,
        });
    }
    groups
}

fn matches(
    thread: &ThreadSummary,
    filter: &Filter,
    workspaces: &[Workspace],
    repositories: &[Repository],
) -> bool {
    if let Some((dimension, key)) = &filter.key
        && !thread_keys(thread, *dimension, workspaces, repositories)
            .iter()
            .any(|candidate| candidate.key == *key)
    {
        return false;
    }
    let needle = filter.text.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let chips = workspace_chips(&thread.workspace_ids, workspaces, repositories);
    std::iter::once(thread.title.as_str())
        .chain(thread.tags.iter().map(String::as_str))
        .chain(chips.iter().map(|chip| chip.label.as_str()))
        .any(|text| text.to_lowercase().contains(&needle))
}

/// The next thread awaiting input after `current` in index order, wrapping around; `current` itself only when it is
/// the only one (UI-021).
#[must_use]
pub fn next_needing_attention(threads: &[ThreadSummary], current: Option<&str>) -> Option<String> {
    let candidates: Vec<&ThreadSummary> = threads
        .iter()
        .filter(|thread| thread.archived_at.is_none())
        .collect();
    let start = current
        .and_then(|current| candidates.iter().position(|thread| thread.id == current))
        .map_or(0, |position| position + 1);
    (0..candidates.len())
        .map(|offset| candidates[(start + offset) % candidates.len()])
        .find(|thread| thread.needs_attention)
        .map(|thread| thread.id.clone())
}

/// A thread has news the user has not seen: it changed after the last time it was shown (UI-020). The selected thread
/// is never unread; a thread the window has never observed has no baseline yet and is not unread.
#[must_use]
pub fn is_unread(
    thread: &ThreadSummary,
    seen: &BTreeMap<String, i64>,
    selected: Option<&str>,
) -> bool {
    selected != Some(thread.id.as_str())
        && seen
            .get(&thread.id)
            .is_some_and(|seen| thread.updated_at > *seen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{repository, thread, workspace};

    fn index() -> (Vec<Repository>, Vec<Workspace>, Vec<ThreadSummary>) {
        let repositories = vec![repository("r-fe", "fe"), repository("r-be", "be")];
        let mut pr = workspace("w-fe412", Some("r-fe"), Some("feat/checkout"));
        pr.pr_number = Some(412);
        pr.pr_url = Some("https://example.test/fe/pull/412".to_owned());
        let workspaces = vec![
            pr,
            workspace("w-be", Some("r-be"), Some("main")),
            workspace("w-be2", Some("r-be"), Some("flaky")),
            workspace("w-scratch", None, None),
        ];
        let mut checkout = thread("t-checkout", "checkout-v2", &["w-fe412", "w-be"]);
        checkout.tags = vec!["release".to_owned()];
        let mut flaky = thread("t-flaky", "flaky-tests", &["w-be", "w-be2"]);
        flaky.tags = vec!["ci".to_owned(), "release".to_owned()];
        let scratch = thread("t-scratch", "scratch", &[]);
        (repositories, workspaces, vec![checkout, flaky, scratch])
    }

    fn groups(grouping: Grouping, filter: &Filter) -> Vec<(String, Vec<String>)> {
        let (repositories, workspaces, threads) = index();
        group_threads(&threads, &workspaces, &repositories, grouping, filter)
            .into_iter()
            .map(|group| (group.label, group.thread_ids))
            .collect()
    }

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    // spec: UI-020
    #[test]
    fn status_puts_attention_before_the_process_and_the_settled_flag() {
        let mut summary = thread("t", "t", &[]);
        assert_eq!(ThreadStatus::of(&summary), ThreadStatus::Idle);
        summary.settled = false;
        assert_eq!(ThreadStatus::of(&summary), ThreadStatus::Working);
        summary.needs_attention = true;
        assert_eq!(ThreadStatus::of(&summary), ThreadStatus::AwaitingInput);
        summary.needs_attention = false;
        summary.process = ProcessState::Failed {
            message: "boom".to_owned(),
            exit_code: None,
            stderr_tail: String::new(),
        };
        assert_eq!(ThreadStatus::of(&summary), ThreadStatus::Failed);
        summary.process = ProcessState::Stopped;
        assert_eq!(ThreadStatus::of(&summary), ThreadStatus::Stopped);
    }

    // spec: UI-020
    #[test]
    fn chips_show_repository_branch_and_pr_and_mark_the_first_as_cwd() {
        let (repositories, workspaces, threads) = index();
        let chips = workspace_chips(&threads[0].workspace_ids, &workspaces, &repositories);
        let summary: Vec<(&str, Option<u64>, bool)> = chips
            .iter()
            .map(|chip| (chip.label.as_str(), chip.pr_number, chip.cwd))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("fe:feat/checkout", Some(412), true),
                ("be:main", None, false)
            ]
        );
        let scratch = workspace_chips(&["w-scratch".to_owned()], &workspaces, &repositories);
        assert_eq!(scratch[0].label, "w-scratch");
    }

    // spec: UI-022
    #[test]
    fn grouping_by_workspace_lists_a_thread_under_each_of_its_workspaces() {
        assert_eq!(
            groups(Grouping::By(Dimension::Workspace), &Filter::default()),
            vec![
                ("be:flaky".to_owned(), ids(&["t-flaky"])),
                ("be:main".to_owned(), ids(&["t-checkout", "t-flaky"])),
                ("fe:feat/checkout".to_owned(), ids(&["t-checkout"])),
                ("No workspace".to_owned(), ids(&["t-scratch"])),
            ]
        );
    }

    // spec: UI-022
    #[test]
    fn grouping_by_repository_lists_a_thread_once_per_repository() {
        assert_eq!(
            groups(Grouping::By(Dimension::Repository), &Filter::default()),
            vec![
                ("be".to_owned(), ids(&["t-checkout", "t-flaky"])),
                ("fe".to_owned(), ids(&["t-checkout"])),
                ("No repository".to_owned(), ids(&["t-scratch"])),
            ]
        );
    }

    // spec: UI-022
    #[test]
    fn grouping_by_tag_and_pr_and_none() {
        assert_eq!(
            groups(Grouping::By(Dimension::Tag), &Filter::default()),
            vec![
                ("ci".to_owned(), ids(&["t-flaky"])),
                ("release".to_owned(), ids(&["t-checkout", "t-flaky"])),
                ("Untagged".to_owned(), ids(&["t-scratch"])),
            ]
        );
        assert_eq!(
            groups(Grouping::By(Dimension::Pr), &Filter::default()),
            vec![
                ("fe#412".to_owned(), ids(&["t-checkout"])),
                ("No PR".to_owned(), ids(&["t-flaky", "t-scratch"])),
            ]
        );
        assert_eq!(
            groups(Grouping::None, &Filter::default()),
            vec![(
                "Threads".to_owned(),
                ids(&["t-checkout", "t-flaky", "t-scratch"])
            )]
        );
    }

    // spec: UI-022
    #[test]
    fn filtering_by_key_and_free_text_combines() {
        let by_repository = Filter {
            text: String::new(),
            key: Some((Dimension::Repository, "r-be".to_owned())),
        };
        assert_eq!(
            groups(Grouping::None, &by_repository)[0].1,
            ids(&["t-checkout", "t-flaky"])
        );
        let text = Filter {
            text: "FEAT/check".to_owned(),
            key: None,
        };
        assert_eq!(groups(Grouping::None, &text)[0].1, ids(&["t-checkout"]));
        let both = Filter {
            text: "ci".to_owned(),
            key: Some((Dimension::Tag, "release".to_owned())),
        };
        assert_eq!(groups(Grouping::None, &both)[0].1, ids(&["t-flaky"]));
        let by_pr = Filter {
            text: String::new(),
            key: Some((Dimension::Pr, "fe#412".to_owned())),
        };
        assert_eq!(groups(Grouping::By(Dimension::Workspace), &by_pr).len(), 2);
    }

    // spec: UI-022
    #[test]
    fn archived_threads_are_neither_listed_nor_offered_as_filter_keys() {
        let (repositories, workspaces, mut threads) = index();
        threads[1].archived_at = Some(5);
        let listed = group_threads(
            &threads,
            &workspaces,
            &repositories,
            Grouping::None,
            &Filter::default(),
        );
        assert_eq!(listed[0].thread_ids, ids(&["t-checkout", "t-scratch"]));
        let tags: Vec<String> = filter_options(&threads, &workspaces, &repositories)
            .into_iter()
            .filter(|(dimension, _)| *dimension == Dimension::Tag)
            .map(|(_, key)| key.key)
            .collect();
        assert_eq!(tags, ids(&["release"]));
    }

    // spec: UI-021
    #[test]
    fn next_needing_attention_wraps_around_after_the_current_thread() {
        let (_, _, mut threads) = index();
        assert_eq!(next_needing_attention(&threads, None), None);
        threads[0].needs_attention = true;
        threads[2].needs_attention = true;
        assert_eq!(
            next_needing_attention(&threads, None).as_deref(),
            Some("t-checkout")
        );
        assert_eq!(
            next_needing_attention(&threads, Some("t-checkout")).as_deref(),
            Some("t-scratch")
        );
        assert_eq!(
            next_needing_attention(&threads, Some("t-scratch")).as_deref(),
            Some("t-checkout")
        );
        threads[2].needs_attention = false;
        assert_eq!(
            next_needing_attention(&threads, Some("t-checkout")).as_deref(),
            Some("t-checkout")
        );
    }

    // spec: UI-020
    #[test]
    fn a_thread_is_unread_when_it_changed_since_it_was_last_seen_and_is_not_selected() {
        let mut summary = thread("t", "t", &[]);
        summary.updated_at = 10;
        let mut seen = BTreeMap::new();
        assert!(!is_unread(&summary, &seen, None));
        seen.insert("t".to_owned(), 10);
        assert!(!is_unread(&summary, &seen, None));
        summary.updated_at = 11;
        assert!(is_unread(&summary, &seen, None));
        assert!(!is_unread(&summary, &seen, Some("t")));
    }
}
