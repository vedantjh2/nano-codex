mod common;
use common::{Server, answer, call};
use serde_json::{Value, json};
use std::{
    fs,
    process::{Command, Output},
};

fn run(root: &std::path::Path, server: &Server, provider: &str, approval: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nano-codex"))
        .env("NANO_CODEX_HOME", root.join("config"))
        .env("OPENAI_API_KEY", "must-never-reach-a-local-server")
        .env_remove("NANO_CODEX_API_KEY")
        .args([
            "--provider",
            provider,
            "--model",
            "test-model",
            "--base-url",
            &format!("{}/v1", server.url),
            "-C",
            root.to_str().unwrap(),
            "--approval",
            approval,
            "--json",
            "exec",
            "Fix the note and check it.",
        ])
        .output()
        .unwrap()
}

#[test]
fn both_local_providers_complete_a_real_edit_and_shell_check() {
    for provider in ["vllm", "sglang"] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("note.txt"), "before\n").unwrap();
        let server = Server::start(vec![
            (200, call("read", "read_file", json!({"path":"note.txt"}))),
            (
                200,
                call(
                    "edit",
                    "edit_file",
                    json!({"path":"note.txt","old_text":"before","new_text":"after"}),
                ),
            ),
            (
                200,
                call("check", "shell", json!({"command":"cat note.txt"})),
            ),
            (200, answer("Changed the note and verified it.")),
        ]);
        let output = run(root.path(), &server, provider, "auto");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(root.path().join("note.txt")).unwrap(),
            "after\n"
        );
        let events: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert!(events.iter().any(|e| e["event"] == "done"));
        let requests = server.finish();
        assert_eq!(requests.len(), 4);
        assert!(
            requests
                .iter()
                .all(|r| r.path == "/v1/chat/completions" && !r.headers.contains("must-never"))
        );
        assert_eq!(requests[1].body["messages"][3]["tool_call_id"], "read");
        assert!(requests[3].body.to_string().contains("exit=0"));
    }
}

#[test]
fn read_only_and_noninteractive_ask_deny_mutations() {
    for policy in ["read-only", "ask"] {
        let root = tempfile::tempdir().unwrap();
        let server = Server::start(vec![
            (
                200,
                call(
                    "write",
                    "write_file",
                    json!({"path":"forbidden","content":"no"}),
                ),
            ),
            (200, answer("The write was denied.")),
        ]);
        let output = run(root.path(), &server, "vllm", policy);
        assert!(output.status.success());
        assert!(!root.path().join("forbidden").exists());
        let requests = server.finish();
        assert!(requests[1].body.to_string().contains("Error:"));
    }
}

#[test]
fn an_interrupted_stream_never_executes_a_complete_looking_call() {
    let root = tempfile::tempdir().unwrap();
    let stream = call(
        "write",
        "write_file",
        json!({"path":"forbidden","content":"no"}),
    )
    .replace("data: [DONE]\n\n", "");
    let server = Server::start(vec![(200, stream)]);
    let output = run(root.path(), &server, "vllm", "auto");
    assert!(!output.status.success());
    assert!(!root.path().join("forbidden").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Disconnected"));
    server.finish();
}

#[test]
fn retryable_http_errors_are_retried_before_the_stream() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(vec![
        (503, "unavailable".into()),
        (200, answer("Recovered.")),
    ]);
    let output = run(root.path(), &server, "sglang", "read-only");
    assert!(output.status.success());
    assert_eq!(server.finish().len(), 2);
}

#[test]
fn streams_without_a_content_type_header_are_still_decoded() {
    let root = tempfile::tempdir().unwrap();
    let server =
        Server::without_content_type(vec![(200, answer("Valid SSE without an optional header."))]);
    let output = run(root.path(), &server, "vllm", "read-only");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.finish();
}
