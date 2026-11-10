//! The shell: sidebar, panels, new-thread dialog, connection banner, theme, live region.

use std::collections::BTreeMap;

use omfp_protocol::{FileChange, FileStatus, GitStatus};

use super::{Page, attr, index, normalize, render, selecting, text};
use crate::local::Theme;
use crate::panels::Placement;
use crate::vm::new_thread::{NewThreadForm, WorkspaceChoice};
use crate::vm::threads::{Dimension, Filter, Grouping};
use crate::{ConnectionView, UiState, ViewInput};

fn rows(page: &Page) -> Vec<(String, String)> {
    page.testid("thread-row")
        .into_iter()
        .map(|row| {
            (
                attr(row, "data-thread-id").unwrap_or_default().to_owned(),
                attr(row, "data-status").unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

// spec: UI-020, TST-031
#[test]
fn the_sidebar_lists_threads_with_state_badges_unread_marks_and_workspace_chips() {
    let mut input = index();
    input.threads[3].updated_at = 9;
    let ui = UiState {
        seen: BTreeMap::from([("t-idle".to_owned(), 1)]),
        ..UiState::default()
    };
    let page = Page::new(input, ui);
    assert_eq!(
        rows(&page),
        pairs(&[
            ("t-checkout", "awaiting"),
            ("t-flaky", "running"),
            ("t-scratch", "failed"),
            ("t-idle", "idle")
        ])
    );
    let badges: Vec<String> = page.testid("thread-status").into_iter().map(text).collect();
    assert_eq!(
        badges,
        vec!["!awaiting input", "◐running", "✕failed", "●idle"]
    );
    let unread: Vec<&str> = page
        .testid("thread-row")
        .into_iter()
        .filter(|row| text(*row).contains('•'))
        .filter_map(|row| attr(row, "data-thread-id"))
        .collect();
    assert_eq!(unread, vec!["t-idle"]);
    let checkout = page.all("[data-thread-id=\"t-checkout\"] [data-testid=\"workspace-chip\"]");
    let chips: Vec<(String, Option<&str>)> = checkout
        .into_iter()
        .map(|chip| (text(chip), attr(chip, "data-cwd")))
        .collect();
    assert_eq!(
        chips,
        vec![
            ("⌂fe:feat/checkout#412".to_owned(), Some("true")),
            ("be:main".to_owned(), Some("false"))
        ]
    );
}

// spec: UI-022
#[test]
fn grouping_by_workspace_shows_a_thread_under_each_of_its_workspaces() {
    let ui = UiState {
        grouping: Grouping::By(Dimension::Workspace),
        ..UiState::default()
    };
    let page = Page::new(index(), ui);
    let groups: Vec<(String, Vec<String>)> = page
        .testid("thread-group")
        .into_iter()
        .map(|group| {
            let label = group
                .select(&scraper::Selector::parse("h3").expect("selector"))
                .next()
                .map(text)
                .unwrap_or_default();
            let ids = group
                .select(
                    &scraper::Selector::parse("[data-testid=\"thread-row\"]").expect("selector"),
                )
                .filter_map(|row| attr(row, "data-thread-id").map(str::to_owned))
                .collect();
            (label, ids)
        })
        .collect();
    let ids = |ids: &[&str]| ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        groups,
        vec![
            ("be:flaky".to_owned(), ids(&["t-flaky"])),
            ("be:main".to_owned(), ids(&["t-checkout", "t-flaky"])),
            ("fe:feat/checkout".to_owned(), ids(&["t-checkout"])),
            ("w-adopted".to_owned(), ids(&["t-idle"])),
            ("No workspace".to_owned(), ids(&["t-scratch"])),
        ]
    );
}

// spec: UI-022
#[test]
fn filters_by_key_and_text_narrow_the_list_and_offer_every_key() {
    let ui = UiState {
        filter: Filter {
            text: "flaky".to_owned(),
            key: Some((Dimension::Repository, "r-be".to_owned())),
            ..Filter::default()
        },
        ..UiState::default()
    };
    let page = Page::new(index(), ui);
    assert_eq!(rows(&page), pairs(&[("t-flaky", "running")]));
    let options: Vec<String> = page
        .all("[data-testid=\"filter-key\"] option")
        .into_iter()
        .map(text)
        .collect();
    assert_eq!(
        options,
        vec![
            "all",
            "Archived",
            "tag: release",
            "workspace: be:flaky",
            "workspace: be:main",
            "workspace: fe:feat/checkout",
            "workspace: w-adopted",
            "repository: be",
            "repository: fe",
            "pr: fe#412",
        ]
    );
}

// spec: UI-026
#[test]
fn the_archived_filter_lists_only_archived_threads() {
    let mut input = index();
    input.threads[1].archived_at = Some(5);
    assert_eq!(
        rows(&Page::new(input.clone(), UiState::default()))
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec!["t-checkout", "t-scratch", "t-idle"]
    );
    let ui = UiState {
        filter: Filter {
            archived: true,
            ..Filter::default()
        },
        ..UiState::default()
    };
    let page = Page::new(input, ui);
    assert_eq!(rows(&page), pairs(&[("t-flaky", "running")]));
    let selected: Vec<&str> = page
        .all("[data-testid=\"filter-key\"] option[selected]")
        .into_iter()
        .filter_map(|option| attr(option, "value"))
        .collect();
    assert_eq!(selected, vec!["archived"]);
}

// spec: UI-026
#[test]
fn the_header_offers_archive_or_unarchive_and_delete() {
    let page = Page::new(index(), selecting("t-idle"));
    assert_eq!(text(page.one("archive-thread")), "Archive");
    assert_eq!(text(page.one("delete-thread")), "Delete…");
    let mut input = index();
    input.threads[3].archived_at = Some(5);
    let archived = Page::new(input, selecting("t-idle"));
    assert_eq!(text(archived.one("archive-thread")), "Unarchive");
    archived.one("delete-thread");
}

// spec: UI-026, UI-092
#[test]
fn deleting_asks_for_confirmation_in_a_focus_trapping_dialog() {
    let ui = UiState {
        delete_thread: Some("t-idle".to_owned()),
        ..selecting("t-idle")
    };
    let page = Page::new(index(), ui);
    let dialog = page.one("delete-thread-dialog");
    assert_eq!(
        (attr(dialog, "role"), attr(dialog, "aria-modal")),
        (Some("dialog"), Some("true"))
    );
    assert_eq!(attr(page.one("shell"), "inert"), Some("true"));
    assert_eq!(
        page.testid("focus-start").len() + page.testid("focus-end").len(),
        2
    );
    assert!(text(dialog).contains("idle-notes"), "{}", text(dialog));
    assert_eq!(
        text(page.one("delete-thread-message")),
        "The thread's history (transcript, session and log) is removed for good. Its workspaces are kept."
    );
    page.one("confirm-delete-thread");
    page.one("cancel-delete-thread");
}

// spec: UI-021
#[test]
fn the_attention_shortcut_is_offered_with_the_number_of_waiting_threads() {
    let page = Page::new(index(), UiState::default());
    let next = page.one("next-attention");
    assert_eq!(text(next), "Next waiting (1)");
    assert_eq!(
        attr(next, "title"),
        Some("Next thread needing attention (Alt+A)")
    );
    assert_ne!(attr(next, "disabled"), Some("true"));
    let mut quiet = index();
    quiet.threads[0].needs_attention = false;
    assert_eq!(
        attr(
            Page::new(quiet, UiState::default()).one("next-attention"),
            "disabled"
        ),
        Some("true")
    );
}

// spec: UI-023, UI-092
#[test]
fn the_new_thread_dialog_offers_every_workspace_source_and_traps_focus() {
    let form = NewThreadForm {
        title: "checkout".to_owned(),
        workspaces: vec![
            WorkspaceChoice::Existing {
                workspace_id: "w-fe".to_owned(),
            },
            WorkspaceChoice::NewWorktree {
                repo: "r-be".to_owned(),
                branch: String::new(),
                base_ref: String::new(),
            },
            WorkspaceChoice::Adopt {
                path: "/tmp/x".to_owned(),
            },
        ],
        ..NewThreadForm::default()
    };
    let page = Page::new(
        index(),
        UiState {
            new_thread: Some(form),
            ..UiState::default()
        },
    );
    let dialog = page.one("new-thread-dialog");
    assert_eq!(
        (attr(dialog, "role"), attr(dialog, "aria-modal")),
        (Some("dialog"), Some("true"))
    );
    assert_eq!(attr(page.one("shell"), "inert"), Some("true"));
    assert_eq!(
        page.testid("focus-start").len() + page.testid("focus-end").len(),
        2
    );
    let kinds: Vec<&str> = page
        .testid("workspace-choice")
        .into_iter()
        .filter_map(|line| attr(line, "data-kind"))
        .collect();
    assert_eq!(kinds, vec!["existing", "worktree", "adopt"]);
    assert!(text(page.testid("workspace-choice")[0]).starts_with("1 (cwd)"));
    for add in ["add-existing", "add-worktree", "add-adopt"] {
        assert_ne!(attr(page.one(add), "disabled"), Some("true"), "{add}");
    }
    assert_eq!(attr(page.one("add-pr"), "disabled"), Some("true"));
    let closed = Page::new(index(), UiState::default());
    assert_eq!(closed.testid("new-thread-dialog").len(), 0);
    // `inert` is a boolean attribute: any value, even "false", would disable the shell.
    assert_eq!(attr(closed.one("shell"), "inert"), None);
}

// spec: UI-024
#[test]
fn panels_render_where_the_registry_places_them() {
    let side = Page::new(index(), UiState::default());
    assert_eq!(
        side.all("[data-testid=\"side-panels\"] [data-panel=\"workspaces\"]")
            .len(),
        1
    );
    assert_eq!(side.testid("bottom-panels").len(), 0);
    for (placement, region) in [
        (Placement::Bottom, "bottom-panels"),
        (Placement::Floating, "floating-panel"),
        (Placement::Main, "main"),
    ] {
        let mut ui = UiState::default();
        ui.panels.place("workspaces", placement);
        ui.main_tab = Some("workspaces".to_owned());
        let page = Page::new(index(), ui);
        let panel = page.all(&format!(
            "[data-testid=\"{region}\"] [data-kind=\"workspaces\"]"
        ));
        assert_eq!(panel.len(), 1, "{placement:?}");
        assert_eq!(page.testid("side-panels").len(), 0, "{placement:?}");
    }
}

// spec: UI-025
#[test]
fn the_workspaces_panel_lists_threads_git_status_pr_and_warns_on_shared_workspaces() {
    let mut input = index();
    input.git.insert(
        "w-be".to_owned(),
        GitStatus {
            branch: Some("main".to_owned()),
            base_ref: None,
            ahead: 0,
            behind: 0,
            files: vec![FileChange {
                path: "a.rs".to_owned(),
                old_path: None,
                status: FileStatus::Modified,
                additions: 2,
                deletions: 1,
            }],
        },
    );
    let page = Page::new(input, UiState::default());
    let shared: Vec<&str> = page
        .testid("workspace-row")
        .into_iter()
        .filter(|row| attr(*row, "data-shared") == Some("true"))
        .filter_map(|row| attr(row, "data-workspace-id"))
        .collect();
    assert_eq!(shared, vec!["w-be"]);
    assert_eq!(text(page.one("shared-warning")), "⚠ shared by 2 threads");
    assert_eq!(
        attr(page.one("workspace-pr"), "href"),
        Some("https://example.test/fe/pull/412")
    );
    assert_eq!(text(page.one("git-status")), "main · 1 file +2 −1");
    assert_eq!(
        page.testid("remove-workspace").len(),
        3,
        "adopted directories are not removable"
    );
    for form in ["create-worktree", "adopt-workspace", "add-repository"] {
        page.one(form);
    }
}

// spec: UI-025
#[test]
fn the_workspaces_panel_lists_registered_repositories_with_their_worktrees() {
    let page = Page::new(index(), UiState::default());
    let rows: Vec<(String, String)> = page
        .testid("repository-row")
        .into_iter()
        .map(|row| {
            let parts: Vec<String> = row
                .select(&scraper::Selector::parse("strong, span, code, button").expect("selector"))
                .map(text)
                .collect();
            (
                attr(row, "data-repository-id")
                    .unwrap_or_default()
                    .to_owned(),
                parts.join(" | "),
            )
        })
        .collect();
    assert_eq!(
        rows,
        pairs(&[
            (
                "r-fe",
                "fe | 1 worktree | /src/fe | base main | New worktree | Remove…"
            ),
            (
                "r-be",
                "be | 2 worktrees | /src/be | base main | New worktree | Remove…"
            ),
        ])
    );
    assert_eq!(page.testid("no-repositories").len(), 0);
    page.one("add-repository");

    let empty = ViewInput {
        repositories: Vec::new(),
        ..index()
    };
    let page = Page::new(empty, UiState::default());
    assert_eq!(page.testid("repository-row").len(), 0);
    assert_eq!(
        text(page.one("no-repositories")),
        "No repository yet: add one with its path."
    );
    page.one("add-repository");
}

// spec: UI-003
#[test]
fn the_connection_banner_shows_while_disconnected_without_blocking_the_shell() {
    let input = ViewInput {
        connection: ConnectionView::Reconnecting {
            attempt: 2,
            retry_in_ms: 2000,
        },
        ..index()
    };
    let page = Page::new(input, UiState::default());
    let banner = page.one("connection-banner");
    assert_eq!(attr(banner, "data-tone"), Some("warning"));
    assert_eq!(
        text(banner),
        "Connection lost. Reconnecting in 2 s (attempt 2)…"
    );
    assert_eq!(attr(page.one("shell"), "inert"), None);
    assert_eq!(page.testid("thread-row").len(), 4);
    assert_eq!(
        Page::new(index(), UiState::default())
            .testid("connection-banner")
            .len(),
        0
    );
}

// spec: UI-091
#[test]
fn the_theme_follows_the_system_unless_overridden() {
    let system = Page::new(index(), UiState::default());
    assert_eq!(attr(system.one("app"), "data-theme"), None);
    let dark = Page::new(
        index(),
        UiState {
            theme: Theme::Dark,
            ..UiState::default()
        },
    );
    assert_eq!(attr(dark.one("app"), "data-theme"), Some("dark"));
    let css = include_str!("../../assets/omfp.css");
    for rule in [
        "@media (prefers-color-scheme: dark)",
        ".app:not([data-theme=\"light\"])",
        ".app[data-theme=\"dark\"]",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(
        css.contains("--bg:") && css.contains("var(--bg)"),
        "colors are custom properties"
    );
}

// spec: UI-092
#[test]
fn status_changes_are_announced_through_a_live_region() {
    let page = Page::new(index(), selecting("t-checkout"));
    let region = page.one("live-region");
    assert_eq!(
        (attr(region, "role"), attr(region, "aria-live")),
        (Some("status"), Some("polite"))
    );
    assert_eq!(
        text(region),
        "checkout-v2: awaiting input. 1 thread awaits input"
    );
}

// spec: UI-092
#[test]
fn every_control_is_a_native_focusable_element_with_a_name() {
    let mut ui = selecting("t-checkout");
    ui.new_thread = Some(NewThreadForm::default());
    let html = render(
        super::with_open(
            index(),
            "t-checkout",
            crate::test_support::fixture("bash_tool_call"),
        ),
        ui,
    );
    let page = scraper::Html::parse_fragment(&html);
    let clickable = scraper::Selector::parse("[role=button], a:not([href])").expect("selector");
    assert_eq!(
        page.select(&clickable).count(),
        0,
        "actions are buttons, links have targets"
    );
    let controls = scraper::Selector::parse("button, select, input, textarea").expect("selector");
    for control in page.select(&controls) {
        let named = !text(control).is_empty()
            || attr(control, "aria-label").is_some()
            || attr(control, "title").is_some()
            || control
                .parent()
                .and_then(scraper::ElementRef::wrap)
                .is_some_and(|parent| parent.value().name() == "label");
        assert!(named, "unnamed control: {}", control.html());
    }
}

// spec: TST-031, TST-032
#[test]
fn shell_snapshot() {
    let mut input = index();
    input.threads[3].updated_at = 9;
    let ui = UiState {
        grouping: Grouping::By(Dimension::Repository),
        seen: BTreeMap::from([("t-idle".to_owned(), 1)]),
        ..UiState::default()
    };
    insta::assert_snapshot!(normalize(&render(input, ui)));
}
