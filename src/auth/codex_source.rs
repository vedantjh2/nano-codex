//! A link records credential ownership explicitly; a path alone cannot establish it.
use super::tokens;
use crate::{config, storage};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
pub struct Source {
    pub home: PathBuf,
    pub managed: bool,
}

fn marker() -> Result<PathBuf> {
    Ok(config::home()?.join("codex-source.json"))
}

impl Source {
    pub fn existing() -> Result<Self> {
        let home = if let Some(home) = env::var_os("CODEX_HOME") {
            PathBuf::from(home)
        } else {
            PathBuf::from(env::var_os("HOME").context("Set HOME or CODEX_HOME")?).join(".codex")
        };
        Ok(Self {
            home,
            managed: false,
        })
    }

    pub fn isolated() -> Result<Self> {
        Ok(Self {
            home: config::home()?.join("codex-login"),
            managed: true,
        })
    }

    pub fn selected() -> Result<Self> {
        if !marker()?.exists() {
            return Self::existing();
        }
        Ok(serde_json::from_slice(&fs::read(marker()?)?)?)
    }

    pub fn save(&mut self) -> Result<()> {
        self.home = self.home.canonicalize()?;
        storage::write_private(&marker()?, &serde_json::to_vec(self)?)
    }

    pub fn unlink() -> Result<()> {
        if marker()?.exists() {
            fs::remove_file(marker()?)?;
        }
        Ok(())
    }

    pub fn read(&self) -> Result<Value> {
        read(&self.home)
    }
}

pub fn read(home: &Path) -> Result<Value> {
    let value: Value = serde_json::from_slice(&fs::read(home.join("auth.json")).context(
        "No file-backed Codex account. Run nano-codex login for an isolated browser sign-in",
    )?)?;
    ensure!(
        value["auth_mode"].as_str().is_none_or(|m| m == "chatgpt"),
        "Codex is not signed in with ChatGPT"
    );
    ensure!(
        value["tokens"]["access_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "Codex has no ChatGPT access token"
    );
    Ok(value)
}

pub fn expires(value: &Value) -> Result<u64> {
    // Trusted local cache: the server validates the bearer. Decode only expiry;
    // never use unverified identity claims to authorize an OAuth callback.
    let token = value["tokens"]["access_token"]
        .as_str()
        .context("Missing bearer")?;
    let payload = token.split('.').nth(1).context("Invalid cached token")?;
    let payload: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    payload["exp"]
        .as_u64()
        .context("Cached token has no expiry")
}

pub fn available() -> bool {
    Source::existing()
        .and_then(|s| s.read())
        .and_then(|v| expires(&v))
        .is_ok_and(|e| e > tokens::now() + 60)
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
