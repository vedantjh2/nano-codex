use super::*;

#[tokio::test]
async fn renewal_uses_initialize_then_account_read_with_refresh() {
    let root = tempfile::tempdir().unwrap();
    let script = "IFS= read -r initialize; printf '%s\\n' \"$initialize\" > initialize.json; printf '%s\\n' '{\"id\":1,\"result\":{}}'; IFS= read -r notification; IFS= read -r account; printf '%s\\n' \"$account\" > account.json; printf '%s\\n' '{\"method\":\"notice\"}' '{\"id\":2,\"result\":{\"account\":{\"type\":\"chatgpt\"}}}'; sleep 1";
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]).current_dir(root.path());
    renew(command).await.unwrap();
    let initialize: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("initialize.json")).unwrap())
            .unwrap();
    let account: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("account.json")).unwrap()).unwrap();
    assert_eq!(initialize["method"], "initialize");
    assert_eq!(account["method"], "account/read");
    assert_eq!(account["params"]["refreshToken"], true);
}

#[tokio::test]
async fn renewal_rejects_disconnected_or_failed_auth_services() {
    for script in [
        "exit 0",
        "IFS= read -r request; printf '%s\\n' '{\"id\":1,\"error\":{\"message\":\"secret detail\"}}'",
    ] {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        let error = renew(command).await.unwrap_err().to_string();
        assert!(!error.contains("secret detail"));
    }
}
