//! Command syntax is separate from config resolution and agent behavior.
use crate::config::{Approval, ProviderKind};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "A tiny coding agent. Read the loop, understand the harness."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Action>,
    #[command(flatten)]
    pub options: Options,
    /// Initial prompt for an interactive session
    pub prompt: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Action {
    /// Run one task and exit
    Exec {
        prompt: String,
    },
    /// Continue a saved session without rerunning uncertain operations
    Resume {
        id: String,
        prompt: Option<String>,
    },
    /// Sign in using Codex's account flow, or the independent OAuth example
    Login {
        #[command(subcommand)]
        command: Option<LoginAction>,
        /// Link the existing Codex account, without copying its tokens
        #[arg(long, conflicts_with_all = ["device_auth", "standalone"])]
        from_codex: bool,
        /// Use Codex's headless device-code login
        #[arg(long, conflicts_with = "standalone")]
        device_auth: bool,
        /// Use nano-codex's native open-source OAuth registration flow
        #[arg(long)]
        standalone: bool,
    },
    Logout,
    Models,
    /// Choose an account or configure a local inference endpoint
    Setup,
}

#[derive(Subcommand, Debug)]
pub enum LoginAction {
    Status,
}

#[derive(Parser, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    #[arg(long, global = true, value_enum, env = "NANO_CODEX_PROVIDER")]
    pub provider: Option<ProviderKind>,
    #[arg(long, short, global = true, env = "NANO_CODEX_MODEL")]
    pub model: Option<String>,
    #[arg(long, global = true, env = "NANO_CODEX_BASE_URL")]
    pub base_url: Option<String>,
    #[arg(long, short = 'C', global = true)]
    pub cwd: Option<PathBuf>,
    /// ask: approve writes/shell; read-only: deny them; auto: allow them
    #[arg(long, global = true, value_enum)]
    pub approval: Option<Approval>,
    #[arg(long, global = true)]
    pub max_steps: Option<usize>,
    /// Input budget in serialized bytes, not a token estimate
    #[arg(long, global = true)]
    pub context_bytes: Option<usize>,
    #[arg(long, global = true)]
    pub tool_timeout: Option<u64>,
    #[arg(long, global = true)]
    #[serde(skip)]
    pub json: bool,
}
