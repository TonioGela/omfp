//! Contract tests (L3) of omp's behavior when its cwd is a workspace folder: a plain directory (not a git repo)
//! whose entries are repositories, one real clone and one symlink to a repository elsewhere. They pin what the
//! pinned omp puts in the system prompt (context files, workspace roots, appended instructions) and how its tools
//! treat paths through the symlink, so an omp upgrade that changes any of it fails here.

mod common;

use std::path::{Path, PathBuf};

use common::{Harness, TIMEOUT, prompt, start};
use fake_llm::{Reply, Step, ToolCall};
use omfp_daemon::omp::LaunchSpec;
use omfp_testkit::{OmpSession, TempRepo};
use serde_json::{Value, json};

const BACKEND_MARKER: &str = "Backend marker: rutabaga-42";
const FRONTEND_MARKER: &str = "Frontend marker: kumquat-17";

/// `ws/` (the omp cwd, not a git repo) with `ws/backend` a real repository and `ws/frontend` a symlink to
/// `external`, a repository outside `ws`. Both repositories have an `AGENTS.md` with a unique marker.
struct Workspace {
    ws: PathBuf,
    external: TempRepo,
}

impl Workspace {
    fn new(harness: &Harness) -> Self {
        let ws = harness.thread.path().join("home");
        std::fs::create_dir_all(&ws).unwrap();
        let backend = TempRepo::new().unwrap();
        backend
            .commit_file("AGENTS.md", &format!("{BACKEND_MARKER}\n"), "agents")
            .unwrap();
        // The temp dir moves into the workspace; its own cleanup then finds nothing, the thread dir removes it.
        std::fs::rename(backend.path(), ws.join("backend")).unwrap();
        let external = TempRepo::new().unwrap();
        external
            .commit_file("AGENTS.md", &format!("{FRONTEND_MARKER}\n"), "agents")
            .unwrap();
        external
            .commit_file("notes.txt", "original note\n", "notes")
            .unwrap();
        std::os::unix::fs::symlink(external.path(), ws.join("frontend")).unwrap();
        Self { ws, external }
    }

    fn backend(&self) -> PathBuf {
        self.ws.join("backend")
    }

    fn frontend(&self) -> PathBuf {
        self.ws.join("frontend")
    }

    fn external(&self) -> String {
        self.external.path().display().to_string()
    }
}

/// The system prompt of a recorded chat-completions request.
fn system_prompt(request: &Value) -> String {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "system" || message["role"] == "developer")
        .map(|message| match &message["content"] {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .collect()
}

/// The `<workspace-roots>` list of a system prompt, one path per entry.
fn workspace_roots(prompt: &str) -> Vec<String> {
    let Some(start) = prompt.find("<workspace-roots>") else {
        return Vec::new();
    };
    let end = prompt[start..].find("</workspace-roots>").unwrap() + start;
    prompt[start..end]
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .map(str::to_owned)
        .collect()
}

/// The text content of the tool result answering `call_id`.
fn tool_result(request: &Value, call_id: &str) -> String {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "tool" && message["tool_call_id"] == call_id)
        .map_or_else(
            || panic!("no result for {call_id}: {request:#}"),
            |message| message["content"].as_str().unwrap_or_default().to_owned(),
        )
}

fn path(dir: &Path) -> String {
    dir.display().to_string()
}

/// Runs one `hello` turn on `spec` and returns the system prompt the model saw.
async fn first_prompt(harness: &Harness, spec: LaunchSpec) -> String {
    tokio::task::spawn_blocking(move || {
        let (process, _) = start(&spec);
        process
            .client()
            .prompt_and_wait(&prompt("hello"), TIMEOUT)
            .unwrap();
    })
    .await
    .unwrap_or_else(|_| panic!("stderr:\n{}", harness.stderr()));
    system_prompt(&harness.fake.requests()[0])
}

#[tokio::test(flavor = "multi_thread")]
async fn without_add_dir_a_non_git_cwd_loads_no_nested_repository_context() {
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let _workspace = Workspace::new(&harness);

    let system = first_prompt(&harness, harness.spec(&[])).await;

    assert!(!system.contains(BACKEND_MARKER), "{system}");
    assert!(!system.contains(FRONTEND_MARKER), "{system}");
    for block in [
        "<repo-rules>",
        "<workspace-roots>",
        "<workspace-tree>",
        "backend",
        "frontend",
    ] {
        assert!(
            !system.contains(block),
            "{block} in the prompt of a bare workspace folder"
        );
    }
    assert!(
        system.contains("<project-context>\n<workstation>"),
        "{system}"
    );
    assert_eq!(harness.stderr(), "", "no warning for a cwd outside git");
    harness.assert_no_stray_network();
}

// spec: DMN-030
#[tokio::test(flavor = "multi_thread")]
async fn add_dir_of_each_entry_loads_its_agents_file_under_the_symlink_path() {
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let workspace = Workspace::new(&harness);

    let system = first_prompt(
        &harness,
        harness.spec(&[&workspace.backend(), &workspace.frontend()]),
    )
    .await;

    let frontend_agents = format!(
        "<file path=\"{}/AGENTS.md\">\n{FRONTEND_MARKER}\n</file>",
        path(&workspace.frontend())
    );
    let backend_agents = format!(
        "<file path=\"{}/AGENTS.md\">\n{BACKEND_MARKER}\n</file>",
        path(&workspace.backend())
    );
    assert!(system.contains(&backend_agents), "{system}");
    assert!(system.contains(&frontend_agents), "{system}");
    assert_eq!(
        workspace_roots(&system),
        [path(&workspace.backend()), path(&workspace.frontend())],
        "roots keep the given (symlink) paths, in order"
    );
    assert!(
        !system.contains(&workspace.external()),
        "omp does not resolve the symlink: {system}"
    );
    assert_eq!(harness.stderr(), "");
    harness.assert_no_stray_network();
}

// spec: DMN-029
#[tokio::test(flavor = "multi_thread")]
async fn live_add_dir_of_a_symlink_loads_its_agents_file_on_the_next_turn() {
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let workspace = Workspace::new(&harness);
    let spec = harness.spec(&[]);
    let frontend = workspace.frontend();

    let ack = tokio::task::spawn_blocking(move || {
        let (process, _) = start(&spec);
        let ack = process
            .client()
            .call(&prompt(&format!("/add-dir {}", frontend.display())))
            .unwrap();
        process
            .client()
            .prompt_and_wait(&prompt("hello"), TIMEOUT)
            .unwrap();
        ack
    })
    .await
    .unwrap();

    assert_eq!(ack.agent_invoked, Some(false));
    let system = system_prompt(&harness.fake.requests()[0]);
    assert!(system.contains(FRONTEND_MARKER), "{system}");
    assert!(!system.contains(BACKEND_MARKER), "{system}");
    assert_eq!(workspace_roots(&system), [path(&workspace.frontend())]);
    assert!(!system.contains(&workspace.external()), "{system}");
    harness.assert_no_stray_network();
}

// spec: DMN-030
#[tokio::test(flavor = "multi_thread")]
async fn restart_with_fewer_add_dirs_keeps_the_roots_saved_in_the_session() {
    let harness = Harness::new(vec![
        Step::reply(Reply::text("ok")),
        Step::reply(Reply::text("ok")),
    ])
    .await;
    let workspace = Workspace::new(&harness);
    let both = harness.spec(&[&workspace.backend(), &workspace.frontend()]);
    let backend_only = harness.spec(&[&workspace.backend()]);

    tokio::task::spawn_blocking(move || {
        for spec in [both, backend_only] {
            let (process, _) = start(&spec);
            process
                .client()
                .prompt_and_wait(&prompt("hello"), TIMEOUT)
                .unwrap();
        }
    })
    .await
    .unwrap();

    let system = system_prompt(&harness.fake.requests()[1]);
    assert_eq!(
        workspace_roots(&system),
        [path(&workspace.backend()), path(&workspace.frontend())],
        "`--continue` merges the session's saved roots with `--add-dir`: dropping a flag does not detach"
    );
    assert!(system.contains(FRONTEND_MARKER), "{system}");
    harness.assert_no_stray_network();
}

#[tokio::test(flavor = "multi_thread")]
async fn tools_follow_the_symlink_relative_to_the_workspace_folder() {
    let harness = Harness::new(vec![
        Step::reply(Reply::tool_calls(vec![
            ToolCall::new("c1", "read", json!({ "path": "frontend/notes.txt" })),
            ToolCall::new(
                "c2",
                "write",
                json!({ "path": "frontend/notes.txt", "content": "written through link\n" }),
            ),
            ToolCall::new(
                "c3",
                "bash",
                json!({ "command": "cat frontend/notes.txt && ls frontend" }),
            ),
        ])),
        Step::reply(Reply::text("done")),
    ])
    .await;
    let workspace = Workspace::new(&harness);
    let mut spec = harness.spec(&[]);
    spec.approval_mode = Some("yolo".into());

    first_prompt(&harness, spec).await;

    let answer = &harness.fake.requests()[1];
    assert!(
        tool_result(answer, "c1").ends_with("\n1:original note"),
        "{answer:#}"
    );
    assert!(
        tool_result(answer, "c2").contains("Successfully wrote 21 bytes to frontend/notes.txt"),
        "{answer:#}"
    );
    assert_eq!(
        tool_result(answer, "c3"),
        "written through link\nAGENTS.md\nREADME.md\nnotes.txt\n"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.external.path().join("notes.txt")).unwrap(),
        "written through link\n",
        "the write lands in the symlink target"
    );
    assert!(
        std::fs::symlink_metadata(workspace.frontend())
            .unwrap()
            .file_type()
            .is_symlink(),
        "the write does not replace the symlink"
    );
    harness.assert_no_stray_network();
}

/// Writes `APPEND_SYSTEM.md` files at user level (agent dir), in the cwd's `.omp/` and in a repository's `.omp/`.
fn write_append_files(harness: &Harness, workspace: &Workspace, in_cwd: bool) {
    std::fs::write(
        harness.env.agent_dir().join("APPEND_SYSTEM.md"),
        "User append marker: quince-5\n",
    )
    .unwrap();
    std::fs::create_dir_all(workspace.backend().join(".omp")).unwrap();
    std::fs::write(
        workspace.backend().join(".omp/APPEND_SYSTEM.md"),
        "Repo append marker: loquat-8\n",
    )
    .unwrap();
    if in_cwd {
        std::fs::create_dir_all(workspace.ws.join(".omp")).unwrap();
        std::fs::write(
            workspace.ws.join(".omp/APPEND_SYSTEM.md"),
            "Cwd append marker: medlar-3\n",
        )
        .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn append_system_prompt_ends_the_prompt_and_replaces_every_append_file() {
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let workspace = Workspace::new(&harness);
    write_append_files(&harness, &workspace, true);
    let mut spec = harness.spec(&[&workspace.backend()]);
    spec.append_system_prompt = Some("omfp line A: fig-1\n\nomfp line B: fig-2".into());

    let system = first_prompt(&harness, spec).await;

    assert!(
        system.ends_with("</critical>\n\nomfp line A: fig-1\n\nomfp line B: fig-2"),
        "appended verbatim after the closing <critical> block: {}",
        &system[system.len().saturating_sub(400)..]
    );
    assert!(
        system.find("</project-context>") < system.find("omfp line A"),
        "{system}"
    );
    for dropped in ["quince-5", "medlar-3", "loquat-8"] {
        assert!(!system.contains(dropped), "{dropped} kept: {system}");
    }
    harness.assert_no_stray_network();
}

#[tokio::test(flavor = "multi_thread")]
async fn without_the_flag_the_cwd_append_file_wins_over_the_user_one() {
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let workspace = Workspace::new(&harness);
    write_append_files(&harness, &workspace, true);

    let system = first_prompt(&harness, harness.spec(&[&workspace.backend()])).await;

    assert!(
        system.ends_with("</critical>\n\nCwd append marker: medlar-3"),
        "file content is trimmed and appended last: {}",
        &system[system.len().saturating_sub(400)..]
    );
    assert!(!system.contains("quince-5"), "{system}");
    assert!(
        !system.contains("loquat-8"),
        "an add-dir root's .omp is not searched"
    );
    harness.assert_no_stray_network();
}

#[tokio::test(flavor = "multi_thread")]
async fn without_the_flag_or_a_cwd_file_the_user_append_file_applies() {
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let workspace = Workspace::new(&harness);
    write_append_files(&harness, &workspace, false);

    let system = first_prompt(&harness, harness.spec(&[&workspace.backend()])).await;

    assert!(
        system.ends_with("</critical>\n\nUser append marker: quince-5"),
        "{}",
        &system[system.len().saturating_sub(400)..]
    );
    assert!(!system.contains("loquat-8"), "{system}");
    harness.assert_no_stray_network();
}

#[tokio::test(flavor = "multi_thread")]
async fn config_overlay_additional_directories_act_like_add_dir() {
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let workspace = Workspace::new(&harness);
    let overlay = harness.thread.path().join("overlay.yml");
    std::fs::write(
        &overlay,
        format!(
            "workspace:\n  additionalDirectories:\n    - {}\n    - {}\n",
            path(&workspace.backend()),
            path(&workspace.frontend())
        ),
    )
    .unwrap();
    let env = &harness.env;
    let ws = path(&workspace.ws);
    let overlay = path(&overlay);

    let session = OmpSession::start(env, &["--cwd", &ws, "--config", &overlay]).unwrap();
    session
        .client()
        .prompt_and_wait(&prompt("hello"), TIMEOUT)
        .unwrap();
    drop(session);

    let system = system_prompt(&harness.fake.requests()[0]);
    assert!(system.contains(BACKEND_MARKER), "{system}");
    assert!(system.contains(FRONTEND_MARKER), "{system}");
    assert_eq!(
        workspace_roots(&system),
        [path(&workspace.backend()), path(&workspace.frontend())]
    );
    harness.assert_no_stray_network();
}
