//! Daemon → client frames other than successful responses: errors, topic events and `resync_required`.

use std::path::Path;

use omfp_core::{ProcessState, ThreadEvent};
use omfp_protocol::{
    ErrorBody, ErrorCode, EventFrame, IndexEvent, JobEvent, JobStream, MAX_FRAME_BYTES,
    PROTOCOL_VERSION, Response, ServerFrame, Topic, TopicEvent, TransientEvent, ViewsEvent,
    WidgetInfo, WidgetSource, WorkspaceEvent,
};
use serde_json::{Value, json};

use crate::samples::{THREAD_ID, WORKSPACE_ID, repo_statuses, settings, theme, thread};

fn round_trip(frame: &ServerFrame) -> Value {
    let text = serde_json::to_string(frame).expect("frame serializes");
    assert_eq!(
        &serde_json::from_str::<ServerFrame>(&text).expect("frame decodes"),
        frame
    );
    serde_json::from_str(&text).expect("valid JSON")
}

// spec: PRT-011
#[test]
fn error_codes_are_the_closed_set_of_the_spec() {
    let wire: Vec<Value> = ErrorCode::ALL
        .iter()
        .map(|code| serde_json::to_value(code).unwrap())
        .collect();
    assert_eq!(
        wire,
        [
            "unauthorized",
            "bad_request",
            "not_found",
            "conflict",
            "already_resolved",
            "limit_reached",
            "version_mismatch",
            "thread_failed",
            "omp_error",
            "git_error",
            "internal",
        ]
        .map(Value::from)
    );
    for code in ErrorCode::ALL {
        let text = serde_json::to_string(&code).unwrap();
        assert_eq!(serde_json::from_str::<ErrorCode>(&text).unwrap(), code);
    }
    assert!(serde_json::from_value::<ErrorCode>(json!("teapot")).is_err());
}

// spec: PRT-011, PRT-010
#[test]
fn omp_errors_carry_omps_code() {
    let frame = ServerFrame::Response(Response::err(
        "c42",
        ErrorBody::new(ErrorCode::OmpError, "agent is busy")
            .with_omp_code("session_busy")
            .with_details(json!({ "command": "navigate_tree" })),
    ));
    let golden = round_trip(&frame);
    insta::assert_json_snapshot!("error_response_omp_error", golden);
}

// spec: PRT-004
#[test]
fn version_mismatch_lists_the_supported_versions() {
    let frame = ServerFrame::Response(Response::err(
        "1",
        ErrorBody::version_mismatch(&[PROTOCOL_VERSION]),
    ));
    let golden = round_trip(&frame);
    assert_eq!(
        golden["error"]["details"]["supported"],
        json!([PROTOCOL_VERSION])
    );
    insta::assert_json_snapshot!("error_response_version_mismatch", golden);
}

#[test]
fn a_response_is_either_a_result_or_an_error() {
    let ok: ServerFrame =
        serde_json::from_value(json!({ "type": "response", "id": "c1", "ok": true, "result": {} }))
            .unwrap();
    assert_eq!(
        ok,
        ServerFrame::Response(Response::ok("c1", &json!({})).unwrap())
    );
    for invalid in [
        json!({ "type": "response", "id": "c1", "ok": false, "result": {} }),
        json!({ "type": "response", "id": "c1", "ok": true, "error": { "code": "internal", "message": "x" } }),
    ] {
        assert!(serde_json::from_value::<ServerFrame>(invalid).is_err());
    }
}

// spec: PRT-022
#[test]
fn thread_event_frame() {
    let frame = ServerFrame::Event(EventFrame {
        topic: Topic::Thread(THREAD_ID.to_owned()),
        epoch: "e7".to_owned(),
        seq: 1289,
        event: TopicEvent::Thread(ThreadEvent::Process {
            state: ProcessState::Running,
        }),
    });
    let golden = round_trip(&frame);
    insta::assert_json_snapshot!("event_thread", golden);
}

#[test]
fn index_event_frame() {
    let frame = ServerFrame::Event(EventFrame {
        topic: Topic::Index,
        epoch: "e1".to_owned(),
        seq: 3,
        event: TopicEvent::Index(IndexEvent::ThreadUpserted { thread: thread() }),
    });
    let golden = round_trip(&frame);
    insta::assert_json_snapshot!("event_index", golden);

    let removed = ServerFrame::Event(EventFrame {
        topic: Topic::Index,
        epoch: "e1".to_owned(),
        seq: 4,
        event: TopicEvent::Index(IndexEvent::WorkspaceRemoved {
            workspace_id: WORKSPACE_ID.to_owned(),
        }),
    });
    assert_eq!(
        round_trip(&removed)["event"],
        json!({ "kind": "workspace_removed", "workspaceId": WORKSPACE_ID })
    );
}

// spec: DMN-009, UI-097
#[test]
fn the_index_topic_carries_settings_changes() {
    let changed = ServerFrame::Event(EventFrame {
        topic: Topic::Index,
        epoch: "e1".to_owned(),
        seq: 6,
        event: TopicEvent::Index(IndexEvent::SettingsChanged {
            settings: settings(),
        }),
    });
    let event = &round_trip(&changed)["event"];
    assert_eq!(event["kind"], "settings_changed");
    assert_eq!(event["settings"]["keymap"][0]["keys"], "Space t");
}

// spec: DMN-075
#[test]
fn the_index_topic_carries_theme_changes() {
    let changed = ServerFrame::Event(EventFrame {
        topic: Topic::Index,
        epoch: "e1".to_owned(),
        seq: 5,
        event: TopicEvent::Index(IndexEvent::ThemeChanged { theme: theme() }),
    });
    assert_eq!(
        round_trip(&changed)["event"],
        json!({ "kind": "theme_changed", "theme": { "css": ":root { --omfp-primary: #6750a4; }" } })
    );
}

#[test]
fn workspace_event_frame() {
    let frame = ServerFrame::Event(EventFrame {
        topic: Topic::Workspace(WORKSPACE_ID.to_owned()),
        epoch: "e1".to_owned(),
        seq: 1,
        event: TopicEvent::Workspace(WorkspaceEvent::Status {
            repos: repo_statuses(),
        }),
    });
    let golden = round_trip(&frame);
    insta::assert_json_snapshot!("event_workspace", golden);
}

fn event(topic: Topic, event: TopicEvent) -> ServerFrame {
    ServerFrame::Event(EventFrame {
        topic,
        epoch: "e1".to_owned(),
        seq: 1,
        event,
    })
}

// spec: PRT-040
#[test]
fn views_event_frames() {
    let topic = Topic::Views(WORKSPACE_ID.to_owned());
    let manifest = json!({ "views": { "work": { "layout": "omfp:chat" } } });
    let changed = event(
        topic.clone(),
        TopicEvent::Views(ViewsEvent::ViewsManifest {
            manifest: Some(manifest.clone()),
            error: None,
        }),
    );
    assert_eq!(
        round_trip(&changed)["event"],
        json!({ "kind": "views_manifest", "manifest": manifest, "error": null })
    );
    let invalid = event(
        topic.clone(),
        TopicEvent::Views(ViewsEvent::ViewsManifest {
            manifest: None,
            error: Some("views.work.layout: expected a widget name".to_owned()),
        }),
    );
    assert_eq!(
        round_trip(&invalid)["event"],
        json!({ "kind": "views_manifest", "manifest": null, "error": "views.work.layout: expected a widget name" })
    );
    let widgets = event(
        topic.clone(),
        TopicEvent::Views(ViewsEvent::ViewsWidgets {
            widgets: vec![WidgetInfo {
                source: WidgetSource::Workspace,
                name: "tests".to_owned(),
                version: "1f2e3d4c5b6a".to_owned(),
                entry: Some("index.tsx".to_owned()),
            }],
        }),
    );
    insta::assert_json_snapshot!("event_views_widgets", round_trip(&widgets));
    let stored = event(
        topic,
        TopicEvent::Views(ViewsEvent::WidgetStored {
            instance: "./widgets/tests".to_owned(),
            key: "runs".to_owned(),
            value: json!(3),
        }),
    );
    assert_eq!(
        round_trip(&stored)["event"],
        json!({ "kind": "widget_stored", "instance": "./widgets/tests", "key": "runs", "value": 3 })
    );
}

// spec: PRT-041
#[test]
fn job_event_frames() {
    let topic = Topic::Job("01JOB".to_owned());
    let line = event(
        topic.clone(),
        TopicEvent::Job(JobEvent::JobOutput {
            stream: JobStream::Stderr,
            text: "PASS a".to_owned(),
        }),
    );
    assert_eq!(
        round_trip(&line)["event"],
        json!({ "kind": "job_output", "stream": "stderr", "text": "PASS a" })
    );
    let exited = event(
        topic,
        TopicEvent::Job(JobEvent::JobExited {
            code: None,
            signal: Some(15),
            ended_at: 1_700_000_000_000,
        }),
    );
    assert_eq!(
        round_trip(&exited)["event"],
        json!({ "kind": "job_exited", "code": null, "signal": 15, "endedAt": 1_700_000_000_000_i64 })
    );
}

#[test]
fn event_must_match_its_topic() {
    let index_event_on_thread_topic = json!({
        "type": "event",
        "topic": format!("thread/{THREAD_ID}"),
        "epoch": "e1",
        "seq": 1,
        "event": { "kind": "thread_removed", "threadId": THREAD_ID }
    });
    assert!(serde_json::from_value::<ServerFrame>(index_event_on_thread_topic).is_err());
}

// spec: PRT-023
#[test]
fn resync_required_frame() {
    let frame = ServerFrame::ResyncRequired {
        topic: Topic::Thread(THREAD_ID.to_owned()),
    };
    let golden = round_trip(&frame);
    assert_eq!(
        golden,
        json!({ "type": "resync_required", "topic": format!("thread/{THREAD_ID}") })
    );
    insta::assert_json_snapshot!("resync_required", golden);
}

// spec: PRT-054, DMN-110
#[test]
fn transient_frame_carries_live_levels_without_a_position() {
    let frame = ServerFrame::Transient {
        topic: Topic::Thread(THREAD_ID.to_owned()),
        event: TransientEvent::LiveLevels {
            input: 0.25,
            output: 0.5,
        },
    };
    let golden = round_trip(&frame);
    assert_eq!(
        golden,
        json!({
            "type": "transient",
            "topic": format!("thread/{THREAD_ID}"),
            "event": { "kind": "live_levels", "input": 0.25, "output": 0.5 }
        })
    );
    insta::assert_json_snapshot!("transient_live_levels", golden);
}

// spec: TST-012
#[test]
fn every_recorded_thread_event_survives_an_event_frame() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/threads/18.18.6");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixture directory")
        .map(|entry| entry.expect("fixture entry").path())
        .collect();
    files.sort();
    assert_ne!(files, Vec::<std::path::PathBuf>::new());

    for file in files {
        let text = std::fs::read_to_string(&file).expect("fixture file");
        for (index, line) in text.lines().enumerate() {
            let event: ThreadEvent =
                serde_json::from_str(line).expect("fixture line is a ThreadEvent");
            let frame = ServerFrame::Event(EventFrame {
                topic: Topic::Thread(THREAD_ID.to_owned()),
                epoch: "e1".to_owned(),
                seq: u64::try_from(index).unwrap() + 1,
                event: TopicEvent::Thread(event),
            });
            // Compared as decoded values: omp-rpc re-encodes some integers as floats (`0` → `0.0`).
            round_trip(&frame);
        }
    }
}

// spec: PRT-013
#[test]
fn frames_are_capped_at_16_mib() {
    assert_eq!(MAX_FRAME_BYTES, 16 * 1024 * 1024);
}
