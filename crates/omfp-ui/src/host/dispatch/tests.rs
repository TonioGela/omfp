use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use futures::executor::block_on;
use omfp_core::wire::{QueuedMessageQueue, ThinkingLevel};
use omfp_protocol::{ErrorBody, ErrorCode, InteractionResponse, PromptBehavior, Request};
use serde_json::{Value, json};

use super::*;

/// What the dispatcher did, in order.
#[derive(Debug, Clone, PartialEq)]
enum Sent {
    Call(&'static str, Value),
    Subscribe(Topic),
    Unsubscribe(Topic),
}

/// Answers per method, in order.
type Script = HashMap<&'static str, VecDeque<Result<Value, CallError>>>;

/// Records calls and answers them from a per-method script (`{}` when unscripted).
#[derive(Debug, Clone, Default)]
struct FakeRpc {
    sent: Rc<RefCell<Vec<Sent>>>,
    answers: Rc<RefCell<Script>>,
}

impl FakeRpc {
    fn answer(&self, method: &'static str, answer: Result<Value, CallError>) {
        self.answers
            .borrow_mut()
            .entry(method)
            .or_default()
            .push_back(answer);
    }

    fn take(&self) -> Vec<Sent> {
        std::mem::take(&mut self.sent.borrow_mut())
    }

    fn methods(&self) -> Vec<&'static str> {
        self.take()
            .into_iter()
            .filter_map(|sent| match sent {
                Sent::Call(method, _) => Some(method),
                _ => None,
            })
            .collect()
    }
}

impl Rpc for FakeRpc {
    fn call(&self, call: Call) -> impl Future<Output = Result<Value, CallError>> + 'static {
        let method = call.method();
        let params = serde_json::to_value(Request {
            id: "1".into(),
            call,
        })
        .expect("request encodes")["params"]
            .clone();
        self.sent.borrow_mut().push(Sent::Call(method, params));
        let answer = self
            .answers
            .borrow_mut()
            .get_mut(method)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| Ok(json!({})));
        std::future::ready(answer)
    }

    fn subscribe(&self, topic: Topic) {
        self.sent.borrow_mut().push(Sent::Subscribe(topic));
    }

    fn unsubscribe(&self, topic: Topic) {
        self.sent.borrow_mut().push(Sent::Unsubscribe(topic));
    }
}

fn daemon_error(message: &str) -> CallError {
    CallError::Daemon(ErrorBody::new(ErrorCode::BadRequest, message))
}

fn thread_topic(id: &str) -> Topic {
    Topic::Thread(id.to_owned())
}

fn select(id: &str) -> Intent {
    Intent::SelectThread {
        thread_id: id.to_owned(),
    }
}

fn outcomes(effects: Vec<Effect>) -> Vec<IntentOutcome> {
    effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Outcome(outcome) => Some(outcome),
            _ => None,
        })
        .collect()
}

// spec: DMN-034, UI-010
#[test]
fn selecting_a_thread_moves_the_subscription_and_fetches_its_details() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer("thread.state", Ok(json!({ "sessionId": "s1" })));
    rpc.answer("thread.models", Ok(json!({ "models": [] })));
    rpc.answer(
        "thread.thinking_levels",
        Ok(json!({ "levels": ["off", "low"] })),
    );
    rpc.answer(
        "thread.git",
        Ok(json!({ "workspaceId": "w1", "repos": [] })),
    );

    let future = dispatcher.dispatch(select("t1"));
    // The subscription changes before any call is awaited.
    assert_eq!(rpc.sent.borrow()[0], Sent::Subscribe(thread_topic("t1")));
    let effects = block_on(future);
    let session: omfp_core::wire::SessionState =
        serde_json::from_value(json!({ "sessionId": "s1" })).unwrap();
    assert_eq!(
        effects,
        vec![
            Effect::Details {
                thread_id: "t1".into(),
                session: Some(Box::new(session)),
                models: Some(Vec::new()),
                thinking_levels: Some(vec![ThinkingLevel::Off, ThinkingLevel::Low]),
            },
            Effect::Git {
                workspace_id: "w1".into(),
                repos: Vec::new(),
            },
        ]
    );
    let thread = json!({ "threadId": "t1" });
    assert_eq!(
        rpc.take(),
        vec![
            Sent::Subscribe(thread_topic("t1")),
            Sent::Call("thread.state", thread.clone()),
            Sent::Call("thread.models", thread.clone()),
            Sent::Call("thread.thinking_levels", thread.clone()),
            Sent::Call("thread.git", thread),
        ]
    );

    // Another thread: the previous one is no longer watched.
    let _ = block_on(dispatcher.dispatch(select("t2")));
    let sent = rpc.take();
    assert_eq!(
        sent[..2],
        [
            Sent::Unsubscribe(thread_topic("t1")),
            Sent::Subscribe(thread_topic("t2"))
        ]
    );
    // The same thread again only refreshes its details.
    let _ = block_on(dispatcher.dispatch(select("t2")));
    assert_eq!(
        rpc.methods(),
        vec![
            "thread.state",
            "thread.models",
            "thread.thinking_levels",
            "thread.git"
        ]
    );
}

#[test]
fn details_that_arrived_are_applied_when_another_fetch_fails() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer("thread.state", Err(daemon_error("omp failed to start")));
    rpc.answer("thread.thinking_levels", Ok(json!({ "levels": ["off"] })));
    rpc.answer("thread.models", Ok(json!({ "models": [] })));
    rpc.answer(
        "thread.git",
        Ok(json!({ "workspaceId": "w1", "repos": [] })),
    );
    let effects = block_on(dispatcher.dispatch(select("t1")));
    assert_eq!(
        effects,
        vec![
            Effect::Details {
                thread_id: "t1".into(),
                session: None,
                models: Some(Vec::new()),
                thinking_levels: Some(vec![ThinkingLevel::Off]),
            },
            Effect::Git {
                workspace_id: "w1".into(),
                repos: Vec::new(),
            },
            Effect::Outcome(IntentOutcome::Failed {
                intent: select("t1"),
                message: "omp failed to start (BadRequest)".into(),
            }),
        ]
    );
}

fn workspace_json(id: &str) -> Value {
    let repos = vec![crate::test_support::repo(id, "front", "main")];
    serde_json::to_value(crate::test_support::workspace(id, repos)).unwrap()
}

fn summary_json(id: &str) -> Value {
    serde_json::to_value(crate::test_support::thread(id, "t", "w-new")).unwrap()
}

// spec: UI-023
#[test]
fn a_new_thread_gets_a_new_workspace_then_its_repositories_in_order_then_its_first_message() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer("threads.create", Ok(summary_json("t1")));
    for _ in 0..2 {
        rpc.answer("workspaces.add_repo", Ok(workspace_json("w-new")));
    }
    rpc.answer(
        "thread.prompt",
        Ok(json!({ "promptId": "p1", "agentInvoked": true })),
    );
    let new = NewThread {
        title: "full stack".into(),
        tags: vec!["web".into()],
        repos: vec![
            RepoSource::Worktree {
                repo: "/src/front".into(),
                branch: Some("feat".into()),
                base_ref: None,
            },
            RepoSource::Link {
                path: "/src/back".into(),
            },
        ],
        prompt: Some("hello".into()),
    };
    let effects = block_on(dispatcher.dispatch(Intent::CreateThread(new)));
    assert_eq!(
        outcomes(effects),
        vec![IntentOutcome::ThreadCreated {
            thread_id: "t1".into()
        }]
    );
    assert_eq!(
        rpc.take(),
        vec![
            Sent::Call(
                "threads.create",
                json!({ "title": "full stack", "workspace": { "new": {} }, "tags": ["web"] })
            ),
            Sent::Call(
                "workspaces.add_repo",
                json!({
                    "workspaceId": "w-new",
                    "source": { "kind": "worktree", "repo": "/src/front", "branch": "feat" }
                })
            ),
            Sent::Call(
                "workspaces.add_repo",
                json!({ "workspaceId": "w-new", "source": { "kind": "link", "path": "/src/back" } })
            ),
            Sent::Call(
                "thread.prompt",
                json!({ "threadId": "t1", "text": "hello" })
            ),
        ]
    );
}

// spec: UI-023
#[test]
fn a_thread_whose_repository_or_first_message_failed_is_still_opened() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer("threads.create", Ok(summary_json("t1")));
    rpc.answer(
        "workspaces.add_repo",
        Err(daemon_error("not a git repository")),
    );
    let new = NewThread {
        title: "t".into(),
        tags: Vec::new(),
        repos: vec![RepoSource::Worktree {
            repo: "/tmp".into(),
            branch: None,
            base_ref: None,
        }],
        prompt: Some("hello".into()),
    };
    let effects = block_on(dispatcher.dispatch(Intent::CreateThread(new.clone())));
    assert_eq!(
        outcomes(effects),
        vec![
            IntentOutcome::ThreadCreated {
                thread_id: "t1".into()
            },
            IntentOutcome::Failed {
                intent: Intent::CreateThread(new),
                message: "not a git repository (BadRequest)".into(),
            },
        ]
    );
    assert_eq!(
        rpc.methods(),
        vec!["threads.create", "workspaces.add_repo"],
        "no first message without the repository"
    );
}

// spec: UI-025
#[test]
fn a_repository_goes_into_the_given_workspace_or_a_new_one() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer("workspaces.create", Ok(workspace_json("w-new")));
    for _ in 0..2 {
        rpc.answer("workspaces.add_repo", Ok(workspace_json("w-new")));
    }
    let link = RepoSource::Link {
        path: "/src/back".into(),
    };
    let source = json!({ "kind": "link", "path": "/src/back" });
    for workspace_id in [Some("w1".to_owned()), None] {
        let effects = block_on(dispatcher.dispatch(Intent::AddRepo {
            workspace_id,
            source: link.clone(),
        }));
        assert_eq!(effects, Vec::new());
    }
    assert_eq!(
        rpc.take(),
        vec![
            Sent::Call(
                "workspaces.add_repo",
                json!({ "workspaceId": "w1", "source": source })
            ),
            Sent::Call("workspaces.create", json!({})),
            Sent::Call(
                "workspaces.add_repo",
                json!({ "workspaceId": "w-new", "source": source })
            ),
        ]
    );
}

// spec: UI-063
#[test]
fn abort_returns_the_restored_queue_texts() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer(
        "thread.abort",
        Ok(json!({ "steering": [{ "text": "steer" }], "followUp": [{ "text": "later" }] })),
    );
    let effects = block_on(dispatcher.dispatch(Intent::Abort {
        thread_id: "t1".into(),
    }));
    assert_eq!(
        outcomes(effects),
        vec![IntentOutcome::Aborted {
            thread_id: "t1".into(),
            steering: vec!["steer".into()],
            follow_up: vec!["later".into()],
        }]
    );
}

// spec: UI-062
#[test]
fn queue_actions_report_whether_omp_applied_them() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer("thread.queue.remove", Ok(json!({ "removed": false })));
    rpc.answer("thread.queue.promote", Ok(json!({ "promoted": true })));
    for (op, applied) in [(QueueOp::Remove, false), (QueueOp::Promote, true)] {
        let effects = block_on(dispatcher.dispatch(Intent::Queue {
            thread_id: "t1".into(),
            op,
            message: "later".into(),
            queue: QueuedMessageQueue::FollowUp,
        }));
        assert_eq!(
            outcomes(effects),
            vec![IntentOutcome::Queue {
                thread_id: "t1".into(),
                op,
                message: "later".into(),
                applied,
            }]
        );
    }
    let params = json!({ "threadId": "t1", "message": "later", "queue": "followUp" });
    assert_eq!(
        rpc.take(),
        vec![
            Sent::Call("thread.queue.remove", params.clone()),
            Sent::Call("thread.queue.promote", params),
        ]
    );
}

/// Intents whose results the UI does not need, with the call each maps to.
fn call_cases() -> Vec<(Intent, &'static str, Value)> {
    vec![
        (
            Intent::UpdateThread {
                thread_id: "t1".into(),
                title: Some("renamed".into()),
                tags: None,
            },
            "threads.update",
            json!({ "threadId": "t1", "title": "renamed" }),
        ),
        (
            Intent::AddRepository {
                path: "/src/a".into(),
            },
            "repos.add",
            json!({ "path": "/src/a" }),
        ),
        (
            Intent::RemoveRepository {
                repository_id: "r1".into(),
            },
            "repos.remove",
            json!({ "repositoryId": "r1" }),
        ),
        (
            Intent::RemoveRepo {
                workspace_id: "w1".into(),
                name: "front".into(),
                force: true,
                delete_branch: false,
            },
            "workspaces.remove_repo",
            json!({ "workspaceId": "w1", "name": "front", "force": true, "deleteBranch": false }),
        ),
        (
            Intent::Prompt {
                thread_id: "t1".into(),
                text: "now".into(),
                images: Vec::new(),
                behavior: Some(PromptBehavior::Steer),
            },
            "thread.prompt",
            json!({ "threadId": "t1", "text": "now", "behavior": "steer" }),
        ),
        (
            Intent::Respond {
                thread_id: "t1".into(),
                interaction_id: "i1".into(),
                response: InteractionResponse::Confirmed(true),
            },
            "interaction.respond",
            json!({ "threadId": "t1", "interactionId": "i1", "response": { "confirmed": true } }),
        ),
        (
            Intent::Stop {
                thread_id: "t1".into(),
            },
            "thread.stop",
            json!({ "threadId": "t1" }),
        ),
        (
            Intent::Restart {
                thread_id: "t1".into(),
            },
            "thread.restart",
            json!({ "threadId": "t1" }),
        ),
    ]
}

// spec: UI-061, UI-025
#[test]
fn intents_without_results_map_to_their_calls_and_failures_come_back() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    let cases = call_cases();
    for (intent, method, params) in cases {
        assert_eq!(
            block_on(dispatcher.dispatch(intent.clone())),
            Vec::new(),
            "{method}"
        );
        assert_eq!(rpc.take(), vec![Sent::Call(method, params)]);

        rpc.answer(method, Err(CallError::ConnectionLost));
        assert_eq!(
            outcomes(block_on(dispatcher.dispatch(intent.clone()))),
            vec![IntentOutcome::Failed {
                intent,
                message: "the connection was lost before the response arrived".into(),
            }],
            "{method}"
        );
        rpc.take();
    }
}

// spec: UI-026
#[test]
fn archiving_unarchiving_and_deleting_map_to_their_calls_and_drop_the_watched_topic() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    let _ = block_on(dispatcher.dispatch(select("t1")));
    rpc.take();
    let thread = json!({ "threadId": "t1" });

    // Another thread's removal leaves the watched topic alone.
    let _ = block_on(dispatcher.dispatch(Intent::ArchiveThread {
        thread_id: "t2".into(),
    }));
    assert_eq!(
        rpc.take(),
        vec![Sent::Call("threads.archive", json!({ "threadId": "t2" }))]
    );
    assert_eq!(dispatcher.watched(), Some("t1"));

    let _ = block_on(dispatcher.dispatch(Intent::ArchiveThread {
        thread_id: "t1".into(),
    }));
    assert_eq!(
        rpc.take(),
        vec![
            Sent::Unsubscribe(thread_topic("t1")),
            Sent::Call("threads.archive", thread.clone()),
        ]
    );
    assert_eq!(dispatcher.watched(), None);

    let _ = block_on(dispatcher.dispatch(Intent::UnarchiveThread {
        thread_id: "t1".into(),
    }));
    assert_eq!(
        rpc.take(),
        vec![Sent::Call("threads.unarchive", thread.clone())]
    );

    let _ = block_on(dispatcher.dispatch(select("t1")));
    rpc.take();
    rpc.answer("threads.delete", Err(CallError::ConnectionLost));
    let intent = Intent::DeleteThread {
        thread_id: "t1".into(),
    };
    assert_eq!(
        outcomes(block_on(dispatcher.dispatch(intent.clone()))),
        vec![IntentOutcome::Failed {
            intent,
            message: "the connection was lost before the response arrived".into(),
        }]
    );
    assert_eq!(
        rpc.take(),
        vec![
            Sent::Unsubscribe(thread_topic("t1")),
            Sent::Call("threads.delete", thread),
        ]
    );
    assert_eq!(dispatcher.watched(), None);
}

// spec: UI-030
#[test]
fn model_and_thinking_changes_refresh_the_session_state() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer("thread.state", Ok(json!({ "sessionId": "s2" })));
    let effects = block_on(dispatcher.dispatch(Intent::SetThinking {
        thread_id: "t1".into(),
        level: ThinkingLevel::High,
    }));
    assert_eq!(
        effects,
        vec![Effect::Details {
            thread_id: "t1".into(),
            session: Some(serde_json::from_value(json!({ "sessionId": "s2" })).unwrap()),
            models: None,
            thinking_levels: None,
        }]
    );
    assert_eq!(
        rpc.take(),
        vec![
            Sent::Call(
                "thread.set_thinking",
                json!({ "threadId": "t1", "level": "high" })
            ),
            Sent::Call("thread.state", json!({ "threadId": "t1" })),
        ]
    );
    let _ = block_on(dispatcher.dispatch(Intent::SetModel {
        thread_id: "t1".into(),
        provider: "fake".into(),
        model_id: "scripted".into(),
    }));
    assert_eq!(rpc.methods(), vec!["thread.set_model", "thread.state"]);
}

#[test]
fn refreshing_a_workspace_rescans_it_and_stores_the_git_status_of_its_repositories() {
    let rpc = FakeRpc::default();
    let mut dispatcher = Dispatcher::new(rpc.clone());
    rpc.answer("workspaces.rescan", Ok(workspace_json("w1")));
    rpc.answer(
        "workspaces.status",
        Ok(json!({ "branch": "feat", "ahead": 2, "behind": 0, "dirty": false, "files": [] })),
    );
    let effects = block_on(dispatcher.dispatch(Intent::RefreshWorkspace {
        workspace_id: "w1".into(),
    }));
    let Some(Effect::Git {
        workspace_id,
        repos,
    }) = effects.first()
    else {
        panic!("{effects:?}");
    };
    assert_eq!(
        (
            workspace_id.as_str(),
            repos[0].name.as_str(),
            repos[0].status.ahead
        ),
        ("w1", "front", 2)
    );
    assert_eq!(
        rpc.take()[1],
        Sent::Call(
            "workspaces.status",
            json!({ "workspaceId": "w1", "repo": "front" })
        )
    );
}
