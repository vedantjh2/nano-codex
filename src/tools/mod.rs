//! Tool schemas are the model's UI. Typed arguments are the execution boundary.
mod files;
mod schema;
pub use schema::definitions;
pub mod paths;
pub mod shell;

use crate::{
    config::{Approval, Config},
    protocol::ToolCall,
    ui::Ui,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

pub const OUTPUT_BYTES: usize = 16_000;
pub const FILE_BYTES: usize = 1_048_576;

#[derive(Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case", deny_unknown_fields)]
enum Tool {
    ListFiles {
        path: String,
    },
    ReadFile {
        path: String,
        #[serde(default)]
        offset: usize,
        #[serde(default = "default_lines")]
        lines: usize,
    },
    Search {
        query: String,
    },
    WriteFile {
        path: String,
        content: String,
    },
    EditFile {
        path: String,
        old_text: String,
        new_text: String,
    },
    Shell {
        command: String,
    },
}

fn default_lines() -> usize {
    200
}

pub async fn execute(call: &ToolCall, config: &Config, ui: &mut Ui) -> String {
    match checked_execute(call, config, ui).await {
        Ok(output) => truncate(&output, OUTPUT_BYTES),
        Err(error) => truncate(&format!("Error: {error:#}"), OUTPUT_BYTES),
    }
}

async fn checked_execute(call: &ToolCall, config: &Config, ui: &mut Ui) -> Result<String> {
    let mut arguments: Value =
        serde_json::from_str(&call.arguments).context("Arguments must be JSON")?;
    let object = arguments
        .as_object_mut()
        .context("Arguments must be an object")?;
    ensure!(!object.contains_key("tool"), "Unexpected argument: tool");
    object.insert("tool".into(), json!(call.name));
    let tool: Tool = serde_json::from_value(arguments).context("Invalid tool name or arguments")?;
    let preview = match &tool {
        Tool::WriteFile { path, content } => Some(format!(
            "CREATE {path}\n{}",
            truncate(content, OUTPUT_BYTES)
        )),
        Tool::EditFile {
            path,
            old_text,
            new_text,
        } => Some(format!(
            "EDIT {path}\nREMOVE:\n{old_text}\nINSERT:\n{new_text}"
        )),
        Tool::Shell { command } => Some(format!("SHELL (unsandboxed): {command}")),
        _ => None,
    };
    if let Some(preview) = preview {
        match config.approval {
            Approval::ReadOnly => bail!("Denied by read-only policy"),
            Approval::Auto => {}
            Approval::Ask => {
                ensure!(
                    ui.terminal,
                    "Approval required: use a terminal or explicitly set --approval auto"
                );
                ui.event("approval", json!(preview));
                ensure!(
                    ui.line("Allow this operation? [y/N] ")
                        .await?
                        .is_some_and(|s| s.eq_ignore_ascii_case("y")),
                    "User denied operation"
                );
            }
        }
    }
    match tool {
        Tool::ListFiles { path } => files::list(&config.cwd, &path),
        Tool::ReadFile {
            path,
            offset,
            lines,
        } => files::read(&config.cwd, &path, offset, lines),
        Tool::Search { query } => files::search(&config.cwd, &query),
        Tool::WriteFile { path, content } => files::write(&config.cwd, &path, &content),
        Tool::EditFile {
            path,
            old_text,
            new_text,
        } => files::edit(&config.cwd, &path, &old_text, &new_text),
        Tool::Shell { command } => shell::run(&config.cwd, &command, config.tool_timeout).await,
    }
}

pub fn truncate(text: &str, bytes: usize) -> String {
    if text.len() <= bytes {
        return text.to_owned();
    }
    let mut end = bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[truncated; {} bytes total]", &text[..end], text.len())
}
