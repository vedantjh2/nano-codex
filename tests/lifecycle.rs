mod common;
use common::{Server, answer, call};
use nano_codex::{protocol::Message, session::Session};
use serde_json::json;
use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn command(root: &std::path::Path, server: &Server) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nano-codex"));
    command
        .env("NANO_CODEX_HOME", root.join("config"))
        .env_remove("NANO_CODEX_API_KEY")
        .args([
            "--provider",
            "vllm",
            "--model",
            "test-model",
            "--base-url",
            &format!("{}/v1", server.url),
            "-C",
            root.to_str().unwrap(),
            "--approval",
            "auto",
        ]);
    command
}

#[test]
fn resume_replays_the_saved_conversation_to_the_model() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(vec![(200, answer("Continued."))]);
    let mut config = common::config(root.path());
    config.base_url = format!("{}/v1", server.url);
    let mut session = Session::open(&config, None).unwrap();
    session
        .push(Message::User {
            content: "Remember the original requirement".into(),
        })
        .unwrap();
    let id = session.id.clone();
    drop(session);
    let output = command(root.path(), &server)
        .args(["resume", &id, "Continue the original task"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let requests = server.finish();
    assert!(
        requests[0]
            .body
            .to_string()
            .contains("original requirement")
    );
    assert!(
        requests[0]
            .body
            .to_string()
            .contains("Continue the original task")
    );
}

#[test]
fn step_exhaustion_is_an_error_with_a_resumable_session() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(vec![(200, call("read", "list_files", json!({"path":"."})))]);
    let output = command(root.path(), &server)
        .args(["--max-steps", "1", "exec", "Keep going"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("max-steps=1"));
    assert_eq!(
        fs::read_dir(root.path().join(".nano-codex/sessions"))
            .unwrap()
            .count(),
        1
    );
    server.finish();
}

#[test]
fn model_listing_needs_no_local_model_name_or_home_variable() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(vec![(
        200,
        json!({"data":[{"id":"served-model"}]}).to_string(),
    )]);
    let output = Command::new(env!("CARGO_BIN_EXE_nano-codex"))
        .env("NANO_CODEX_HOME", root.path().join("config"))
        .env_remove("HOME")
        .env_remove("NANO_CODEX_MODEL")
        .env_remove("NANO_CODEX_API_KEY")
        .args([
            "--provider",
            "vllm",
            "--base-url",
            &format!("{}/v1", server.url),
            "models",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "served-model\n");
    assert_eq!(server.finish()[0].path, "/v1/models");
}

#[cfg(unix)]
#[test]
fn ctrl_c_kills_the_actual_cli_shell_descendants() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start(vec![(
        200,
        call(
            "shell",
            "shell",
            json!({"command":"touch started; (sleep 2; touch leaked) & wait"}),
        ),
    )]);
    let mut child = command(root.path(), &server)
        .args(["exec", "Run the command"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.path().join("started").exists() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("Shell never started");
        }
        thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: this PID belongs to the child we just launched.
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    assert_eq!(child.wait().unwrap().code(), Some(130));
    thread::sleep(Duration::from_millis(2200));
    assert!(!root.path().join("leaked").exists());
    server.finish();
}
