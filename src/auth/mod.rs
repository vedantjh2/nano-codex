//! API keys and ChatGPT plans are different credentials; local servers get neither.
pub mod codex;
mod codex_rpc;
mod codex_source;
pub mod commands;
mod oauth;
mod store;
mod tokens;

use crate::config::ProviderKind;
use anyhow::{Context, Result};
use reqwest::Client;
use std::env;
use store::Store;

pub async fn credential(kind: ProviderKind, client: &Client) -> Result<Option<String>> {
    match kind {
        ProviderKind::Vllm | ProviderKind::Sglang => Ok(env::var("NANO_CODEX_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())),
        ProviderKind::Openai => Ok(Some(
            env::var("OPENAI_API_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .context("Set OPENAI_API_KEY, or use --provider chatgpt after nano-codex login")?,
        )),
        ProviderKind::Chatgpt => {
            let store = Store::open()?; // Lock covers read, rotating refresh, and write.
            let mut account = store.load()?.context("Run nano-codex login first")?;
            if account.expires_at <= tokens::now() + 60 {
                tokens::refresh(client, &mut account).await?;
                store.save(&account)?;
            }
            Ok(Some(
                account.access_token.context("Run nano-codex login first")?,
            ))
        }
        ProviderKind::Codex => anyhow::bail!("Codex uses account headers, not a bare API key"),
    }
}

pub async fn headers(kind: ProviderKind, client: &Client) -> Result<reqwest::header::HeaderMap> {
    if kind == ProviderKind::Codex {
        return codex::headers().await;
    }
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = credential(kind, client).await? {
        let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))?;
        value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
    Ok(headers)
}

pub async fn login(client: &Client) -> Result<()> {
    oauth::login(client).await
}

pub async fn logout(client: &Client) -> Result<()> {
    let store = Store::open()?;
    let Some(mut account) = store.load()? else {
        eprintln!("No ChatGPT account saved.");
        return Ok(());
    };
    let mut revoked = true;
    if let Some(refresh_token) = &account.refresh_token {
        let result = async {
            let discovery = tokens::discovery(client).await?;
            let endpoint = discovery
                .revocation_endpoint
                .context("Missing revocation endpoint")?;
            client
                .post(endpoint)
                .form(&[
                    ("token", refresh_token.as_str()),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", account.client_id.as_str()),
                ])
                .send()
                .await?
                .error_for_status()?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        revoked = result.is_ok();
    }
    account.access_token = None;
    account.refresh_token = None;
    account.id_token = None;
    account.expires_at = 0;
    store.save(&account)?;
    eprintln!("Signed out of nano-codex.");
    if !revoked {
        eprintln!(
            "Remote revocation was not confirmed. Disconnect nano-codex in ChatGPT Settings to revoke access."
        );
    }
    Ok(())
}
