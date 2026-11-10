//! Contract tests (L3) of the daemon's omp adapter against the pinned omp and the fake LLM: session setup
//! (DMN-030..032) and multi-workspace threads (spike S-07).

mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use common::{Harness, TIMEOUT, prompt, start};
use fake_llm::{Reply, Step};
use omp_rpc::PromptStatus;
use pmo_testkit::TempRepo;

// spec: DMN-031, DMN-032
#[tokio::test(flavor = "multi_thread")]
async fn setup_opens_the_thread_session_and_a_restart_resumes_it_with_history() {
    let harness = Harness::new(vec![Step::reply(Reply::text("first answer"))]).await;
    let spec = harness.spec(&[]);

    let (first, second) = tokio::task::spawn_blocking(move || {
        let (process, startup) = start(&spec);
        let turn = process
            .client()
            .prompt_and_wait(&prompt("remember this"), TIMEOUT)
            .unwrap();
        assert_eq!(turn.result.unwrap().status, PromptStatus::Completed);
        drop(process);
        let (_process, resumed) = start(&spec);
        (startup, resumed)
    })
    .await
    .unwrap();

    assert!(
        !first.session.resumed,
        "a new thread starts a fresh session"
    );
    assert_eq!(first.history, Vec::new());
    assert_ne!(first.session.session_id, "");
    assert!(second.session.resumed, "{}", harness.stderr());
    assert_eq!(second.session.session_id, first.session.session_id);
    assert_eq!(
        second.history.len(),
        2,
        "user prompt and assistant answer: {:#?}",
        second.history
    );
    harness.assert_no_stray_network();
}

// spec: DMN-030
#[tokio::test(flavor = "multi_thread")]
async fn context_files_of_every_add_dir_root_reach_the_model() {
    let frontend = TempRepo::new().unwrap();
    frontend
        .commit_file(
            "AGENTS.md",
            "Frontend marker: kumquat-17\n",
            "add agents file",
        )
        .unwrap();
    let backend = TempRepo::new().unwrap();
    backend
        .commit_file(
            "AGENTS.md",
            "Backend marker: rutabaga-42\n",
            "add agents file",
        )
        .unwrap();
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let spec = harness.spec(&[frontend.path(), backend.path()]);

    tokio::task::spawn_blocking(move || {
        let (process, _) = start(&spec);
        process
            .client()
            .prompt_and_wait(&prompt("hello"), TIMEOUT)
            .unwrap();
    })
    .await
    .unwrap();

    let request = harness.fake.requests()[0].to_string();
    let excerpt = request
        .find("project-context")
        .map_or("<no project-context block>", |start| {
            &request[start..(start + 4000).min(request.len())]
        });
    assert!(
        request.contains("kumquat-17"),
        "frontend AGENTS.md missing from the model request: {excerpt}"
    );
    assert!(
        request.contains("rutabaga-42"),
        "backend AGENTS.md missing from the model request: {excerpt}"
    );
    harness.assert_no_stray_network();
}

// spec: DMN-029
#[tokio::test(flavor = "multi_thread")]
async fn add_dir_attaches_a_workspace_to_a_live_session() {
    let later = TempRepo::new().unwrap();
    later
        .commit_file("AGENTS.md", "Late marker: persimmon-9\n", "add agents file")
        .unwrap();
    let harness = Harness::new(vec![Step::reply(Reply::text("ok"))]).await;
    let spec = harness.spec(&[]);
    let later_path = later.path().to_path_buf();

    let ack = tokio::task::spawn_blocking(move || {
        let (process, _) = start(&spec);
        let command = format!("/add-dir {}", later_path.display());
        let ack = process
            .client()
            .call(&prompt(&command))
            .expect("/add-dir is accepted over RPC");
        process
            .client()
            .prompt_and_wait(&prompt("hello"), TIMEOUT)
            .unwrap();
        ack
    })
    .await
    .unwrap();

    assert_eq!(
        ack.agent_invoked,
        Some(false),
        "/add-dir runs locally without a model turn"
    );
    let request = harness.fake.requests()[0].to_string();
    assert!(
        request.contains(&later.path().display().to_string()),
        "the new root is announced to the model: {request}"
    );
    harness.assert_no_stray_network();
}

/// Accepts one HTTP request on a loopback port and reports its head (request line and headers).
fn record_one_request() -> (u16, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let _ = sender.send(head);
            let _ =
                stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n");
        }
    });
    (port, receiver)
}

// spec: AGT-005
#[tokio::test(flavor = "multi_thread")]
async fn an_mcp_server_in_the_thread_home_is_contacted_with_its_header() {
    let harness = Harness::new(Vec::new()).await;
    let spec = harness.spec(&[]);
    let (port, requests) = record_one_request();
    let config_dir = spec.cwd.join(".omp");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("mcp.json"),
        format!(
            r#"{{"mcpServers":{{"pmo":{{"type":"http","url":"http://127.0.0.1:{port}/mcp","headers":{{"Authorization":"Bearer thread-token-1"}}}}}}}}"#
        ),
    )
    .unwrap();

    let head = tokio::task::spawn_blocking(move || {
        let (_process, _) = start(&spec);
        requests.recv_timeout(TIMEOUT)
    })
    .await
    .unwrap()
    .unwrap_or_else(|_| {
        panic!(
            "omp never contacted the MCP server\nstderr:\n{}",
            harness.stderr()
        )
    });

    assert!(head.starts_with("POST /mcp "), "{head}");
    assert!(
        head.to_ascii_lowercase()
            .contains("authorization: bearer thread-token-1"),
        "{head}"
    );
}
