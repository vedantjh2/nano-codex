use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn cache() -> String {
    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let payload = URL_SAFE_NO_PAD.encode(json!({"exp":expiry}).to_string());
    json!({"auth_mode":"chatgpt","tokens":{"access_token":format!("h.{payload}.s"),"account_id":"test-account","refresh_token":"test-refresh"}}).to_string()
}

fn cli(settings: &Path, binaries: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nano-codex"));
    command
        .env("NANO_CODEX_HOME", settings)
        .env("PATH", binaries)
        .env_remove("NANO_CODEX_PROVIDER")
        .env_remove("NANO_CODEX_MODEL")
        .env_remove("NANO_CODEX_BASE_URL");
    command
}

#[cfg(unix)]
fn executable(path: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn shared_login_stays_read_only_even_when_it_uses_the_isolated_directory_name() {
    let root = tempfile::tempdir().unwrap();
    let settings = root.path().join("settings");
    let original = settings.join("codex-login");
    fs::create_dir_all(&original).unwrap();
    let bytes = cache();
    fs::write(original.join("auth.json"), &bytes).unwrap();
    let binaries = root.path().join("bin");
    fs::create_dir(&binaries).unwrap();
    executable(&binaries.join("codex"), "#!/bin/sh\nexit 99\n");
    let output = cli(&settings, &binaries)
        .env("CODEX_HOME", &original)
        .args(["login", "--from-codex"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let link: Value =
        serde_json::from_slice(&fs::read(settings.join("codex-source.json")).unwrap()).unwrap();
    assert_eq!(link["managed"], false);
    assert!(link.get("tokens").is_none());
    let status = cli(&settings, &binaries)
        .args(["login", "status"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&status.stdout).contains("shared login"));
    let logout = cli(&settings, &binaries).arg("logout").output().unwrap();
    assert!(
        logout.status.success(),
        "{}",
        String::from_utf8_lossy(&logout.stderr)
    );
    assert_eq!(
        fs::read_to_string(original.join("auth.json")).unwrap(),
        bytes
    );
    assert!(!settings.join("auth.json").exists());
}

#[cfg(unix)]
#[test]
fn browser_and_device_login_use_an_isolated_codex_home() {
    for device in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let settings = root.path().join("settings");
        let binaries = root.path().join("bin");
        fs::create_dir(&binaries).unwrap();
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CODEX_HOME/invocation\"\nprintf '%s' '{}' > \"$CODEX_HOME/auth.json\"\n",
            cache()
        );
        executable(&binaries.join("codex"), &script);
        let mut command = cli(&settings, &binaries);
        command.arg("login");
        if device {
            command.arg("--device-auth");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let invocation = fs::read_to_string(settings.join("codex-login/invocation")).unwrap();
        assert!(invocation.contains("cli_auth_credentials_store=\"file\""));
        assert_eq!(invocation.contains("--device-auth"), device);
        let link: Value =
            serde_json::from_slice(&fs::read(settings.join("codex-source.json")).unwrap()).unwrap();
        assert_eq!(link["managed"], true);
        let permissions = fs::metadata(settings.join("codex-source.json"))
            .unwrap()
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(permissions.mode() & 0o777, 0o600);
    }
}
