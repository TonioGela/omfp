//! Authorization (PRT-034): a connection's [`Principal`] is resolved once by `system.hello`, and every request is
//! checked in one place against a method → [`Permission`] table, so scoped principals (per-thread agent tokens,
//! 08-agent-control) only touch this module.

use omfp_protocol::{ErrorBody, ErrorCode};

use crate::store::Provenance;

/// Who is on the other end of a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// Holder of the daemon's token; `client` is the name sent in `system.hello` (PRT-033).
    Owner { client: String },
}

impl Principal {
    /// The `client` of `system.hello`, recorded as provenance (PRT-033).
    #[must_use]
    pub fn client(&self) -> &str {
        match self {
            Self::Owner { client } => client,
        }
    }

    /// Who created what this principal creates (ARC-009).
    #[must_use]
    pub fn created_by(&self) -> Provenance {
        match self {
            Self::Owner { .. } => Provenance::User,
        }
    }
}

/// What a method needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Session plumbing: hello, subscriptions.
    Connect,
    /// Read workspaces, threads, their state, repository suggestions and directory listings.
    Read,
    /// Create, change or delete workspaces and threads.
    Manage,
    /// Drive a thread's agent: prompts, answers, process control.
    Drive,
    /// Open and use terminals.
    Terminal,
}

/// The method → permission table; `None` for an unknown method.
#[must_use]
pub fn permission(method: &str) -> Option<Permission> {
    use Permission::{Connect, Drive, Manage, Read, Terminal};
    Some(match method {
        "system.hello" | "subscribe" | "unsubscribe" | "widgets.report" => Connect,
        "repos.suggest"
        | "fs.complete"
        | "threads.search"
        | "workspaces.status"
        | "workspaces.diff"
        | "workspaces.read_file"
        | "workspaces.find_file"
        | "thread.models"
        | "thread.thinking_levels"
        | "thread.commands"
        | "thread.tree"
        | "thread.subagents"
        | "thread.subagent_messages"
        | "thread.git"
        | "thread.state"
        | "theme.get"
        | "settings.get" => Read,
        "workspaces.create"
        | "workspaces.update"
        | "workspaces.remove"
        | "workspaces.rescan"
        | "workspaces.add_repo"
        | "workspaces.remove_repo"
        | "workspaces.write_file"
        | "threads.create"
        | "threads.update"
        | "threads.archive"
        | "threads.unarchive"
        | "threads.delete"
        | "theme.set"
        | "settings.set"
        | "views.set"
        | "widgets.copy"
        | "widgets.state_set" => Manage,
        "thread.prompt"
        | "thread.steer"
        | "thread.follow_up"
        | "thread.abort"
        | "thread.queue.remove"
        | "thread.queue.promote"
        | "thread.set_model"
        | "thread.set_thinking"
        | "thread.compact"
        | "thread.navigate"
        | "thread.fork"
        | "thread.label"
        | "thread.subagent_steer"
        | "thread.subagent_cancel"
        | "thread.goal"
        | "thread.btw"
        | "thread.bash"
        | "thread.restart"
        | "thread.stop"
        | "thread.live_connect"
        | "thread.live_stop"
        | "thread.live_mute"
        | "interaction.respond"
        | "jobs.start"
        | "jobs.kill" => Drive,
        "terminal.open" | "terminal.resize" | "terminal.close" | "terminal.list" => Terminal,
        _ => return None,
    })
}

/// The single authorization check for a request of `method` by `principal`.
pub fn authorize(principal: &Principal, method: &str) -> Result<(), ErrorBody> {
    let Some(permission) = permission(method) else {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("unknown method `{method}`"),
        ));
    };
    match (principal, permission) {
        // The owner token grants everything.
        (
            Principal::Owner { .. },
            Permission::Connect
            | Permission::Read
            | Permission::Manage
            | Permission::Drive
            | Permission::Terminal,
        ) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: PRT-034
    #[test]
    fn every_method_has_exactly_one_permission() {
        let missing: Vec<&str> = omfp_protocol::METHODS
            .iter()
            .copied()
            .filter(|method| permission(method).is_none())
            .collect();

        assert_eq!(missing, Vec::<&str>::new());
        assert_eq!(permission("thread.prompt"), Some(Permission::Drive));
        assert_eq!(permission("thread.state"), Some(Permission::Read));
        assert_eq!(permission("no.such.method"), None);
    }

    // spec: PRT-034
    #[test]
    fn the_owner_may_call_every_method() {
        let owner = Principal::Owner {
            client: "omfp/web".into(),
        };

        for method in omfp_protocol::METHODS {
            assert_eq!(authorize(&owner, method), Ok(()), "{method}");
        }
        assert_eq!(
            authorize(&owner, "no.such.method").unwrap_err().code,
            ErrorCode::BadRequest
        );
    }

    // spec: PRT-033
    #[test]
    fn the_owner_creates_entities_as_the_user_and_names_its_client() {
        let owner = Principal::Owner {
            client: "script/ci".into(),
        };

        assert_eq!(owner.created_by(), Provenance::User);
        assert_eq!(owner.client(), "script/ci");
    }
}
