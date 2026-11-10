//! The new-thread dialog (UI-023): title, tags, first message, and any combination of workspaces. It traps focus
//! (UI-092): the shell behind it is inert and focus sentinels at both ends wrap Tab around.

use std::rc::Rc;

use dioxus::prelude::*;

use super::Ctx;
use crate::vm::new_thread::{NewThreadForm, WorkspaceChoice};
use crate::vm::threads::workspace_label;

/// Updates the open form, if any.
fn edit(mut ui: Signal<crate::UiState>, change: impl FnOnce(&mut NewThreadForm)) {
    if let Some(form) = ui.write().new_thread.as_mut() {
        change(form);
    }
}

pub(super) fn focus(target: Signal<Option<Rc<MountedData>>>) {
    if let Some(target) = target.peek().clone() {
        spawn(async move {
            // Focus is best effort: the element may be gone.
            let _ = target.set_focus(true).await;
        });
    }
}

#[component]
pub(crate) fn NewThreadDialog() -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let Some(form) = ui.read().new_thread.clone() else {
        return rsx! {};
    };
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let first: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let last: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let input = ctx.input.read();
    let workspaces: Vec<(String, String)> = input
        .workspaces
        .iter()
        .map(|workspace| {
            (
                workspace.id.clone(),
                workspace_label(workspace, &input.repositories),
            )
        })
        .collect();
    let repositories: Vec<(String, String)> = input
        .repositories
        .iter()
        .map(|repository| (repository.id.clone(), repository.name.clone()))
        .collect();
    drop(input);
    let mut close = move || ui.write().new_thread = None;

    rsx! {
        div { class: "dialog-backdrop",
            div {
                class: "dialog",
                role: "dialog",
                "aria-modal": "true",
                "aria-labelledby": "new-thread-heading",
                "data-testid": "new-thread-dialog",
                onkeydown: move |event: KeyboardEvent| {
                    if event.key() == Key::Escape {
                        event.prevent_default();
                        close();
                    }
                },
                div { tabindex: "0", "data-testid": "focus-start", onfocus: move |_| focus(last) }
                h2 { id: "new-thread-heading", "New thread" }
                form {
                    onsubmit: move |event: FormEvent| {
                        event.prevent_default();
                        let Some(form) = ui.peek().new_thread.clone() else { return };
                        match form.to_intent() {
                            Ok(intent) => {
                                error.set(None);
                                ctx.send(intent);
                            }
                            Err(message) => error.set(Some(message)),
                        }
                    },
                    TitleAndTags { title: form.title.clone(), tags: form.tags.clone(), first }
                    WorkspaceChoices { choices: form.workspaces.clone(), workspaces, repositories }
                    label {
                        "First message (optional)"
                        textarea {
                            "data-testid": "new-thread-prompt",
                            rows: "3",
                            value: "{form.prompt}",
                            oninput: move |event| edit(ui, |form| form.prompt = event.value()),
                        }
                    }
                    if let Some(message) = error() {
                        p { class: "form-error", role: "alert", "data-testid": "new-thread-error", "{message}" }
                    }
                    div { class: "dialog-actions",
                        button { r#type: "submit", class: "primary", "data-testid": "create-thread", "Create" }
                        button {
                            r#type: "button",
                            "data-testid": "cancel-thread",
                            onmounted: move |event| {
                                let mut last = last;
                                last.set(Some(event.data()));
                            },
                            onclick: move |_| close(),
                            "Cancel"
                        }
                    }
                }
                div { tabindex: "0", "data-testid": "focus-end", onfocus: move |_| focus(first) }
            }
        }
    }
}

/// The title (focused when the dialog opens) and tags fields.
#[component]
fn TitleAndTags(title: String, tags: String, first: Signal<Option<Rc<MountedData>>>) -> Element {
    let ui = Ctx::get().ui;
    rsx! {
        label {
            "Title"
            input {
                "data-testid": "new-thread-title",
                value: "{title}",
                placeholder: "New thread",
                onmounted: move |event| {
                    let mut first = first;
                    first.set(Some(event.data()));
                    focus(first);
                },
                oninput: move |event| edit(ui, |form| form.title = event.value()),
            }
        }
        label {
            "Tags"
            input {
                "data-testid": "new-thread-tags",
                value: "{tags}",
                placeholder: "comma-separated",
                oninput: move |event| edit(ui, |form| form.tags = event.value()),
            }
        }
    }
}

/// The workspace lines and the buttons adding one of each source (UI-023).
#[component]
fn WorkspaceChoices(
    choices: Vec<WorkspaceChoice>,
    workspaces: Vec<(String, String)>,
    repositories: Vec<(String, String)>,
) -> Element {
    let ui = Ctx::get().ui;
    let none_to_pick = workspaces.is_empty();
    rsx! {
        fieldset { class: "workspace-choices",
            legend { "Workspaces (optional; the first is the working directory)" }
            for (index, choice) in choices.into_iter().enumerate() {
                WorkspaceLine { key: "{index}", index, choice, workspaces: workspaces.clone(), repositories: repositories.clone() }
            }
            div { class: "choice-buttons",
                button {
                    r#type: "button",
                    "data-testid": "add-existing",
                    disabled: none_to_pick,
                    onclick: move |_| edit(ui, |form| form.workspaces.push(WorkspaceChoice::Existing { workspace_id: String::new() })),
                    "+ Existing workspace"
                }
                button {
                    r#type: "button",
                    "data-testid": "add-worktree",
                    onclick: move |_| edit(ui, |form| form.workspaces.push(WorkspaceChoice::NewWorktree {
                        repo: String::new(),
                        branch: String::new(),
                        base_ref: String::new(),
                    })),
                    "+ New worktree"
                }
                button {
                    r#type: "button",
                    "data-testid": "add-pr",
                    disabled: true,
                    title: "Workspaces from pull requests are not supported by the daemon yet",
                    "+ From PR"
                }
                button {
                    r#type: "button",
                    "data-testid": "add-adopt",
                    onclick: move |_| edit(ui, |form| form.workspaces.push(WorkspaceChoice::Adopt { path: String::new() })),
                    "+ Adopt directory"
                }
            }
        }
    }
}

/// Changes workspace line `index` of the open form.
fn set_choice(ui: Signal<crate::UiState>, index: usize, change: impl FnOnce(&mut WorkspaceChoice)) {
    edit(ui, |form| {
        if let Some(choice) = form.workspaces.get_mut(index) {
            change(choice);
        }
    });
}

/// One workspace of the form.
#[component]
fn WorkspaceLine(
    index: usize,
    choice: WorkspaceChoice,
    workspaces: Vec<(String, String)>,
    repositories: Vec<(String, String)>,
) -> Element {
    let ui = Ctx::get().ui;
    let line = index + 1;
    let kind = choice.token();
    rsx! {
        div { class: "workspace-choice", "data-testid": "workspace-choice", "data-kind": kind,
            span { class: "choice-index", if index == 0 { "{line} (cwd)" } else { "{line}" } }
            match choice {
                WorkspaceChoice::Existing { workspace_id } => rsx! {
                    select {
                        "aria-label": "Workspace {line}",
                        value: "{workspace_id}",
                        onchange: move |event| {
                            let value = event.value();
                            set_choice(ui, index, move |choice| *choice = WorkspaceChoice::Existing { workspace_id: value });
                        },
                        option { value: "", "Workspace…" }
                        for (id, label) in workspaces {
                            option { key: "{id}", value: "{id}", selected: *id == workspace_id, "{label}" }
                        }
                    }
                },
                WorkspaceChoice::NewWorktree { repo, branch, base_ref } => rsx! {
                    WorktreeInputs { index, repo, branch, base_ref, repositories }
                },
                WorkspaceChoice::FromPr { pr, .. } => rsx! {
                    span { "PR {pr}" }
                },
                WorkspaceChoice::Adopt { path } => rsx! {
                    input {
                        "aria-label": "Directory of workspace {line}",
                        placeholder: "/path/to/directory",
                        value: "{path}",
                        oninput: move |event| {
                            let value = event.value();
                            set_choice(ui, index, move |choice| *choice = WorkspaceChoice::Adopt { path: value });
                        },
                    }
                },
            }
            button {
                r#type: "button",
                "aria-label": "Remove workspace {line}",
                onclick: move |_| edit(ui, |form| {
                    if index < form.workspaces.len() {
                        form.workspaces.remove(index);
                    }
                }),
                "×"
            }
        }
    }
}

/// Repository (registered id or path), branch and base of a new worktree line.
#[component]
fn WorktreeInputs(
    index: usize,
    repo: String,
    branch: String,
    base_ref: String,
    repositories: Vec<(String, String)>,
) -> Element {
    let ui = Ctx::get().ui;
    let line = index + 1;
    rsx! {
        input {
            "data-testid": "worktree-repo",
            "aria-label": "Repository of workspace {line} (registered id or path)",
            placeholder: "repository or /path",
            list: "new-thread-repositories",
            value: "{repo}",
            oninput: move |event| {
                let value = event.value();
                set_choice(ui, index, move |choice| {
                    if let WorkspaceChoice::NewWorktree { repo, .. } = choice {
                        *repo = value;
                    }
                });
            },
        }
        datalist { id: "new-thread-repositories",
            for (id, name) in repositories {
                option { key: "{id}", value: "{id}", "{name}" }
            }
        }
        input {
            "data-testid": "worktree-branch",
            "aria-label": "Branch of workspace {line}",
            placeholder: "branch (optional)",
            value: "{branch}",
            oninput: move |event| {
                let value = event.value();
                set_choice(ui, index, move |choice| {
                    if let WorkspaceChoice::NewWorktree { branch, .. } = choice {
                        *branch = value;
                    }
                });
            },
        }
        input {
            "data-testid": "worktree-base",
            "aria-label": "Base ref of workspace {line}",
            placeholder: "base (optional)",
            value: "{base_ref}",
            oninput: move |event| {
                let value = event.value();
                set_choice(ui, index, move |choice| {
                    if let WorkspaceChoice::NewWorktree { base_ref, .. } = choice {
                        *base_ref = value;
                    }
                });
            },
        }
    }
}
