mod common;
use nano_codex::{
    config::Approval,
    protocol::ToolCall,
    tools::{self, paths, shell},
    ui::Ui,
};
use serde_json::json;
use std::{fs, time::Duration};

fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: "test".into(),
        name: name.into(),
        arguments: arguments.to_string(),
    }
}

#[tokio::test]
async fn file_edits_require_one_exact_match_and_never_clobber_existing_files() {
    let root = tempfile::tempdir().unwrap();
    let config = common::config(root.path());
    let mut ui = Ui::new(true);
    fs::write(root.path().join("file"), "x x").unwrap();
    let ambiguous = call(
        "edit_file",
        json!({"path":"file","old_text":"x","new_text":"y"}),
    );
    assert!(
        tools::execute(&ambiguous, &config, &mut ui)
            .await
            .contains("exactly once")
    );
    let overwrite = call("write_file", json!({"path":"file","content":"clobber"}));
    assert!(
        tools::execute(&overwrite, &config, &mut ui)
            .await
            .starts_with("Error:")
    );
    assert_eq!(fs::read_to_string(root.path().join("file")).unwrap(), "x x");
    let edit = call(
        "edit_file",
        json!({"path":"file","old_text":"x x","new_text":"fixed"}),
    );
    assert!(
        !tools::execute(&edit, &config, &mut ui)
            .await
            .starts_with("Error:")
    );
    assert_eq!(
        fs::read_to_string(root.path().join("file")).unwrap(),
        "fixed"
    );
}

#[tokio::test]
async fn read_file_accepts_the_minimum_line_count_advertised_to_the_model() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("file"), "observed value\n").unwrap();
    let schema = tools::definitions()
        .into_iter()
        .find(|tool| tool["name"] == "read_file")
        .unwrap();
    let minimum = schema["parameters"]["properties"]["lines"]["minimum"]
        .as_u64()
        .unwrap();
    let output = tools::execute(
        &call("read_file", json!({"path":"file","lines":minimum})),
        &common::config(root.path()),
        &mut Ui::new(true),
    )
    .await;
    assert!(
        output.contains("observed value"),
        "The advertised minimum failed: {output}"
    );
}

#[test]
fn paths_reject_traversal_symlink_escapes_and_protected_directories() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    fs::create_dir(root_path.join(".git")).unwrap();
    assert!(paths::resolve(&root_path, "../escape").is_err());
    assert!(paths::resolve(&root_path, "/etc/passwd").is_err());
    assert!(paths::resolve(&root_path, ".git/config").is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), root_path.join("outside")).unwrap();
        std::os::unix::fs::symlink(root_path.join(".git"), root_path.join("alias")).unwrap();
        assert!(paths::resolve(&root_path, "outside/file").is_err());
        assert!(paths::resolve(&root_path, "alias/config").is_err());
    }
}

#[tokio::test]
async fn invalid_arguments_and_denied_shells_are_tool_results() {
    let root = tempfile::tempdir().unwrap();
    let mut config = common::config(root.path());
    let mut ui = Ui::new(true);
    config.approval = Approval::ReadOnly;
    let shell = call("shell", json!({"command":"touch forbidden"}));
    assert!(
        tools::execute(&shell, &config, &mut ui)
            .await
            .contains("Denied")
    );
    assert!(!root.path().join("forbidden").exists());
    for invalid in [
        call("unknown", json!({})),
        call("read_file", json!({"path":"x","extra":1})),
        call("read_file", json!({"path":"x","tool":"shell"})),
    ] {
        assert!(
            tools::execute(&invalid, &config, &mut ui)
                .await
                .starts_with("Error:")
        );
    }
}

#[tokio::test]
async fn search_respects_gitignore_and_read_output_is_bounded() {
    let root = tempfile::tempdir().unwrap();
    let config = common::config(root.path());
    let mut ui = Ui::new(true);
    fs::create_dir(root.path().join(".git")).unwrap();
    fs::write(root.path().join(".gitignore"), "secret\n").unwrap();
    fs::write(root.path().join("secret"), "needle hidden").unwrap();
    fs::write(root.path().join("visible"), "needle visible").unwrap();
    let output = tools::execute(&call("search", json!({"query":"needle"})), &config, &mut ui).await;
    assert!(output.contains("visible"));
    assert!(!output.contains("hidden"));
    fs::write(root.path().join("big"), "é".repeat(20_000)).unwrap();
    let output = tools::execute(&call("read_file", json!({"path":"big"})), &config, &mut ui).await;
    assert!(output.len() < 16_100 && output.contains("truncated"));
}

#[tokio::test]
async fn shell_drains_output_without_deadlock_and_reports_nonzero_exit() {
    let root = tempfile::tempdir().unwrap();
    let output = shell::run(
        root.path(),
        "yes x | head -c 100000; printf problem >&2; exit 7",
        2,
    )
    .await
    .unwrap();
    assert!(
        output.contains("exit=7") && output.contains("truncated") && output.contains("problem")
    );
    assert!(output.len() < 17_000);
}

#[tokio::test]
async fn timeout_and_future_cancellation_kill_descendant_processes() {
    for cancel in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let command = "(sleep 2; touch leaked) & wait";
        if cancel {
            assert!(
                tokio::time::timeout(
                    Duration::from_millis(100),
                    shell::run(root.path(), command, 10)
                )
                .await
                .is_err()
            );
        } else {
            assert!(shell::run(root.path(), command, 1).await.is_err());
        }
        tokio::time::sleep(Duration::from_millis(2100)).await;
        assert!(!root.path().join("leaked").exists());
    }
}
