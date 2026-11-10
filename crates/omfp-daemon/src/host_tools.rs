//! Tools the daemon provides to omp (DMN-060, DMN-061), registered at launch through `omp-rpc`'s
//! `ClientOptions::tools`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use omfp_core::ThreadEvent;
use omp_rpc::{
    ExtensionUiRequest, HostTool, HostToolError, HostToolOutput, OpenUrlUiRequest, RpcNotification,
};
use serde_json::{Map, Value, json};

use crate::store::new_id;

/// Name of the tool that opens a file for the user.
pub const OPEN_IN_EDITOR: &str = "open_in_editor";

/// Name of the tool that renames the agent's thread (DMN-073).
pub const SET_THREAD_TITLE: &str = "set_thread_title";

/// Longest title `set_thread_title` accepts, in characters.
pub const MAX_TITLE_CHARS: usize = 200;

/// Receives the events a host tool adds to its thread.
pub type Emit = Arc<dyn Fn(ThreadEvent) + Send + Sync>;

/// Stores a title the agent set for its thread; the error goes back to the agent.
pub type SetTitle = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// The host tools of one thread: `cwd` (omp's working directory, the thread's workspace folder, DMN-030) resolves
/// relative paths, `emit` delivers the presentation events to the thread's clients and `set_title` renames the thread.
#[must_use]
pub fn thread_tools(cwd: PathBuf, emit: Emit, set_title: SetTitle) -> Vec<HostTool> {
    vec![open_in_editor(cwd, emit), set_thread_title(set_title)]
}

/// `set_thread_title { title }`: renames the thread; a title set this way is never replaced by a generated one
/// (DMN-073).
#[must_use]
pub fn set_thread_title(set_title: SetTitle) -> HostTool {
    let parameters = json!({
        "type": "object",
        "properties": {
            "title": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_TITLE_CHARS,
                "description": "The new title: a few words saying what the thread is about."
            }
        },
        "required": ["title"],
        "additionalProperties": false
    })
    .as_object()
    .cloned()
    .unwrap_or_default();
    HostTool::new(
        SET_THREAD_TITLE,
        "Renames this thread in the omfp thread list, e.g. when the user asks for another title.",
        parameters,
        move |arguments, context| {
            run_set_thread_title(&arguments, || context.is_cancelled(), &*set_title)
        },
    )
    .label("Set thread title")
}

/// The body of [`set_thread_title`], without the `omp-rpc` context. Bad arguments and cancellation become tool errors
/// (`isError: true`), never panics (DMN-061).
pub fn run_set_thread_title(
    arguments: &Map<String, Value>,
    is_cancelled: impl Fn() -> bool,
    set_title: &dyn Fn(&str) -> Result<(), String>,
) -> Result<HostToolOutput, HostToolError> {
    if is_cancelled() {
        return Err("cancelled".into());
    }
    let title = match arguments.get("title") {
        Some(Value::String(title)) if !title.trim().is_empty() => title.trim(),
        _ => return Err("`title` must be a non-empty string".into()),
    };
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("`title` must be at most {MAX_TITLE_CHARS} characters").into());
    }
    if is_cancelled() {
        return Err("cancelled".into());
    }
    set_title(title)?;
    Ok(HostToolOutput::Text(format!(
        "The thread is now titled \"{title}\"."
    )))
}

/// `open_in_editor { path, line? }`: asks the user's UI to open a file (an `open_url` presentation event with a
/// `file://` URL, `#L<line>` when a line is given).
#[must_use]
pub fn open_in_editor(cwd: PathBuf, emit: Emit) -> HostTool {
    let parameters = json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "File to open; relative paths start at the working directory."
            },
            "line": { "type": "integer", "minimum": 1, "description": "Line to show." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
    .as_object()
    .cloned()
    .unwrap_or_default();
    HostTool::new(
        OPEN_IN_EDITOR,
        "Opens a file in the user's editor so they can look at it or edit it.",
        parameters,
        move |arguments, context| {
            run_open_in_editor(&arguments, || context.is_cancelled(), &cwd, &*emit)
        },
    )
    .label("Open in editor")
}

/// The body of [`open_in_editor`], without the `omp-rpc` context. Bad arguments and cancellation become tool errors
/// (`isError: true`), never panics (DMN-061).
pub fn run_open_in_editor(
    arguments: &Map<String, Value>,
    is_cancelled: impl Fn() -> bool,
    cwd: &Path,
    emit: &dyn Fn(ThreadEvent),
) -> Result<HostToolOutput, HostToolError> {
    if is_cancelled() {
        return Err("cancelled".into());
    }
    let path = match arguments.get("path") {
        Some(Value::String(path)) if !path.trim().is_empty() => Path::new(path),
        _ => return Err("`path` must be a non-empty string".into()),
    };
    let line = match arguments.get("line") {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_u64().filter(|line| *line >= 1) {
            Some(line) => Some(line),
            None => return Err("`line` must be a positive integer".into()),
        },
    };
    let path = cwd.join(path);
    let mut url = file_url(&path);
    let mut shown = path.display().to_string();
    if let Some(line) = line {
        let _ = write!(url, "#L{line}");
        let _ = write!(shown, ":{line}");
    }
    if is_cancelled() {
        return Err("cancelled".into());
    }
    emit(ThreadEvent::Omp {
        frame: RpcNotification::ExtensionUiRequest(ExtensionUiRequest::OpenUrl(OpenUrlUiRequest {
            id: new_id(),
            url,
            launch_url: None,
            instructions: Some(format!("Open {shown} in the editor")),
        })),
    });
    Ok(HostToolOutput::Text(format!(
        "Asked the user's editor to open {shown}."
    )))
}

/// `file://` URL of an absolute path, percent-encoding everything but unreserved characters and `/`.
fn file_url(path: &Path) -> String {
    let mut url = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            url.push(char::from(byte));
        } else {
            let _ = write!(url, "%{byte:02X}");
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn args(value: Value) -> Map<String, Value> {
        let Value::Object(map) = value else {
            panic!("not an object")
        };
        map
    }

    fn run(arguments: Value, cancelled: bool) -> (Result<String, String>, Vec<ThreadEvent>) {
        let events = Mutex::new(Vec::new());
        let result = run_open_in_editor(
            &args(arguments),
            || cancelled,
            Path::new("/ws/checkout"),
            &|event| events.lock().unwrap().push(event),
        );
        let result = match result {
            Ok(HostToolOutput::Text(text)) => Ok(text),
            Ok(other) => panic!("unexpected output {other:?}"),
            Err(error) => Err(error.to_string()),
        };
        (result, events.into_inner().unwrap())
    }

    fn opened_url(events: &[ThreadEvent]) -> String {
        match events {
            [
                ThreadEvent::Omp {
                    frame: RpcNotification::ExtensionUiRequest(ExtensionUiRequest::OpenUrl(open)),
                },
            ] => open.url.clone(),
            other => panic!("expected one open_url event, got {other:?}"),
        }
    }

    // spec: DMN-060
    #[test]
    fn asks_the_ui_to_open_the_file_at_a_line() {
        let (result, events) = run(json!({ "path": "/repo/src/main.rs", "line": 12 }), false);

        assert_eq!(
            result.unwrap(),
            "Asked the user's editor to open /repo/src/main.rs:12."
        );
        assert_eq!(opened_url(&events), "file:///repo/src/main.rs#L12");
    }

    // spec: DMN-060
    #[test]
    fn relative_paths_start_at_the_workspace_folder_and_are_url_encoded() {
        let (result, events) = run(json!({ "path": "notes/a b.md" }), false);

        assert!(result.is_ok());
        assert_eq!(opened_url(&events), "file:///ws/checkout/notes/a%20b.md");
    }

    // spec: DMN-061
    #[test]
    fn bad_arguments_are_tool_errors_not_panics() {
        for arguments in [
            json!({}),
            json!({ "path": "" }),
            json!({ "path": 3 }),
            json!({ "path": "a.rs", "line": 0 }),
            json!({ "path": "a.rs", "line": "x" }),
        ] {
            let (result, events) = run(arguments.clone(), false);
            assert!(result.is_err(), "{arguments}");
            assert_eq!(events, Vec::new());
        }
    }

    // spec: DMN-061
    #[test]
    fn a_cancelled_call_does_nothing_and_reports_an_error() {
        let (result, events) = run(json!({ "path": "/repo/a.rs" }), true);

        assert_eq!(result, Err("cancelled".to_owned()));
        assert_eq!(events, Vec::new());
    }

    // spec: DMN-060
    #[test]
    fn the_tools_are_registered_with_their_schemas() {
        let tools = thread_tools("/home".into(), Arc::new(|_| {}), Arc::new(|_| Ok(())));
        let definitions: Vec<_> = tools.iter().map(HostTool::definition).collect();

        assert_eq!(definitions[0].name, OPEN_IN_EDITOR);
        assert_eq!(definitions[0].parameters["required"], json!(["path"]));
        assert_eq!(definitions[1].name, SET_THREAD_TITLE);
        assert_eq!(definitions[1].parameters["required"], json!(["title"]));
    }

    fn rename(arguments: Value, cancelled: bool) -> (Result<String, String>, Vec<String>) {
        let titles = Mutex::new(Vec::new());
        let result = run_set_thread_title(&args(arguments), || cancelled, &|title| {
            titles.lock().unwrap().push(title.to_owned());
            Ok(())
        });
        let result = match result {
            Ok(HostToolOutput::Text(text)) => Ok(text),
            Ok(other) => panic!("unexpected output {other:?}"),
            Err(error) => Err(error.to_string()),
        };
        (result, titles.into_inner().unwrap())
    }

    // spec: DMN-060, DMN-073
    #[test]
    fn set_thread_title_renames_the_thread() {
        let (result, titles) = rename(json!({ "title": "  Checkout v2 " }), false);

        assert_eq!(result.unwrap(), "The thread is now titled \"Checkout v2\".");
        assert_eq!(titles, ["Checkout v2"]);
    }

    // spec: DMN-061
    #[test]
    fn set_thread_title_rejects_bad_titles_and_honours_cancellation() {
        for arguments in [
            json!({}),
            json!({ "title": " " }),
            json!({ "title": 3 }),
            json!({ "title": "x".repeat(MAX_TITLE_CHARS + 1) }),
        ] {
            let (result, titles) = rename(arguments.clone(), false);
            assert!(result.is_err(), "{arguments}");
            assert_eq!(titles, Vec::<String>::new());
        }
        let (result, titles) = rename(json!({ "title": "Late" }), true);
        assert_eq!(result, Err("cancelled".to_owned()));
        assert_eq!(titles, Vec::<String>::new());

        let failed = run_set_thread_title(&args(json!({ "title": "x" })), || false, &|_| {
            Err("thread 01X not found".into())
        });
        assert_eq!(failed.unwrap_err().to_string(), "thread 01X not found");
    }
}
