//! Tools the daemon provides to omp (DMN-060..064), registered at launch through `omp-rpc`'s
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
use crate::widgets::layout::{Addition, Place};

/// Name of the tool that opens a file for the user.
pub const OPEN_IN_EDITOR: &str = "open_in_editor";

/// Name of the tool that renames the agent's thread (DMN-073).
pub const SET_THREAD_TITLE: &str = "set_thread_title";

/// Name of the tool that checks a widget (DMN-063).
pub const WIDGET_CHECK: &str = "omfp_widget_check";

/// Name of the tool that adds a widget to a view (DMN-064).
pub const VIEW_ADD: &str = "omfp_view_add";

/// Longest title `set_thread_title` accepts, in characters.
pub const MAX_TITLE_CHARS: usize = 200;

/// Receives the events a host tool adds to its thread.
pub type Emit = Arc<dyn Fn(ThreadEvent) + Send + Sync>;

/// Stores a title the agent set for its thread; the error goes back to the agent.
pub type SetTitle = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// What the widget tools need from the daemon (DMN-063, DMN-064); the server implements it. Both run on an
/// `omp-rpc` handler thread and may block it.
pub trait WidgetHost: Send + Sync {
    /// The `omfp_widget_check` answer for the widget `name` of workspace `workspace_id`.
    fn check_widget(
        &self,
        workspace_id: &str,
        name: &str,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<String, String>;
    /// Adds a widget to a view of workspace `workspace_id`, writes `views.json` and says what it did.
    fn add_to_view(&self, workspace_id: &str, addition: &Addition<'_>) -> Result<String, String>;
}

/// The host tools of one thread: `cwd` (omp's working directory, the thread's workspace folder, DMN-030) resolves
/// relative paths, `emit` delivers the presentation events to the thread's clients, `set_title` renames the thread,
/// and `widgets` (absent in tests that need no widget tools) backs the widget tools of workspace `workspace_id`.
#[must_use]
pub fn thread_tools(
    cwd: PathBuf,
    emit: Emit,
    set_title: SetTitle,
    workspace_id: &str,
    widgets: Option<Arc<dyn WidgetHost>>,
) -> Vec<HostTool> {
    let mut tools = vec![open_in_editor(cwd, emit), set_thread_title(set_title)];
    if let Some(host) = widgets {
        tools.push(widget_check(workspace_id.to_owned(), Arc::clone(&host)));
        tools.push(view_add(workspace_id.to_owned(), host));
    }
    tools
}

fn schema(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// `omfp_widget_check { name }` (DMN-063).
#[must_use]
pub fn widget_check(workspace_id: String, host: Arc<dyn WidgetHost>) -> HostTool {
    let parameters = schema(&json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "minLength": 1,
                "description": "The widget: its folder name (`tests` for .omfp/widgets/tests/), `./widgets/<name>` or `user:<name>`."
            }
        },
        "required": ["name"],
        "additionalProperties": false
    }));
    HostTool::new(
        WIDGET_CHECK,
        "Checks a workspace or user widget without a browser: the daemon compiles its files, then reports what every \
         open omfp window shows of it (render, import and console errors, the rendered text, its jobs) or that no \
         window shows it. Use it after writing or changing a widget.",
        parameters,
        move |arguments, context| {
            run_widget_check(&arguments, || context.is_cancelled(), &workspace_id, &*host)
        },
    )
    .label("Check widget")
}

/// The body of [`widget_check`], without the `omp-rpc` context (DMN-061).
pub fn run_widget_check(
    arguments: &Map<String, Value>,
    is_cancelled: impl Fn() -> bool,
    workspace_id: &str,
    host: &dyn WidgetHost,
) -> Result<HostToolOutput, HostToolError> {
    if is_cancelled() {
        return Err("cancelled".into());
    }
    let name = match arguments.get("name") {
        Some(Value::String(name)) if !name.trim().is_empty() => name.trim(),
        _ => return Err("`name` must be a non-empty string".into()),
    };
    Ok(HostToolOutput::Text(host.check_widget(
        workspace_id,
        name,
        &is_cancelled,
    )?))
}

/// `omfp_view_add { widget, props?, view?, where? }` (DMN-064).
#[must_use]
pub fn view_add(workspace_id: String, host: Arc<dyn WidgetHost>) -> HostTool {
    let parameters = schema(&json!({
        "type": "object",
        "properties": {
            "widget": {
                "type": "string",
                "minLength": 1,
                "description": "The widget reference: `./widgets/<name>`, `user:<name>`, `omfp:<name>`; a bare name means `./widgets/<name>`."
            },
            "props": { "type": "object", "description": "The widget's props (an `instance` string tells two copies apart)." },
            "view": { "type": "string", "minLength": 1, "description": "The view to change (default: the default view); a new id creates a view showing only the widget." },
            "where": {
                "type": "string",
                "enum": ["side-tab", "main-tab", "end"],
                "description": "side-tab (default): a tab next to the workspace and changes widgets; main-tab: a tab next to the conversation; end: the end of the view's root container."
            }
        },
        "required": ["widget"],
        "additionalProperties": false
    }));
    HostTool::new(
        VIEW_ADD,
        "Adds a widget to a view of this workspace's .omfp/views.json (open windows show it at once), without \
         rewriting the file by hand. Keep the main slot, the [\"tabs\", {\"id\": \"main\"}, …\"omfp:chat\"] container: \
         files the user opens become its tabs (without it they float over the view), so never remove or replace it.",
        parameters,
        move |arguments, context| {
            run_view_add(&arguments, || context.is_cancelled(), &workspace_id, &*host)
        },
    )
    .label("Add widget to view")
}

/// The body of [`view_add`], without the `omp-rpc` context (DMN-061).
pub fn run_view_add(
    arguments: &Map<String, Value>,
    is_cancelled: impl Fn() -> bool,
    workspace_id: &str,
    host: &dyn WidgetHost,
) -> Result<HostToolOutput, HostToolError> {
    if is_cancelled() {
        return Err("cancelled".into());
    }
    let widget = match arguments.get("widget") {
        Some(Value::String(widget)) if !widget.trim().is_empty() => widget.trim(),
        _ => return Err("`widget` must be a non-empty string".into()),
    };
    let reference = if widget.starts_with("./widgets/")
        || widget.starts_with("user:")
        || widget.starts_with("omfp:")
    {
        widget.to_owned()
    } else {
        format!("./widgets/{widget}")
    };
    let props = match arguments.get("props") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(props)) => props.clone(),
        Some(_) => return Err("`props` must be an object".into()),
    };
    let view = match arguments.get("view") {
        None | Some(Value::Null) => None,
        Some(Value::String(view)) if !view.trim().is_empty() => Some(view.trim()),
        Some(_) => return Err("`view` must be a non-empty string".into()),
    };
    let place = match arguments.get("where") {
        None | Some(Value::Null) => Place::SideTab,
        Some(Value::String(place)) => {
            Place::parse(place).ok_or("`where` must be \"side-tab\", \"main-tab\" or \"end\"")?
        }
        Some(_) => return Err("`where` must be \"side-tab\", \"main-tab\" or \"end\"".into()),
    };
    if is_cancelled() {
        return Err("cancelled".into());
    }
    let addition = Addition {
        reference: &reference,
        props,
        view,
        place,
    };
    Ok(HostToolOutput::Text(
        host.add_to_view(workspace_id, &addition)?,
    ))
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
        let tools = thread_tools(
            "/home".into(),
            Arc::new(|_| {}),
            Arc::new(|_| Ok(())),
            "01WS",
            None,
        );
        let definitions: Vec<_> = tools.iter().map(HostTool::definition).collect();

        assert_eq!(definitions[0].name, OPEN_IN_EDITOR);
        assert_eq!(definitions[0].parameters["required"], json!(["path"]));
        assert_eq!(definitions[1].name, SET_THREAD_TITLE);
        assert_eq!(definitions[1].parameters["required"], json!(["title"]));
        assert_eq!(
            definitions.len(),
            2,
            "no widget tools without a widget host"
        );
    }

    /// A widget host that records what the tools asked for.
    #[derive(Default)]
    struct FakeWidgets {
        asked: Mutex<Vec<String>>,
    }

    impl WidgetHost for FakeWidgets {
        fn check_widget(
            &self,
            workspace_id: &str,
            name: &str,
            _is_cancelled: &dyn Fn() -> bool,
        ) -> Result<String, String> {
            self.asked
                .lock()
                .unwrap()
                .push(format!("check {workspace_id} {name}"));
            if name == "missing" {
                return Err("no widget missing".to_owned());
            }
            Ok(format!("checked {name}"))
        }

        fn add_to_view(
            &self,
            workspace_id: &str,
            addition: &Addition<'_>,
        ) -> Result<String, String> {
            self.asked.lock().unwrap().push(format!(
                "add {workspace_id} {} {} {:?} {:?}",
                addition.reference,
                Value::Object(addition.props.clone()),
                addition.view,
                addition.place
            ));
            Ok("added".to_owned())
        }
    }

    fn text(output: Result<HostToolOutput, HostToolError>) -> Result<String, String> {
        match output {
            Ok(HostToolOutput::Text(text)) => Ok(text),
            Ok(other) => panic!("unexpected output {other:?}"),
            Err(error) => Err(error.to_string()),
        }
    }

    // spec: DMN-060, DMN-063, DMN-064
    #[test]
    fn the_widget_tools_are_registered_and_reach_the_widget_host_of_the_threads_workspace() {
        let host = Arc::new(FakeWidgets::default());
        let tools = thread_tools(
            "/home".into(),
            Arc::new(|_| {}),
            Arc::new(|_| Ok(())),
            "01WS",
            Some(Arc::clone(&host) as Arc<dyn WidgetHost>),
        );
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool.definition().name.as_str())
            .collect();
        assert_eq!(
            names,
            [OPEN_IN_EDITOR, SET_THREAD_TITLE, WIDGET_CHECK, VIEW_ADD]
        );
        assert_eq!(
            tools[2].definition().parameters["required"],
            json!(["name"])
        );
        assert_eq!(
            tools[3].definition().parameters["properties"]["where"]["enum"],
            json!(["side-tab", "main-tab", "end"])
        );

        let check =
            |arguments: Value| text(run_widget_check(&args(arguments), || false, "01WS", &*host));
        assert_eq!(
            check(json!({ "name": " tests " })),
            Ok("checked tests".to_owned())
        );
        assert_eq!(
            check(json!({ "name": "missing" })),
            Err("no widget missing".to_owned())
        );

        let add = |arguments: Value| text(run_view_add(&args(arguments), || false, "01WS", &*host));
        assert_eq!(add(json!({ "widget": "tests" })), Ok("added".to_owned()));
        assert_eq!(
            add(
                json!({ "widget": "user:clock", "props": { "zone": "UTC" }, "view": "dash", "where": "end" })
            ),
            Ok("added".to_owned())
        );
        assert_eq!(
            *host.asked.lock().unwrap(),
            [
                "check 01WS tests",
                "check 01WS missing",
                "add 01WS ./widgets/tests {} None SideTab",
                "add 01WS user:clock {\"zone\":\"UTC\"} Some(\"dash\") End",
            ]
        );
    }

    // spec: DMN-061
    #[test]
    fn the_widget_tools_refuse_bad_arguments_and_honour_cancellation() {
        let host = FakeWidgets::default();
        for arguments in [json!({}), json!({ "name": "" }), json!({ "name": 3 })] {
            assert!(
                text(run_widget_check(
                    &args(arguments.clone()),
                    || false,
                    "w",
                    &host
                ))
                .is_err(),
                "{arguments}"
            );
        }
        for arguments in [
            json!({}),
            json!({ "widget": " " }),
            json!({ "widget": "t", "props": [] }),
            json!({ "widget": "t", "view": "" }),
            json!({ "widget": "t", "where": "left" }),
        ] {
            assert!(
                text(run_view_add(&args(arguments.clone()), || false, "w", &host)).is_err(),
                "{arguments}"
            );
        }
        assert_eq!(
            text(run_widget_check(
                &args(json!({ "name": "t" })),
                || true,
                "w",
                &host
            )),
            Err("cancelled".to_owned())
        );
        assert_eq!(
            text(run_view_add(
                &args(json!({ "widget": "t" })),
                || true,
                "w",
                &host
            )),
            Err("cancelled".to_owned())
        );
        assert_eq!(*host.asked.lock().unwrap(), Vec::<String>::new());
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
