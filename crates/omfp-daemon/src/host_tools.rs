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

/// Receives the events a host tool adds to its thread.
pub type Emit = Arc<dyn Fn(ThreadEvent) + Send + Sync>;

/// The host tools of one thread: `home` resolves relative paths (it is omp's cwd, DMN-028) and `emit` delivers the
/// presentation events to the thread's clients.
#[must_use]
pub fn thread_tools(home: PathBuf, emit: Emit) -> Vec<HostTool> {
    vec![open_in_editor(home, emit)]
}

/// `open_in_editor { path, line? }`: asks the user's UI to open a file (an `open_url` presentation event with a
/// `file://` URL, `#L<line>` when a line is given).
#[must_use]
pub fn open_in_editor(home: PathBuf, emit: Emit) -> HostTool {
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
            run_open_in_editor(&arguments, || context.is_cancelled(), &home, &*emit)
        },
    )
    .label("Open in editor")
}

/// The body of [`open_in_editor`], without the `omp-rpc` context. Bad arguments and cancellation become tool errors
/// (`isError: true`), never panics (DMN-061).
pub fn run_open_in_editor(
    arguments: &Map<String, Value>,
    is_cancelled: impl Fn() -> bool,
    home: &Path,
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
    let path = home.join(path);
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
            Path::new("/data/threads/t1/home"),
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
    fn relative_paths_start_at_the_thread_home_and_are_url_encoded() {
        let (result, events) = run(json!({ "path": "notes/a b.md" }), false);

        assert!(result.is_ok());
        assert_eq!(
            opened_url(&events),
            "file:///data/threads/t1/home/notes/a%20b.md"
        );
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
    fn the_tool_is_registered_with_its_schema() {
        let tools = thread_tools("/home".into(), Arc::new(|_| {}));
        let definition = tools[0].definition();

        assert_eq!(definition.name, OPEN_IN_EDITOR);
        assert_eq!(definition.parameters["required"], json!(["path"]));
    }
}
