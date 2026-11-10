//! Panel regions fed by the registry (UI-024) and the Workspaces panel (UI-025).

use dioxus::prelude::*;

use super::Ctx;
use crate::intent::{Intent, WorkspaceSource};
use crate::panels::{PanelContent, Placement};
use crate::vm::workspaces::{RepositoryRow, WorkspaceRow, repository_rows, workspace_rows};

fn panel_ids(ctx: Ctx, placement: Placement) -> Vec<String> {
    ctx.ui
        .read()
        .panels
        .in_placement(placement)
        .into_iter()
        .map(|panel| panel.id.clone())
        .collect()
}

/// The right-hand column: one tab per side panel.
#[component]
pub(crate) fn SidePanels() -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let ids = panel_ids(ctx, Placement::Side);
    if ids.is_empty() {
        return rsx! {};
    }
    let state = ui.read();
    let active = state.active_side_panel();
    let tabs: Vec<(String, String)> = ids
        .iter()
        .filter_map(|id| {
            state
                .panels
                .get(id)
                .map(|panel| (id.clone(), panel.title.clone()))
        })
        .collect();
    drop(state);
    rsx! {
        aside { class: "side", "data-testid": "side-panels", "aria-label": "Panels",
            if tabs.len() > 1 {
                div { class: "tabs", role: "tablist", "aria-label": "Side panels",
                    for (id, title) in tabs {
                        button {
                            key: "{id}",
                            r#type: "button",
                            role: "tab",
                            "aria-selected": active.as_deref() == Some(id.as_str()),
                            onclick: {
                                let id = id.clone();
                                move |_| ui.write().side_tab = Some(id.clone())
                            },
                            "{title}"
                        }
                    }
                }
            }
            if let Some(id) = active {
                PanelFrame { panel_id: id }
            }
        }
    }
}

/// Panels below the thread, stacked.
#[component]
pub(crate) fn BottomPanels() -> Element {
    let ids = panel_ids(Ctx::get(), Placement::Bottom);
    if ids.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "bottom", "data-testid": "bottom-panels",
            for id in ids {
                PanelFrame { key: "{id}", panel_id: id }
            }
        }
    }
}

/// Panels over the shell.
#[component]
pub(crate) fn FloatingPanels() -> Element {
    let ids = panel_ids(Ctx::get(), Placement::Floating);
    rsx! {
        for id in ids {
            div { key: "{id}", class: "floating", "data-testid": "floating-panel",
                PanelFrame { panel_id: id }
            }
        }
    }
}

/// A panel with its title and placement selector.
#[component]
pub(crate) fn PanelFrame(panel_id: String) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let Some(panel) = ui.read().panels.get(&panel_id).cloned() else {
        return rsx! {};
    };
    let id = panel_id.clone();
    rsx! {
        section {
            class: "panel",
            "data-testid": "panel",
            "data-panel": "{panel.id}",
            "data-kind": panel.content.kind(),
            "data-placement": panel.placement.token(),
            "aria-label": "{panel.title}",
            div { class: "panel-header",
                h2 { "{panel.title}" }
                select {
                    "data-testid": "panel-placement",
                    "aria-label": "Move the {panel.title} panel",
                    value: panel.placement.token(),
                    onchange: move |event| {
                        if let Some(placement) = Placement::from_token(&event.value()) {
                            ui.write().panels.place(&id, placement);
                        }
                    },
                    for placement in Placement::ALL {
                        option { value: placement.token(), selected: placement == panel.placement, "{placement.label()}" }
                    }
                }
            }
            PanelBody { panel_id }
        }
    }
}

/// The content of a panel, by kind.
#[component]
fn PanelBody(panel_id: String) -> Element {
    let content = Ctx::get()
        .ui
        .read()
        .panels
        .get(&panel_id)
        .map(|panel| panel.content.clone());
    match content {
        Some(PanelContent::Workspaces) => rsx! {
            WorkspacesPanel {}
        },
        None => rsx! {},
    }
}

/// The trimmed value of a form field, which is cleared.
fn take(mut field: Signal<String>) -> String {
    let value = field.peek().trim().to_owned();
    field.set(String::new());
    value
}

/// All workspaces with their threads, git status and PR link; create, adopt, remove; the registered repositories
/// (UI-025).
#[component]
fn WorkspacesPanel() -> Element {
    let ctx = Ctx::get();
    let input = ctx.input.read();
    let rows = workspace_rows(
        &input.workspaces,
        &input.repositories,
        &input.threads,
        &input.git,
    );
    let registered = repository_rows(&input.repositories, &input.workspaces);
    let repositories: Vec<(String, String)> = input
        .repositories
        .iter()
        .map(|repository| (repository.id.clone(), repository.name.clone()))
        .collect();
    drop(input);
    let confirm_remove: Signal<Option<String>> = use_signal(|| None);
    let worktree_repo = use_signal(String::new);
    rsx! {
        div { class: "workspaces", "data-testid": "workspaces-panel",
            ul { class: "workspace-list",
                for row in rows {
                    WorkspaceRowView { key: "{row.id}", row, confirm_remove }
                }
            }
            WorktreeForm { repositories, worktree_repo }
            AdoptForm {}
            Repositories { rows: registered, worktree_repo }
        }
    }
}

/// The registered repositories with their worktree count: start a worktree of one, or remove it (its workspaces
/// stay); the form registering one comes last.
#[component]
fn Repositories(rows: Vec<RepositoryRow>, worktree_repo: Signal<String>) -> Element {
    let confirm_remove: Signal<Option<String>> = use_signal(|| None);
    rsx! {
        section { class: "repositories", "data-testid": "repositories", "aria-labelledby": "repositories-heading",
            h3 { id: "repositories-heading", "Repositories" }
            if rows.is_empty() {
                p { class: "muted", "data-testid": "no-repositories", "No repository yet: add one with its path." }
            } else {
                ul { class: "repository-list",
                    for row in rows {
                        RepositoryRowView { key: "{row.id}", row, worktree_repo, confirm_remove }
                    }
                }
            }
            RepositoryForm {}
        }
    }
}

/// One repository: name, path, default base ref, worktree count, "New worktree" and removal.
#[component]
fn RepositoryRowView(
    row: RepositoryRow,
    worktree_repo: Signal<String>,
    confirm_remove: Signal<Option<String>>,
) -> Element {
    let ctx = Ctx::get();
    let mut worktree_repo = worktree_repo;
    let mut confirm_remove = confirm_remove;
    let noun = if row.worktrees == 1 {
        "worktree"
    } else {
        "worktrees"
    };
    let confirming = confirm_remove().as_deref() == Some(row.id.as_str());
    let (preselect, confirm, remove) = (row.id.clone(), row.id.clone(), row.id.clone());
    rsx! {
        li { class: "repository-row", "data-testid": "repository-row", "data-repository-id": "{row.id}",
            div { class: "workspace-title",
                strong { "{row.name}" }
                span { class: "muted", "data-testid": "repository-worktrees", " {row.worktrees} {noun}" }
            }
            code { class: "workspace-path", "{row.path}" }
            span { class: "muted", "data-testid": "repository-base", "base {row.default_base_ref}" }
            div { class: "choice-buttons",
                button {
                    r#type: "button",
                    "data-testid": "repository-new-worktree",
                    onclick: move |_| worktree_repo.set(preselect.clone()),
                    "New worktree"
                }
                if confirming {
                    span { "data-testid": "remove-repository-note", "Its workspaces stay." }
                    button {
                        r#type: "button",
                        class: "danger",
                        "data-testid": "confirm-remove-repository",
                        onclick: move |_| {
                            confirm_remove.set(None);
                            if *worktree_repo.peek() == remove {
                                worktree_repo.set(String::new());
                            }
                            ctx.send(Intent::RemoveRepository { repository_id: remove.clone() });
                        },
                        "Remove the repository"
                    }
                    button { r#type: "button", onclick: move |_| confirm_remove.set(None), "Keep" }
                } else {
                    button {
                        r#type: "button",
                        "data-testid": "remove-repository",
                        onclick: move |_| confirm_remove.set(Some(confirm.clone())),
                        "Remove…"
                    }
                }
            }
        }
    }
}

/// One workspace: label, kind, shared warning, path, PR link, git status, threads, removal.
#[component]
fn WorkspaceRowView(row: WorkspaceRow, confirm_remove: Signal<Option<String>>) -> Element {
    let ctx = Ctx::get();
    let refresh_id = row.id.clone();
    rsx! {
        li {
            class: "workspace-row",
            "data-testid": "workspace-row",
            "data-workspace-id": "{row.id}",
            "data-shared": row.shared,
            div { class: "workspace-title",
                strong { "{row.label}" }
                span { class: "workspace-kind", if row.managed { " worktree" } else { " adopted" } }
                if row.shared {
                    span {
                        class: "warning",
                        "data-testid": "shared-warning",
                        title: "Several threads work in this directory: their changes mix.",
                        "⚠ shared by {row.threads.len()} threads"
                    }
                }
            }
            code { class: "workspace-path", "{row.path}" }
            if let (Some(number), Some(url)) = (row.pr_number, row.pr_url.as_ref()) {
                a { "data-testid": "workspace-pr", href: "{url}", target: "_blank", rel: "noopener noreferrer", "PR #{number}" }
            }
            div { class: "workspace-git",
                match row.git.as_ref() {
                    Some(git) => rsx! {
                        span { "data-testid": "git-status", "{git}" }
                    },
                    None => rsx! {
                        span { class: "muted", "git status unknown" }
                    },
                }
                button {
                    r#type: "button",
                    "data-testid": "refresh-workspace",
                    onclick: move |_| ctx.send(Intent::RefreshWorkspace { workspace_id: refresh_id.clone() }),
                    "Refresh"
                }
            }
            if !row.threads.is_empty() {
                ul { class: "workspace-threads", "aria-label": "Threads",
                    for (thread_id, title) in row.threads.iter().cloned() {
                        li { key: "{thread_id}",
                            button { r#type: "button", class: "link", onclick: move |_| ctx.select_thread(&thread_id), "{title}" }
                        }
                    }
                }
            }
            if row.managed {
                RemoveWorkspace { workspace_id: row.id.clone(), confirm_remove }
            }
        }
    }
}

/// Removing a managed worktree takes a confirmation.
#[component]
fn RemoveWorkspace(workspace_id: String, confirm_remove: Signal<Option<String>>) -> Element {
    let ctx = Ctx::get();
    let mut confirm_remove = confirm_remove;
    if confirm_remove().as_deref() != Some(workspace_id.as_str()) {
        return rsx! {
            button {
                r#type: "button",
                "data-testid": "remove-workspace",
                onclick: move |_| confirm_remove.set(Some(workspace_id.clone())),
                "Remove…"
            }
        };
    }
    rsx! {
        button {
            r#type: "button",
            class: "danger",
            "data-testid": "confirm-remove-workspace",
            onclick: move |_| {
                confirm_remove.set(None);
                ctx.send(Intent::RemoveWorkspace {
                    workspace_id: workspace_id.clone(),
                    force: false,
                    delete_branch: false,
                });
            },
            "Remove the worktree"
        }
        button { r#type: "button", onclick: move |_| confirm_remove.set(None), "Keep" }
    }
}

/// A new managed worktree of a registered repository; `worktree_repo` is the chosen one, which a repository row can
/// preselect.
#[component]
fn WorktreeForm(repositories: Vec<(String, String)>, worktree_repo: Signal<String>) -> Element {
    let ctx = Ctx::get();
    let mut worktree_repo = worktree_repo;
    let mut worktree_branch = use_signal(String::new);
    rsx! {
        form {
            class: "inline-form",
            "data-testid": "create-worktree",
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                let repo = take(worktree_repo);
                let branch = take(worktree_branch);
                if !repo.is_empty() {
                    ctx.send(Intent::CreateWorkspace(WorkspaceSource::NewWorktree {
                        repo,
                        branch: (!branch.is_empty()).then_some(branch),
                        base_ref: None,
                    }));
                }
            },
            select {
                "aria-label": "Repository",
                value: "{worktree_repo}",
                onchange: move |event| worktree_repo.set(event.value()),
                option { value: "", "Repository…" }
                for (id, name) in repositories {
                    option { key: "{id}", value: "{id}", selected: *id == *worktree_repo.read(), "{name}" }
                }
            }
            input {
                "aria-label": "Branch (optional)",
                placeholder: "branch (optional)",
                value: "{worktree_branch}",
                oninput: move |event| worktree_branch.set(event.value()),
            }
            button { r#type: "submit", "New worktree" }
        }
    }
}

/// Records an existing directory as a workspace.
#[component]
fn AdoptForm() -> Element {
    let ctx = Ctx::get();
    let mut adopt_path = use_signal(String::new);
    rsx! {
        form {
            class: "inline-form",
            "data-testid": "adopt-workspace",
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                let path = take(adopt_path);
                if !path.is_empty() {
                    ctx.send(Intent::CreateWorkspace(WorkspaceSource::Adopt { path }));
                }
            },
            input {
                "aria-label": "Directory to adopt",
                placeholder: "/path/to/directory",
                value: "{adopt_path}",
                oninput: move |event| adopt_path.set(event.value()),
            }
            button { r#type: "submit", "Adopt" }
        }
    }
}

/// Registers a git repository.
#[component]
fn RepositoryForm() -> Element {
    let ctx = Ctx::get();
    let mut repo_path = use_signal(String::new);
    rsx! {
        form {
            class: "inline-form",
            "data-testid": "add-repository",
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                let path = take(repo_path);
                if !path.is_empty() {
                    ctx.send(Intent::AddRepository { path });
                }
            },
            input {
                "aria-label": "Repository path",
                placeholder: "/path/to/repository",
                value: "{repo_path}",
                oninput: move |event| repo_path.set(event.value()),
            }
            button { r#type: "submit", "Add repository" }
        }
    }
}
