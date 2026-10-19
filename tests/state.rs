mod common;
use nano_codex::{
    context,
    protocol::{Message, ToolCall},
    session::Session,
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

fn pending() -> Message {
    Message::Assistant {
        content: String::new(),
        calls: vec![ToolCall {
            id: "c1".into(),
            name: "shell".into(),
            arguments: "{}".into(),
        }],
        items: vec![],
        reasoning: None,
    }
}

#[test]
fn resume_repairs_pending_tools_without_reexecuting_them_and_discards_torn_bytes() {
    let root = tempfile::tempdir().unwrap();
    let config = common::config(root.path());
    let mut session = Session::open(&config, None).unwrap();
    session
        .push(Message::User {
            content: "task".into(),
        })
        .unwrap();
    session.push(pending()).unwrap();
    let id = session.id.clone();
    drop(session);
    let path = root.path().join(format!(".nano-codex/sessions/{id}.jsonl"));
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(&[b'{', 0xe2])
        .unwrap();
    let session = Session::open(&config, Some(&id)).unwrap();
    assert_eq!(session.messages.len(), 3);
    match &session.messages[2] {
        Message::Tool { call_id, content } => {
            assert_eq!(call_id, "c1");
            assert!(content.contains("may have run"));
        }
        _ => panic!("Missing interruption result"),
    }
    drop(session);
    assert_eq!(Session::open(&config, Some(&id)).unwrap().messages.len(), 3);
}

#[test]
fn session_locks_and_provider_identity_prevent_unsafe_resume() {
    let root = tempfile::tempdir().unwrap();
    let mut config = common::config(root.path());
    let session = Session::open(&config, None).unwrap();
    let id = session.id.clone();
    assert!(Session::open(&config, Some(&id)).is_err());
    drop(session);
    config.model = "another-model".into();
    assert!(Session::open(&config, Some(&id)).is_err());
    assert!(Session::open(&config, Some("../../escape")).is_err());
}

#[cfg(unix)]
#[test]
fn journals_do_not_follow_repository_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join(".nano-codex")).unwrap();
    assert!(Session::open(&common::config(root.path()), None).is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn complete_corrupt_records_are_not_silently_discarded() {
    let root = tempfile::tempdir().unwrap();
    let config = common::config(root.path());
    let session = Session::open(&config, None).unwrap();
    let id = session.id.clone();
    drop(session);
    OpenOptions::new()
        .append(true)
        .open(root.path().join(format!(".nano-codex/sessions/{id}.jsonl")))
        .unwrap()
        .write_all(b"broken-json\n")
        .unwrap();
    assert!(Session::open(&config, Some(&id)).is_err());
}

#[test]
fn compaction_drops_whole_old_turns_and_preserves_the_active_task() {
    let root = tempfile::tempdir().unwrap();
    let config = common::config(root.path());
    let mut session = Session::open(&config, None).unwrap();
    session
        .push(Message::User {
            content: "old task".repeat(5000),
        })
        .unwrap();
    session.push(pending()).unwrap();
    session
        .push(Message::Tool {
            call_id: "c1".into(),
            content: "large output".repeat(5000),
        })
        .unwrap();
    session
        .push(Message::User {
            content: "active task".into(),
        })
        .unwrap();
    assert!(context::fit(&mut session, "instructions", config.context_bytes).unwrap());
    assert_eq!(session.messages.len(), 2);
    assert!(matches!(&session.messages[1], Message::User {content} if content == "active task"));
    session.repair_pending().unwrap();
}

#[test]
fn an_oversized_active_task_is_rejected_instead_of_silently_truncated() {
    let root = tempfile::tempdir().unwrap();
    let config = common::config(root.path());
    let mut session = Session::open(&config, None).unwrap();
    session
        .push(Message::User {
            content: "x".repeat(30_000),
        })
        .unwrap();
    assert!(context::fit(&mut session, "instructions", 20_000).is_err());
    assert_eq!(session.messages.len(), 1);
}

#[test]
fn ancestor_project_instructions_are_loaded_in_order() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".git")).unwrap();
    fs::create_dir(root.path().join("child")).unwrap();
    fs::write(root.path().join("AGENTS.md"), "root instruction").unwrap();
    fs::write(root.path().join("child/AGENTS.md"), "child instruction").unwrap();
    let instructions = context::instructions(&common::config(&root.path().join("child"))).unwrap();
    assert!(
        instructions.find("root instruction").unwrap()
            < instructions.find("child instruction").unwrap()
    );
}
