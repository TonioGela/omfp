//! Example values shared by the golden tests. Ids are fixed ULID-shaped strings so snapshots stay stable.

use omfp_core::ProcessState;
use omfp_protocol::{
    CreatedBy, FileChange, FileStatus, GitStatus, Provenance, RepoKind, RepoStatus, Repository,
    ThreadSummary, Workspace, WorkspaceRepo,
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
        client: Some("omfp-ui/web".to_owned()),
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

pub fn workspace() -> Workspace {
    Workspace {
        id: WORKSPACE_ID.to_owned(),
        name: "checkout-v2".to_owned(),
        path: "/home/dev/omfp/workspaces/checkout-v2".to_owned(),
        owned: true,
        repos: vec![
            WorkspaceRepo {
                name: "backend".to_owned(),
                path: "/home/dev/omfp/workspaces/checkout-v2/backend".to_owned(),
                kind: RepoKind::Link {
                    target: "/home/dev/src/shop-backend".to_owned(),
                },
                branch: Some("main".to_owned()),
                head: Some("f6ab326c0e3d4b2a1f9e8d7c6b5a4f3e2d1c0b9a".to_owned()),
                is_root: false,
                canonical_path: "/home/dev/src/shop-backend".to_owned(),
                shared_with: vec![OTHER_WORKSPACE_ID.to_owned()],
            },
            WorkspaceRepo {
                name: "shop-frontend".to_owned(),
                path: "/home/dev/omfp/workspaces/checkout-v2/shop-frontend".to_owned(),
                kind: RepoKind::Worktree {
                    main: "/home/dev/src/shop-frontend".to_owned(),
                },
                branch: Some("omfp/checkout-v2-1".to_owned()),
                head: Some("0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d".to_owned()),
                is_root: false,
                canonical_path: "/home/dev/omfp/workspaces/checkout-v2/shop-frontend".to_owned(),
                shared_with: Vec::new(),
            },
        ],
        thread_ids: vec![THREAD_ID.to_owned()],
        provenance: user_provenance(),
        created_at: CREATED_AT,
    }
}

/// An existing folder that is itself a repository, recorded by an agent.
pub fn root_workspace() -> Workspace {
    Workspace {
        id: OTHER_WORKSPACE_ID.to_owned(),
        name: "shop-backend".to_owned(),
        path: "/home/dev/src/shop-backend".to_owned(),
        owned: false,
        repos: vec![WorkspaceRepo {
            name: "shop-backend".to_owned(),
            path: "/home/dev/src/shop-backend".to_owned(),
            kind: RepoKind::Clone,
            branch: Some("main".to_owned()),
            head: Some("f6ab326c0e3d4b2a1f9e8d7c6b5a4f3e2d1c0b9a".to_owned()),
            is_root: true,
            canonical_path: "/home/dev/src/shop-backend".to_owned(),
            shared_with: vec![WORKSPACE_ID.to_owned()],
        }],
        thread_ids: Vec::new(),
        provenance: Provenance {
            created_by: CreatedBy::Thread {
                thread_id: THREAD_ID.to_owned(),
            },
            client: Some("omfp-agent".to_owned()),
        },
        created_at: CREATED_AT,
    }
}

pub fn thread() -> ThreadSummary {
    ThreadSummary {
        id: THREAD_ID.to_owned(),
        title: "Checkout v2".to_owned(),
        tags: vec!["checkout-v2".to_owned()],
        workspace_id: WORKSPACE_ID.to_owned(),
        process: ProcessState::Stopped,
        settled: true,
        needs_attention: false,
        provenance: user_provenance(),
        created_at: CREATED_AT,
        updated_at: CREATED_AT + 60_000,
        archived_at: None,
    }
}

pub fn repo_statuses() -> Vec<RepoStatus> {
    vec![
        RepoStatus {
            name: "backend".to_owned(),
            status: GitStatus {
                branch: Some("main".to_owned()),
                base_ref: None,
                ahead: 0,
                behind: 0,
                dirty: false,
                files: Vec::new(),
            },
        },
        RepoStatus {
            name: "shop-frontend".to_owned(),
            status: git_status(),
        },
    ]
}

pub fn git_status() -> GitStatus {
    GitStatus {
        branch: Some("omfp/checkout-v2-1".to_owned()),
        base_ref: Some("origin/main".to_owned()),
        ahead: 2,
        behind: 1,
        dirty: true,
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
