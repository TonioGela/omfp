//! The thread list: state badges, workspace chips, grouping, filtering and the "next thread needing attention"
//! shortcut (UI-020..022). Workspaces and repositories are groupings, never parents.

use std::collections::BTreeMap;
use std::fmt;

use omfp_core::ProcessState;
use omfp_protocol::{RepoKind, ThreadSummary, Workspace, WorkspaceRepo};

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

/// A thread's workspace, or one of its repositories, as shown next to the thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceChip {
    /// Unique among a thread's chips: the workspace id, then `<workspace id>/<repository name>`.
    pub key: String,
    /// `workspace` (the thread's cwd), or the repository's kind: `link`, `worktree` or `clone`.
    pub kind: &'static str,
    /// The workspace's name, or the repository's `name:branch` (`name` on a detached HEAD).
    pub label: String,
}

impl WorkspaceChip {
    /// The chip of the workspace itself, the thread's cwd.
    #[must_use]
    pub fn is_workspace(&self) -> bool {
        self.kind == "workspace"
    }
}

/// The chip of workspace `workspace_id`, then one per repository in folder order; none for an unknown id.
#[must_use]
pub fn workspace_chips(workspace_id: &str, workspaces: &[Workspace]) -> Vec<WorkspaceChip> {
    let Some(workspace) = workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
    else {
        return Vec::new();
    };
    std::iter::once(WorkspaceChip {
        key: workspace.id.clone(),
        kind: "workspace",
        label: workspace.name.clone(),
    })
    .chain(workspace.repos.iter().map(|repo| WorkspaceChip {
        key: format!("{}/{}", workspace.id, repo.name),
        kind: repo_kind(&repo.kind),
        label: repo_label(repo),
    }))
    .collect()
}

/// `link`, `worktree` or `clone`.
#[must_use]
pub fn repo_kind(kind: &RepoKind) -> &'static str {
    match kind {
        RepoKind::Link { .. } => "link",
        RepoKind::Worktree { .. } => "worktree",
        RepoKind::Clone => "clone",
    }
}

/// `name:branch`, or `name` on a detached HEAD.
#[must_use]
pub fn repo_label(repo: &WorkspaceRepo) -> String {
    match &repo.branch {
        Some(branch) => format!("{}:{branch}", repo.name),
        None => repo.name.clone(),
    }
}

/// What makes repositories of different workspaces the same one: a worktree's main working tree, else the checkout.
fn repo_identity(repo: &WorkspaceRepo) -> &str {
    match &repo.kind {
        RepoKind::Worktree { main } => main,
        RepoKind::Link { .. } | RepoKind::Clone => &repo.canonical_path,
    }
}

/// A key threads can be grouped and filtered by (UI-022).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dimension {
    Tag,
    Workspace,
    Repository,
}

impl Dimension {
    pub const ALL: [Self; 3] = [Self::Tag, Self::Workspace, Self::Repository];

    /// Token used in form values.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::Workspace => "workspace",
            Self::Repository => "repository",
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
    /// Only archived threads; they are hidden otherwise (UI-026).
    pub archived: bool,
}

/// One grouping/filter key of a thread.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupKey {
    pub key: String,
    pub label: String,
}

/// The keys of `thread` along `dimension`, without duplicates, in folder order (tags in tag order).
#[must_use]
pub fn thread_keys(
    thread: &ThreadSummary,
    dimension: Dimension,
    workspaces: &[Workspace],
) -> Vec<GroupKey> {
    let workspace = workspaces
        .iter()
        .find(|workspace| workspace.id == thread.workspace_id);
    let keys: Vec<GroupKey> = match dimension {
        Dimension::Tag => thread
            .tags
            .iter()
            .map(|tag| GroupKey {
                key: tag.clone(),
                label: tag.clone(),
            })
            .collect(),
        Dimension::Workspace => workspace
            .map(|workspace| GroupKey {
                key: workspace.id.clone(),
                label: workspace.name.clone(),
            })
            .into_iter()
            .collect(),
        Dimension::Repository => workspace
            .into_iter()
            .flat_map(|workspace| &workspace.repos)
            .map(|repo| GroupKey {
                key: repo_identity(repo).to_owned(),
                label: repo.name.clone(),
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
) -> Vec<(Dimension, GroupKey)> {
    let mut options = Vec::new();
    for dimension in Dimension::ALL {
        let mut keys: Vec<GroupKey> = threads
            .iter()
            .filter(|thread| thread.archived_at.is_none())
            .flat_map(|thread| thread_keys(thread, dimension, workspaces))
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

/// The sidebar's sections: threads that pass `filter` (archived ones only under [`Filter::archived`], UI-026), in index
/// order within each group. With a grouping, a thread appears under every group it has a key for; threads without any
/// key come last in their own group.
#[must_use]
pub fn group_threads(
    threads: &[ThreadSummary],
    workspaces: &[Workspace],
    grouping: Grouping,
    filter: &Filter,
) -> Vec<Group> {
    let visible = threads
        .iter()
        .filter(|thread| thread.archived_at.is_some() == filter.archived)
        .filter(|thread| matches(thread, filter, workspaces));
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
        let keys = thread_keys(thread, dimension, workspaces);
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

fn matches(thread: &ThreadSummary, filter: &Filter, workspaces: &[Workspace]) -> bool {
    if let Some((dimension, key)) = &filter.key
        && !thread_keys(thread, *dimension, workspaces)
            .iter()
            .any(|candidate| candidate.key == *key)
    {
        return false;
    }
    let needle = filter.text.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let chips = workspace_chips(&thread.workspace_id, workspaces);
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
    use crate::test_support::{repo, thread, workspace};

    fn index() -> (Vec<Workspace>, Vec<ThreadSummary>) {
        let mut linked = repo("w-checkout", "be", "main");
        linked.kind = RepoKind::Link {
            target: "/src/be".to_owned(),
        };
        linked.canonical_path = "/src/be".to_owned();
        let mut checkout_workspace = workspace(
            "w-checkout",
            vec![repo("w-checkout", "fe", "feat/checkout"), linked],
        );
        checkout_workspace.name = "checkout".to_owned();
        let mut flaky_workspace = workspace("w-flaky", vec![repo("w-flaky", "be", "flaky")]);
        flaky_workspace.name = "flaky".to_owned();
        let mut scratch_workspace = workspace("w-scratch", Vec::new());
        scratch_workspace.name = "scratch".to_owned();
        let workspaces = vec![checkout_workspace, flaky_workspace, scratch_workspace];
        let mut checkout = thread("t-checkout", "checkout-v2", "w-checkout");
        checkout.tags = vec!["release".to_owned()];
        let mut flaky = thread("t-flaky", "flaky-tests", "w-flaky");
        flaky.tags = vec!["ci".to_owned(), "release".to_owned()];
        let scratch = thread("t-scratch", "scratch", "w-scratch");
        (workspaces, vec![checkout, flaky, scratch])
    }

    fn groups(grouping: Grouping, filter: &Filter) -> Vec<(String, Vec<String>)> {
        let (workspaces, threads) = index();
        group_threads(&threads, &workspaces, grouping, filter)
            .into_iter()
            .map(|group| (group.label, group.thread_ids))
            .collect()
    }

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    // spec: UI-020, UI-011
    #[test]
    fn status_puts_attention_before_the_process_and_the_settled_flag() {
        let mut summary = thread("t", "t", "w");
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

    // spec: UI-020, UI-011
    #[test]
    fn chips_show_the_workspace_then_each_repository_with_kind_and_branch() {
        let (workspaces, threads) = index();
        let chips = workspace_chips(&threads[0].workspace_id, &workspaces);
        let summary: Vec<(&str, &str, bool)> = chips
            .iter()
            .map(|chip| (chip.label.as_str(), chip.kind, chip.is_workspace()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("checkout", "workspace", true),
                ("fe:feat/checkout", "worktree", false),
                ("be:main", "link", false)
            ]
        );
        assert_eq!(workspace_chips("w-unknown", &workspaces), Vec::new());
    }

    // spec: UI-022, UI-011
    #[test]
    fn grouping_by_workspace_lists_each_thread_under_its_workspace() {
        assert_eq!(
            groups(Grouping::By(Dimension::Workspace), &Filter::default()),
            vec![
                ("checkout".to_owned(), ids(&["t-checkout"])),
                ("flaky".to_owned(), ids(&["t-flaky"])),
                ("scratch".to_owned(), ids(&["t-scratch"])),
            ]
        );
    }

    // spec: UI-022, UI-011
    #[test]
    fn grouping_by_repository_lists_a_thread_under_each_repository_of_its_workspace() {
        assert_eq!(
            groups(Grouping::By(Dimension::Repository), &Filter::default()),
            vec![
                ("be".to_owned(), ids(&["t-checkout", "t-flaky"])),
                ("fe".to_owned(), ids(&["t-checkout"])),
                ("No repository".to_owned(), ids(&["t-scratch"])),
            ]
        );
    }

    // spec: UI-022, UI-011
    #[test]
    fn grouping_by_tag_and_none() {
        assert_eq!(
            groups(Grouping::By(Dimension::Tag), &Filter::default()),
            vec![
                ("ci".to_owned(), ids(&["t-flaky"])),
                ("release".to_owned(), ids(&["t-checkout", "t-flaky"])),
                ("Untagged".to_owned(), ids(&["t-scratch"])),
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
            key: Some((Dimension::Repository, "/src/be".to_owned())),
            ..Filter::default()
        };
        assert_eq!(
            groups(Grouping::None, &by_repository)[0].1,
            ids(&["t-checkout", "t-flaky"])
        );
        let text = Filter {
            text: "FEAT/check".to_owned(),
            ..Filter::default()
        };
        assert_eq!(groups(Grouping::None, &text)[0].1, ids(&["t-checkout"]));
        let both = Filter {
            text: "ci".to_owned(),
            key: Some((Dimension::Tag, "release".to_owned())),
            ..Filter::default()
        };
        assert_eq!(groups(Grouping::None, &both)[0].1, ids(&["t-flaky"]));
    }

    // spec: UI-022
    #[test]
    fn archived_threads_are_neither_listed_nor_offered_as_filter_keys() {
        let (workspaces, mut threads) = index();
        threads[1].archived_at = Some(5);
        let listed = group_threads(&threads, &workspaces, Grouping::None, &Filter::default());
        assert_eq!(listed[0].thread_ids, ids(&["t-checkout", "t-scratch"]));
        let tags: Vec<String> = filter_options(&threads, &workspaces)
            .into_iter()
            .filter(|(dimension, _)| *dimension == Dimension::Tag)
            .map(|(_, key)| key.key)
            .collect();
        assert_eq!(tags, ids(&["release"]));
    }

    // spec: UI-026
    #[test]
    fn the_archived_filter_lists_only_archived_threads() {
        let (workspaces, mut threads) = index();
        threads[1].archived_at = Some(5);
        let archived = |text: &str| {
            group_threads(
                &threads,
                &workspaces,
                Grouping::None,
                &Filter {
                    text: text.to_owned(),
                    archived: true,
                    ..Filter::default()
                },
            )[0]
            .thread_ids
            .clone()
        };
        assert_eq!(archived(""), ids(&["t-flaky"]));
        assert_eq!(archived("checkout"), Vec::<String>::new());
    }

    // spec: UI-021
    #[test]
    fn next_needing_attention_wraps_around_after_the_current_thread() {
        let (_, mut threads) = index();
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
        let mut summary = thread("t", "t", "w");
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
