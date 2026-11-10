//! Golden JSON of every protocol method: one example request and its response per method, as insta snapshots, so
//! every change to the wire format shows up in review.

use omfp_core::wire::{AskAnswer, GoalOp, ImageContent, QueuedMessageQueue, ThinkingLevel};
use omfp_protocol::{
    AddRepositoryParams, AdoptWorkspaceParams, AttachWorkspaceParams, BashParams, BtwParams, Call,
    CompactParams, CreateThreadParams, CreateWorkspaceParams, DetachWorkspaceParams, DiffPage,
    Empty, FileDiff, ForkParams, GoalParams, HelloParams, HelloResult, IndexSnapshot,
    InteractionResponse, LabelParams, ListTerminalsParams, METHODS, MessageParams, Method,
    NavigateParams, OpenTerminalParams, OpenTerminalResult, PROTOCOL_VERSION, Position,
    PromptBehavior, PromptParams, PromptResult, QueueMessageParams, RemoveWorkspaceParams,
    RepositoryList, RepositoryRef, Request, ResizeTerminalParams, RespondParams, Response,
    ServerFrame, SetModelParams, SetThinkingParams, SubagentMessagesParams, SubagentRef,
    SubagentSteerParams, SubscribeParams, SubscribeResult, TerminalInfo, TerminalKind,
    TerminalList, TerminalRef, ThreadGit, ThreadRef, Topic, TopicParams, UpdateThreadParams,
    WorkspaceDiffParams, WorkspaceFromPrParams, WorkspaceGit, WorkspaceRef, decode_request, method,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::samples::{
    OTHER_WORKSPACE_ID, REPO_ID, TERMINAL_ID, THREAD_ID, WORKSPACE_ID, adopted_workspace,
    git_status, managed_workspace, pr_workspace, repository, thread,
};

const REQUEST_ID: &str = "c42";

/// Checks that the example request and response survive a round trip through their JSON text, then returns the golden
/// document for the method.
fn example<M: Method>(params: M::Params, result: &M::Result) -> (&'static str, Value) {
    let request = Request::new(REQUEST_ID, M::call(params));
    assert_eq!(request.call.method(), M::NAME);
    let text = serde_json::to_string(&request).expect("request serializes");
    assert_eq!(
        decode_request(&text).expect("example request decodes"),
        request
    );

    let response =
        ServerFrame::Response(Response::ok(REQUEST_ID, result).expect("result serializes"));
    let text = serde_json::to_string(&response).expect("response serializes");
    let decoded: ServerFrame = serde_json::from_str(&text).expect("example response decodes");
    assert_eq!(decoded, response);
    let ServerFrame::Response(Response {
        result: Ok(value), ..
    }) = decoded
    else {
        panic!("expected a successful response, got {decoded:?}");
    };
    let typed: M::Result = serde_json::from_value(value).expect("result decodes");
    assert_eq!(&typed, result);

    (M::NAME, json!({ "request": request, "response": response }))
}

/// Builds an omp-defined value from its JSON form.
fn wire<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("valid omp wire value")
}

fn thread_ref() -> ThreadRef {
    ThreadRef {
        thread_id: THREAD_ID.to_owned(),
    }
}

fn png() -> ImageContent {
    wire(json!({ "type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png" }))
}

fn model_info() -> Value {
    json!({
        "id": "scripted",
        "name": "Scripted",
        "api": "openai-completions",
        "provider": "fake",
        "baseUrl": "http://127.0.0.1:4000/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": { "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0 },
        "contextWindow": 128_000,
        "maxTokens": 8192
    })
}

fn system_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::SystemHello>(
            HelloParams {
                token: "3f9c1e…".to_owned(),
                protocol: PROTOCOL_VERSION,
                client: "omfp-ui/web".to_owned(),
            },
            &HelloResult {
                protocol: PROTOCOL_VERSION,
                daemon: "0.1.0".to_owned(),
                omp: "18.18.6".to_owned(),
                warnings: vec!["omp version mismatch allowed (--allow-omp-mismatch)".to_owned()],
            },
        ),
        example::<method::Subscribe>(
            SubscribeParams {
                topic: Topic::Thread(THREAD_ID.to_owned()),
                since: Some(Position {
                    epoch: "e7".to_owned(),
                    seq: 1200,
                }),
            },
            &SubscribeResult::Snapshot {
                epoch: "e9".to_owned(),
                seq: 40,
                state: json!({ "transcript": [], "queue": { "steering": [], "followUp": [] } }),
            },
        ),
        example::<method::Unsubscribe>(
            TopicParams {
                topic: Topic::Index,
            },
            &Empty {},
        ),
    ]
}

fn repository_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::ReposAdd>(
            AddRepositoryParams {
                path: "/home/dev/src/shop-frontend".to_owned(),
            },
            &repository(),
        ),
        example::<method::ReposRemove>(
            RepositoryRef {
                repository_id: REPO_ID.to_owned(),
            },
            &Empty {},
        ),
        example::<method::ReposList>(
            Empty {},
            &RepositoryList {
                repositories: vec![repository()],
            },
        ),
    ]
}

fn workspace_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::WorkspacesCreate>(
            CreateWorkspaceParams {
                repo: REPO_ID.to_owned(),
                branch: None,
                base_ref: Some("origin/main".to_owned()),
            },
            &managed_workspace(),
        ),
        example::<method::WorkspacesFromPr>(
            WorkspaceFromPrParams {
                pr: "acme/shop-frontend#412".to_owned(),
                repo: Some("/home/dev/src/shop-frontend".to_owned()),
            },
            &pr_workspace(),
        ),
        example::<method::WorkspacesAdopt>(
            AdoptWorkspaceParams {
                path: "/home/dev/notes".to_owned(),
            },
            &adopted_workspace(),
        ),
        example::<method::WorkspacesRemove>(
            RemoveWorkspaceParams {
                workspace_id: WORKSPACE_ID.to_owned(),
                force: true,
                delete_branch: false,
            },
            &Empty {},
        ),
        example::<method::WorkspacesForget>(
            WorkspaceRef {
                workspace_id: OTHER_WORKSPACE_ID.to_owned(),
            },
            &Empty {},
        ),
        example::<method::WorkspacesStatus>(
            WorkspaceRef {
                workspace_id: WORKSPACE_ID.to_owned(),
            },
            &git_status(),
        ),
        example::<method::WorkspacesDiff>(
            WorkspaceDiffParams {
                workspace_id: WORKSPACE_ID.to_owned(),
                path: None,
                cursor: Some("p2".to_owned()),
            },
            &DiffPage {
                files: vec![FileDiff {
                    path: "src/checkout.rs".to_owned(),
                    patch: "@@ -1,3 +1,4 @@\n use std::fmt;\n+use crate::cart;\n".to_owned(),
                    truncated: false,
                }],
                next_cursor: Some("p3".to_owned()),
            },
        ),
    ]
}

fn threads_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::ThreadsCreate>(
            CreateThreadParams {
                title: "Checkout v2".to_owned(),
                workspace_ids: vec![WORKSPACE_ID.to_owned(), OTHER_WORKSPACE_ID.to_owned()],
                tags: vec!["checkout-v2".to_owned()],
                model: Some("anthropic/claude-sonnet-4-5".to_owned()),
                thinking_level: Some(ThinkingLevel::High),
                approval_mode: Some("always-ask".to_owned()),
                prompt: Some("Implement checkout v2 as described in docs/checkout.md".to_owned()),
            },
            &thread(),
        ),
        example::<method::ThreadsUpdate>(
            UpdateThreadParams {
                thread_id: THREAD_ID.to_owned(),
                title: Some("Checkout v2".to_owned()),
                tags: Some(vec!["checkout-v2".to_owned()]),
            },
            &thread(),
        ),
        example::<method::ThreadsAttach>(
            AttachWorkspaceParams {
                thread_id: THREAD_ID.to_owned(),
                workspace_id: OTHER_WORKSPACE_ID.to_owned(),
                position: Some(1),
            },
            &thread(),
        ),
        example::<method::ThreadsDetach>(
            DetachWorkspaceParams {
                thread_id: THREAD_ID.to_owned(),
                workspace_id: OTHER_WORKSPACE_ID.to_owned(),
            },
            &thread(),
        ),
        example::<method::ThreadsArchive>(thread_ref(), &Empty {}),
        example::<method::ThreadsDelete>(thread_ref(), &Empty {}),
    ]
}

fn conversation_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::ThreadPrompt>(
            PromptParams {
                thread_id: THREAD_ID.to_owned(),
                text: "Add a test for the cart total".to_owned(),
                images: vec![png()],
                behavior: Some(PromptBehavior::FollowUp),
            },
            &PromptResult {
                prompt_id: "p17".to_owned(),
                agent_invoked: true,
            },
        ),
        example::<method::ThreadSteer>(
            MessageParams {
                thread_id: THREAD_ID.to_owned(),
                text: "Use the existing Money type".to_owned(),
                images: Vec::new(),
            },
            &Empty {},
        ),
        example::<method::ThreadFollowUp>(
            MessageParams {
                thread_id: THREAD_ID.to_owned(),
                text: "Then run the tests".to_owned(),
                images: Vec::new(),
            },
            &Empty {},
        ),
        example::<method::ThreadAbort>(
            thread_ref(),
            &wire(json!({
                "steering": [{ "text": "Use the existing Money type" }],
                "followUp": [{ "text": "Then run the tests" }]
            })),
        ),
        example::<method::ThreadQueueRemove>(
            QueueMessageParams {
                thread_id: THREAD_ID.to_owned(),
                message: "Then run the tests".to_owned(),
                queue: Some(QueuedMessageQueue::FollowUp),
            },
            &wire(json!({ "removed": true })),
        ),
        example::<method::ThreadQueuePromote>(
            QueueMessageParams {
                thread_id: THREAD_ID.to_owned(),
                message: "Then run the tests".to_owned(),
                queue: None,
            },
            &wire(json!({ "promoted": true })),
        ),
    ]
}

fn settings_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::ThreadSetModel>(
            SetModelParams {
                thread_id: THREAD_ID.to_owned(),
                provider: "fake".to_owned(),
                model_id: "scripted".to_owned(),
            },
            &Empty {},
        ),
        example::<method::ThreadSetThinking>(
            SetThinkingParams {
                thread_id: THREAD_ID.to_owned(),
                level: ThinkingLevel::Medium,
            },
            &Empty {},
        ),
        example::<method::ThreadModels>(thread_ref(), &wire(json!({ "models": [model_info()] }))),
        example::<method::ThreadThinkingLevels>(
            thread_ref(),
            &wire(json!({ "levels": ["off", "low", "medium", "high"] })),
        ),
        example::<method::ThreadCommands>(
            thread_ref(),
            &wire(json!({
                "commands": [{
                    "name": "compact",
                    "source": "builtin",
                    "description": "Compact the conversation",
                    "input": { "hint": "[instructions]" }
                }]
            })),
        ),
        example::<method::ThreadCompact>(
            CompactParams {
                thread_id: THREAD_ID.to_owned(),
                instructions: Some("Keep the API decisions".to_owned()),
            },
            &wire(json!({
                "summary": "Implemented the cart total; tests pass.",
                "firstKeptEntryId": "e12",
                "tokensBefore": 48_210
            })),
        ),
    ]
}

fn tree_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::ThreadTree>(
            thread_ref(),
            &wire(json!({
                "tree": [{ "id": "e1", "type": "message", "children": [] }],
                "leafId": "e1"
            })),
        ),
        example::<method::ThreadNavigate>(
            NavigateParams {
                thread_id: THREAD_ID.to_owned(),
                entry_id: "e1".to_owned(),
                summarize: Some(true),
                instructions: None,
            },
            &wire(json!({ "cancelled": false, "editorText": "Add a test for the cart total" })),
        ),
        example::<method::ThreadFork>(
            ForkParams {
                thread_id: THREAD_ID.to_owned(),
                entry_id: Some("e1".to_owned()),
            },
            &wire(json!({ "cancelled": false })),
        ),
        example::<method::ThreadLabel>(
            LabelParams {
                thread_id: THREAD_ID.to_owned(),
                entry_id: "e1".to_owned(),
                label: Some("before refactor".to_owned()),
            },
            &Empty {},
        ),
    ]
}

fn subagent_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::ThreadSubagents>(
            thread_ref(),
            &wire(json!({
                "subagents": [{
                    "id": "sa1",
                    "index": 0,
                    "agent": "explore",
                    "agentSource": "bundled",
                    "status": "running",
                    "lastUpdate": 1_794_268_800_000_i64,
                    "description": "Find the cart code"
                }]
            })),
        ),
        example::<method::ThreadSubagentMessages>(
            SubagentMessagesParams {
                thread_id: THREAD_ID.to_owned(),
                subagent_id: "sa1".to_owned(),
                from_byte: Some(0),
            },
            &wire(json!({
                "sessionFile": "/home/dev/.local/share/omfp/threads/t/sessions/sa1.jsonl",
                "fromByte": 0,
                "nextByte": 0,
                "reset": false,
                "entries": [],
                "messages": []
            })),
        ),
        example::<method::ThreadSubagentSteer>(
            SubagentSteerParams {
                thread_id: THREAD_ID.to_owned(),
                subagent_id: "sa1".to_owned(),
                text: "Only look under src/cart".to_owned(),
            },
            &Empty {},
        ),
        example::<method::ThreadSubagentCancel>(
            SubagentRef {
                thread_id: THREAD_ID.to_owned(),
                subagent_id: "sa1".to_owned(),
            },
            &Empty {},
        ),
    ]
}

fn tool_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::ThreadGoal>(
            GoalParams {
                thread_id: THREAD_ID.to_owned(),
                op: GoalOp::Create,
                objective: Some("Ship checkout v2".to_owned()),
                token_budget: Some(200_000),
            },
            &wire(json!({
                "goal": {
                    "id": "g1",
                    "objective": "Ship checkout v2",
                    "status": "active",
                    "tokensUsed": 0,
                    "timeUsedSeconds": 0.0,
                    "createdAt": 1_794_268_800_000_i64,
                    "updatedAt": 1_794_268_800_000_i64,
                    "tokenBudget": 200_000
                },
                "state": null
            })),
        ),
        example::<method::ThreadBtw>(
            BtwParams {
                thread_id: THREAD_ID.to_owned(),
                question: "Which test runner does this repo use?".to_owned(),
                record_id: None,
            },
            &wire(json!({
                "record": {
                    "id": "b1",
                    "question": "Which test runner does this repo use?",
                    "answer": "cargo-nextest",
                    "status": "complete",
                    "createdAt": 1_794_268_800_000_i64,
                    "updatedAt": 1_794_268_801_000_i64,
                    "leafId": null
                }
            })),
        ),
        example::<method::ThreadBash>(
            BashParams {
                thread_id: THREAD_ID.to_owned(),
                command: "git log --oneline -1".to_owned(),
                exclude_from_context: Some(true),
            },
            &wire(json!({
                "output": "f6ab326 feat(daemon): add omp process adapter\n",
                "cancelled": false,
                "truncated": false,
                "totalLines": 1,
                "totalBytes": 45,
                "outputLines": 1,
                "outputBytes": 45,
                "exitCode": 0
            })),
        ),
        example::<method::ThreadGit>(
            thread_ref(),
            &ThreadGit {
                workspaces: vec![
                    WorkspaceGit {
                        workspace_id: WORKSPACE_ID.to_owned(),
                        status: Some(git_status()),
                    },
                    WorkspaceGit {
                        workspace_id: OTHER_WORKSPACE_ID.to_owned(),
                        status: None,
                    },
                ],
            },
        ),
        example::<method::ThreadState>(
            thread_ref(),
            &wire(
                json!({ "sessionId": "0f8e2c4a-5b1d-4e7f-9a3c-2d6b8e1f4a7c", "model": model_info() }),
            ),
        ),
        example::<method::ThreadRestart>(thread_ref(), &Empty {}),
        example::<method::ThreadStop>(thread_ref(), &Empty {}),
        example::<method::InteractionRespond>(
            RespondParams {
                thread_id: THREAD_ID.to_owned(),
                interaction_id: "ui-7".to_owned(),
                response: InteractionResponse::Confirmed(true),
            },
            &Empty {},
        ),
    ]
}

fn terminal_examples() -> Vec<(&'static str, Value)> {
    vec![
        example::<method::TerminalOpen>(
            OpenTerminalParams {
                kind: TerminalKind::Omp,
                thread_id: Some(THREAD_ID.to_owned()),
                workspace_id: None,
                cols: 120,
                rows: 40,
            },
            &OpenTerminalResult {
                terminal_id: TERMINAL_ID.to_owned(),
                channel: 1,
            },
        ),
        example::<method::TerminalResize>(
            ResizeTerminalParams {
                terminal_id: TERMINAL_ID.to_owned(),
                cols: 100,
                rows: 30,
            },
            &Empty {},
        ),
        example::<method::TerminalClose>(
            TerminalRef {
                terminal_id: TERMINAL_ID.to_owned(),
            },
            &Empty {},
        ),
        example::<method::TerminalList>(
            ListTerminalsParams {
                thread_id: Some(THREAD_ID.to_owned()),
                workspace_id: None,
            },
            &TerminalList {
                terminals: vec![TerminalInfo {
                    id: TERMINAL_ID.to_owned(),
                    kind: TerminalKind::Shell,
                    thread_id: Some(THREAD_ID.to_owned()),
                    workspace_id: Some(WORKSPACE_ID.to_owned()),
                    cols: 120,
                    rows: 40,
                }],
            },
        ),
    ]
}

// spec: PRT-031, PRT-010
#[test]
fn every_method_has_a_golden_request_and_response() {
    let examples: Vec<(&str, Value)> = [
        system_examples(),
        repository_examples(),
        workspace_examples(),
        threads_examples(),
        conversation_examples(),
        settings_examples(),
        tree_examples(),
        subagent_examples(),
        tool_examples(),
        terminal_examples(),
    ]
    .concat();

    let names: Vec<&str> = examples.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, METHODS);
    for (name, golden) in examples {
        insta::assert_json_snapshot!(name, golden);
    }
}

// spec: PRT-020
#[test]
fn subscribe_answers_with_a_replay_or_a_snapshot() {
    let replay = SubscribeResult::Replay {
        epoch: "e7".to_owned(),
        from_seq: 1201,
    };
    assert_eq!(
        serde_json::to_value(&replay).unwrap(),
        json!({ "mode": "replay", "epoch": "e7", "fromSeq": 1201 })
    );
    let snapshot = SubscribeResult::Snapshot {
        epoch: "e9".to_owned(),
        seq: 40,
        state: json!({}),
    };
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap(),
        json!({ "mode": "snapshot", "epoch": "e9", "seq": 40, "state": {} })
    );
    for result in [replay, snapshot] {
        let text = serde_json::to_string(&result).unwrap();
        assert_eq!(
            serde_json::from_str::<SubscribeResult>(&text).unwrap(),
            result
        );
    }
}

// spec: PRT-020
#[test]
fn subscribe_without_since_omits_it() {
    let call = Call::Subscribe(SubscribeParams {
        topic: Topic::Index,
        since: None,
    });
    let request = serde_json::to_value(Request::new("c7", call)).unwrap();
    assert_eq!(
        request,
        json!({ "type": "request", "id": "c7", "method": "subscribe", "params": { "topic": "index" } })
    );
}

#[test]
fn interaction_responses_carry_exactly_one_answer_key() {
    let answer: AskAnswer = wire(json!({ "id": "q1", "selectedOptions": ["Postgres"] }));
    let cases = [
        (
            InteractionResponse::Value("main".to_owned()),
            json!({ "value": "main" }),
        ),
        (
            InteractionResponse::Confirmed(false),
            json!({ "confirmed": false }),
        ),
        (InteractionResponse::Cancelled, json!({ "cancelled": true })),
        (
            InteractionResponse::Answers(vec![answer]),
            json!({ "answers": [{ "id": "q1", "selectedOptions": ["Postgres"] }] }),
        ),
    ];
    for (response, expected) in cases {
        assert_eq!(serde_json::to_value(&response).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<InteractionResponse>(expected).unwrap(),
            response
        );
    }
    assert!(serde_json::from_value::<InteractionResponse>(json!({ "cancelled": false })).is_err());
}

#[test]
fn index_snapshot_lists_repositories_workspaces_and_threads() {
    let snapshot = IndexSnapshot {
        repositories: vec![repository()],
        workspaces: vec![managed_workspace(), adopted_workspace()],
        threads: vec![thread()],
    };
    let text = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        serde_json::from_str::<IndexSnapshot>(&text).unwrap(),
        snapshot
    );
    insta::assert_json_snapshot!("index_snapshot", snapshot);
}
