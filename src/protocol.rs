//! The harness speaks these types; only provider adapters know the wire formats.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    User {
        content: String,
    },
    Assistant {
        content: String,
        calls: Vec<ToolCall>,
        // Responses reasoning items (including encrypted state) must survive replay.
        items: Vec<Value>,
        // Some local reasoning models require this on subsequent tool turns.
        reasoning: Option<String>,
    },
    Tool {
        call_id: String,
        content: String,
    },
}

#[derive(Default)]
pub struct Completion {
    pub text: String,
    pub calls: Vec<ToolCall>,
    pub items: Vec<Value>,
    pub reasoning: Option<String>,
    pub usage: Value,
}

impl Completion {
    pub fn validate(&self) -> Result<()> {
        let mut ids = HashSet::new();
        for call in &self.calls {
            ensure!(
                !call.id.is_empty() && !call.name.is_empty(),
                "Incomplete tool call"
            );
            ensure!(ids.insert(&call.id), "Duplicate tool call ID: {}", call.id);
        }
        ensure!(
            !self.text.is_empty() || !self.calls.is_empty(),
            "Model returned no text or tools"
        );
        ensure!(
            self.calls.len() <= 32,
            "Too many tool calls in one completion"
        );
        Ok(())
    }

    pub fn message(self) -> Message {
        Message::Assistant {
            content: self.text,
            calls: self.calls,
            items: self.items,
            reasoning: self.reasoning,
        }
    }
}
