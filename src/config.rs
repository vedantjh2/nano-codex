//! Settings have one precedence rule: CLI / environment > user TOML > defaults.
pub use crate::cli::{Action, Cli, Options};
use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Openai,
    Chatgpt,
    Codex,
    Vllm,
    Sglang,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Approval {
    #[default]
    Ask,
    ReadOnly,
    Auto,
}

pub struct Config {
    pub provider: ProviderKind,
    pub model: String,
    pub base_url: String,
    pub cwd: PathBuf,
    pub approval: Approval,
    pub max_steps: usize,
    pub context_bytes: usize,
    pub tool_timeout: u64,
}

pub fn home() -> Result<PathBuf> {
    if let Some(directory) = env::var_os("NANO_CODEX_HOME") {
        return Ok(PathBuf::from(directory));
    }
    Ok(
        PathBuf::from(env::var_os("HOME").context("Set HOME or NANO_CODEX_HOME")?)
            .join(".nano-codex"),
    )
}

impl Config {
    pub fn load(options: Options) -> Result<Self> {
        let path = home()?.join("config.toml");
        let saved: Options = if path.exists() {
            toml::from_str(&fs::read_to_string(path)?).context("Invalid config.toml")?
        } else {
            Options::default()
        };
        let provider = options
            .provider
            .or(saved.provider)
            .unwrap_or(ProviderKind::Openai);
        let local = matches!(provider, ProviderKind::Vllm | ProviderKind::Sglang);
        let model = options.model.or(saved.model).unwrap_or_else(|| {
            if local {
                String::new()
            } else {
                "gpt-6.1-sol".into()
            }
        });
        let default_url = match provider {
            ProviderKind::Vllm => "http://127.0.0.1:8000/v1",
            ProviderKind::Sglang => "http://127.0.0.1:30000/v1",
            ProviderKind::Codex => crate::auth::codex::BASE_URL,
            _ => "https://api.openai.com/v1",
        };
        let base_url = options
            .base_url
            .or(saved.base_url)
            .unwrap_or(default_url.into());
        let url = reqwest::Url::parse(&base_url).context("Invalid base URL")?;
        ensure!(
            matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
            "Use an HTTP(S) API URL"
        );
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Base URL must not contain credentials, query, or fragment"
        );
        if !local {
            ensure!(
                base_url.trim_end_matches('/') == default_url,
                "Account credentials are only sent to the provider's official endpoint"
            );
        }
        let config = Self {
            provider,
            model,
            base_url: base_url.trim_end_matches('/').into(),
            cwd: options
                .cwd
                .or(saved.cwd)
                .unwrap_or(env::current_dir()?)
                .canonicalize()?,
            approval: options.approval.or(saved.approval).unwrap_or_default(),
            max_steps: options.max_steps.or(saved.max_steps).unwrap_or(40),
            context_bytes: options
                .context_bytes
                .or(saved.context_bytes)
                .unwrap_or(180_000),
            tool_timeout: options.tool_timeout.or(saved.tool_timeout).unwrap_or(30),
        };
        ensure!(config.cwd.is_dir(), "Workspace must be a directory");
        ensure!(
            config.max_steps > 0 && config.tool_timeout > 0 && config.context_bytes >= 8192,
            "Use positive limits and at least 8192 context bytes"
        );
        ensure!(
            config.model.is_empty() || !config.model.trim().is_empty(),
            "Model must not be whitespace"
        );
        Ok(config)
    }
}

pub fn save_connection(provider: ProviderKind, model: &str, base_url: &str) -> Result<()> {
    let path = home()?.join("config.toml");
    let mut saved: Options = if path.exists() {
        toml::from_str(&fs::read_to_string(&path)?)?
    } else {
        Options::default()
    };
    saved.provider = Some(provider);
    saved.model = Some(model.into());
    saved.base_url = Some(base_url.into());
    crate::storage::write_private(&path, toml::to_string_pretty(&saved)?.as_bytes())
}

pub fn clear_connection() -> Result<()> {
    let path = home()?.join("config.toml");
    if !path.exists() {
        return Ok(());
    }
    let mut saved: Options = toml::from_str(&fs::read_to_string(&path)?)?;
    saved.provider = None;
    saved.model = None;
    saved.base_url = None;
    crate::storage::write_private(&path, toml::to_string_pretty(&saved)?.as_bytes())
}
