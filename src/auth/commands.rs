//! Login UX selects a credential owner; it never copies another app's refresh token.
use super::{codex, store::Store};
use crate::{
    cli::LoginAction,
    config::{self, ProviderKind},
    ui::visible,
};
use anyhow::{Result, ensure};
use reqwest::Client;

pub async fn login(
    client: &Client,
    action: &Option<LoginAction>,
    reuse: bool,
    device: bool,
    standalone: bool,
) -> Result<()> {
    if action.is_some() {
        return status();
    }
    if standalone {
        super::login(client).await?;
        config::save_connection(
            ProviderKind::Chatgpt,
            "gpt-6.1-sol",
            "https://api.openai.com/v1",
        )?;
    } else {
        codex::login(reuse, device).await?;
    }
    Ok(())
}

fn status() -> Result<()> {
    let config = config::Config::load(Default::default())?;
    match config.provider {
        ProviderKind::Codex => codex::status(),
        ProviderKind::Chatgpt => {
            let store = Store::open()?;
            let account = store
                .load()?
                .ok_or_else(|| anyhow::anyhow!("Not signed in"))?;
            ensure!(account.access_token.is_some(), "Not signed in");
            println!(
                "ChatGPT via native OAuth; {}",
                if account.expires_at > super::tokens::now() {
                    "access token valid"
                } else {
                    "renewal required"
                }
            );
            Ok(())
        }
        ProviderKind::Openai => {
            ensure!(
                std::env::var("OPENAI_API_KEY").is_ok_and(|s| !s.is_empty()),
                "OPENAI_API_KEY is not set"
            );
            println!("OpenAI API key is configured");
            Ok(())
        }
        _ => {
            println!("Local inference: {}", visible(&config.base_url));
            Ok(())
        }
    }
}

pub async fn logout(client: &Client) -> Result<()> {
    let config = config::Config::load(Default::default())?;
    if config.provider == ProviderKind::Codex {
        return codex::logout().await;
    }
    super::logout(client).await?;
    if config.provider == ProviderKind::Chatgpt {
        config::clear_connection()?;
    }
    Ok(())
}
