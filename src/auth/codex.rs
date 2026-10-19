//! Codex account compatibility. Shared credentials are read, never copied or rotated.
pub use super::codex_source::available;
use super::{
    codex_source::{Source, expires},
    store::Store,
    tokens,
};
use crate::config;
use anyhow::{Context, Result, ensure};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use std::fs;
use tokio::process::Command;

pub const BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const API_VERSION: &str = "0.163.0";

pub async fn headers() -> Result<HeaderMap> {
    let _lock = Store::open()?;
    let source = Source::selected()?;
    let mut cached = source.read()?;
    if expires(&cached)? <= tokens::now() + 60 {
        ensure!(
            source.managed,
            "Shared Codex login needs renewal. Open Codex, or run nano-codex login for an independent session"
        );
        super::codex_rpc::refresh(&source.home).await?;
        cached = source.read()?;
        ensure!(
            expires(&cached)? > tokens::now(),
            "Codex token renewal failed; run nano-codex login"
        );
    }
    let mut headers = HeaderMap::new();
    let token = cached["tokens"]["access_token"]
        .as_str()
        .context("Missing bearer")?;
    let mut bearer = HeaderValue::from_str(&format!("Bearer {token}"))?;
    bearer.set_sensitive(true);
    headers.insert(AUTHORIZATION, bearer);
    let account = cached["tokens"]["account_id"]
        .as_str()
        .context("Missing Codex account ID")?;
    headers.insert("chatgpt-account-id", HeaderValue::from_str(account)?);
    headers.insert("originator", HeaderValue::from_static("nano-codex"));
    Ok(headers)
}

pub async fn login(reuse: bool, device: bool) -> Result<()> {
    let _lock = Store::open()?;
    let mut source = if reuse {
        Source::existing()?
    } else {
        Source::isolated()?
    };
    if !reuse {
        fs::create_dir_all(&source.home)?;
        let mut command = Command::new("codex");
        command
            .env("CODEX_HOME", &source.home)
            .args(["-c", "cli_auth_credentials_store=\"file\"", "login"])
            .kill_on_drop(true);
        if device {
            command.arg("--device-auth");
        }
        let status = command
            .status()
            .await
            .context("Install Codex to use browser/device sign-in, or use login --standalone")?;
        ensure!(status.success(), "Codex sign-in did not complete");
    }
    ensure!(
        expires(&source.read()?)? > tokens::now(),
        "Codex login is expired"
    );
    source.save()?;
    config::save_connection(config::ProviderKind::Codex, "gpt-6.1-sol", BASE_URL)?;
    eprintln!(
        "ChatGPT account linked. nano-codex owns the model/tool loop; Codex manages this login."
    );
    Ok(())
}

pub fn status() -> Result<()> {
    let source = Source::selected()?;
    let cached = source.read()?;
    println!(
        "ChatGPT account via Codex; {}; {}",
        if source.managed {
            "isolated session"
        } else {
            "shared login (read only)"
        },
        if expires(&cached)? > tokens::now() {
            "access token valid"
        } else {
            "renewal required"
        }
    );
    Ok(())
}

pub async fn logout() -> Result<()> {
    let _lock = Store::open()?;
    let source = Source::selected()?;
    if source.managed {
        let status = Command::new("codex")
            .env("CODEX_HOME", &source.home)
            .args(["-c", "cli_auth_credentials_store=\"file\"", "logout"])
            .kill_on_drop(true)
            .status()
            .await?;
        ensure!(status.success(), "Codex logout failed");
    }
    Source::unlink()?;
    config::clear_connection()?;
    eprintln!("Disconnected nano-codex. A shared Codex login remains signed in.");
    Ok(())
}
