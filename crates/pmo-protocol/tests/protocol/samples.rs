//! Example values shared by the golden tests. Ids are fixed ULID-shaped strings so snapshots stay stable.

use pmo_core::ProcessState;
use pmo_protocol::{
    CreatedBy, FileChange, FileStatus, GitStatus, Provenance, Repository, ThreadSummary, Workspace,
    WorkspaceKind,
};

pub const REPO_ID: &str = "01JD8X3Q4T9V2W6Y5Z7A1B3C6F";
pub const WORKSPACE_ID: &str = "01JD8X3Q4T9V2W6Y5Z7A1B3C5E";
pub const OTHER_WORKSPACE_ID: &str = "01JD8X3Q4T9V2W6Y5Z7A1B3C7G";
pub const THREAD_ID: &str = "01JD8X3Q4T9V2W6Y5Z7A1B3C4D";
pub const TERMINAL_ID: &str = "01JD8X3Q4T9V2W6Y5Z7A1B3C8H";
/// 2026-11-10T00:00:00Z in Unix milliseconds.
pub const CREATED_AT: i64 = 1_794_268_800_000;

pub fn user_provenance() -> Provenance {
    Provenance {
        created_by: CreatedBy::User,
        client: Some("pmo-ui/web".to_owned()),
    }
}

pub fn repository() -> Repository {
    Repository {
        id: REPO_ID.to_owned(),
        name: "shop-frontend".to_owned(),
        path: "/home/dev/src/shop-frontend".to_owned(),
        default_base_ref: "origin/main".to_owned(),
        remote_url: Some("git@github.com:acme/shop-frontend.git".to_owned()),
        created_at: CREATED_AT,
    }
}

pub fn managed_workspace() -> Workspace {
    Workspace {
        id: WORKSPACE_ID.to_owned(),
        path: "/home/dev/.local/share/pmo/worktrees/shop-frontend/checkout-v2".to_owned(),
        kind: WorkspaceKind::ManagedWorktree,
        repository_id: Some(REPO_ID.to_owned()),
        branch: Some("pmo/checkout-v2-k3p9".to_owned()),
        base_ref: Some("origin/main".to_owned()),
        pr_url: None,
        pr_number: None,
        provenance: user_provenance(),
        created_at: CREATED_AT,
    }
}

pub fn pr_workspace() -> Workspace {
    Workspace {
        branch: Some("feature/checkout-v2".to_owned()),
        pr_url: Some("https://github.com/acme/shop-frontend/pull/412".to_owned()),
        pr_number: Some(412),
        provenance: Provenance {
            created_by: CreatedBy::Thread {
                thread_id: THREAD_ID.to_owned(),
            },
            client: Some("pmo-agent".to_owned()),
        },
        ..managed_workspace()
    }
}

pub fn adopted_workspace() -> Workspace {
    Workspace {
        id: OTHER_WORKSPACE_ID.to_owned(),
        path: "/home/dev/notes".to_owned(),
        kind: WorkspaceKind::Adopted,
        repository_id: None,
        branch: None,
        base_ref: None,
        pr_url: None,
        pr_number: None,
        provenance: user_provenance(),
        created_at: CREATED_AT,
    }
}

pub fn thread() -> ThreadSummary {
    ThreadSummary {
        id: THREAD_ID.to_owned(),
        title: "Checkout v2".to_owned(),
        tags: vec!["checkout-v2".to_owned()],
        workspace_ids: vec![WORKSPACE_ID.to_owned(), OTHER_WORKSPACE_ID.to_owned()],
        process: ProcessState::Stopped,
        settled: true,
        needs_attention: false,
        provenance: user_provenance(),
        created_at: CREATED_AT,
        updated_at: CREATED_AT + 60_000,
        archived_at: None,
    }
}

pub fn git_status() -> GitStatus {
    GitStatus {
        branch: Some("pmo/checkout-v2-k3p9".to_owned()),
        base_ref: Some("origin/main".to_owned()),
        ahead: 2,
        behind: 1,
        files: vec![
            FileChange {
                path: "src/checkout.rs".to_owned(),
                old_path: None,
                status: FileStatus::Modified,
                additions: 40,
                deletions: 3,
            },
            FileChange {
                path: "src/cart/total.rs".to_owned(),
                old_path: Some("src/total.rs".to_owned()),
                status: FileStatus::Renamed,
                additions: 1,
                deletions: 1,
            },
            FileChange {
                path: "notes.txt".to_owned(),
                old_path: None,
                status: FileStatus::Untracked,
                additions: 5,
                deletions: 0,
            },
        ],
    }
}
