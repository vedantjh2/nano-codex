use super::*;
use serde_json::json;

#[test]
fn codex_cache_validation_requires_a_chatgpt_token_and_expiry() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("auth.json");
    fs::write(
        &path,
        json!({"auth_mode":"apikey","tokens":{"access_token":"wrong"}}).to_string(),
    )
    .unwrap();
    assert!(read(root.path()).is_err());
    fs::write(
        &path,
        json!({"auth_mode":"chatgpt","tokens":{"access_token":"opaque"}}).to_string(),
    )
    .unwrap();
    assert!(expires(&read(root.path()).unwrap()).is_err());
    let payload = URL_SAFE_NO_PAD.encode(json!({"exp":tokens::now()+3600}).to_string());
    fs::write(&path,json!({"auth_mode":"chatgpt","tokens":{"access_token":format!("header.{payload}.signature"),"account_id":"account"}}).to_string()).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(expires(&read(root.path()).unwrap()).unwrap() > tokens::now());
    assert_eq!(
        fs::read(path).unwrap(),
        bytes,
        "Reading credentials must not modify them"
    );
}
