//! vLLM / SGLang: assemble indexed function deltas before running ANY tool.
use super::sse::Decoder;
use crate::{
    protocol::{Completion, Message, ToolCall},
    tools,
    ui::Ui,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn request(model: &str, instructions: &str, messages: &[Message]) -> Value {
    let mut input = vec![json!({"role":"system","content":instructions})];
    for message in messages {
        input.push(match message {
            Message::User { content } => json!({"role":"user","content":content}),
            Message::Tool { call_id, content } => {
                json!({"role":"tool","tool_call_id":call_id,"content":content})
            }
            Message::Assistant {
                content,
                calls,
                reasoning,
                ..
            } => {
                let mut value = json!({"role":"assistant","content":content});
                if !calls.is_empty() {
                    value["tool_calls"] = json!(
                        calls
                            .iter()
                            .map(|call| json!({"id":call.id,"type":"function",
                        "function":{"name":call.name,"arguments":call.arguments}}))
                            .collect::<Vec<_>>()
                    );
                }
                if let Some(reasoning) = reasoning {
                    value["reasoning_content"] = json!(reasoning);
                }
                value
            }
        });
    }
    let tools: Vec<_> = tools::definitions()
        .into_iter()
        .map(|mut tool| {
            tool.as_object_mut().unwrap().remove("type");
            json!({"type":"function","function":tool})
        })
        .collect();
    json!({"model":model,"messages":input,"tools":tools,"tool_choice":"auto","stream":true,
        "stream_options":{"include_usage":true}})
}

#[derive(Default)]
pub struct Assembly {
    pub completion: Completion,
    calls: BTreeMap<u64, ToolCall>,
    finish: Option<String>,
}

impl Assembly {
    pub fn push(&mut self, event: &Value, ui: &Ui) -> Result<()> {
        if let Some(error) = event.get("error") {
            bail!("Provider stream error: {error}");
        }
        if let Some(usage) = event.get("usage").filter(|u| !u.is_null()) {
            self.completion.usage = usage.clone();
        }
        let Some(choice) = event["choices"].as_array().and_then(|a| a.first()) else {
            return Ok(());
        };
        if let Some(reason) = choice["finish_reason"].as_str() {
            ensure!(
                reason == "stop" || reason == "tool_calls",
                "Model stopped with {reason}; no tools were executed"
            );
            self.finish = Some(reason.into());
        }
        let delta = &choice["delta"];
        if let Some(text) = delta["content"]
            .as_str()
            .or_else(|| delta["refusal"].as_str())
        {
            self.completion.text.push_str(text);
            ui.delta(text);
        }
        if let Some(reasoning) = delta["reasoning_content"]
            .as_str()
            .or_else(|| delta["reasoning"].as_str())
        {
            self.completion
                .reasoning
                .get_or_insert_default()
                .push_str(reasoning);
        }
        if let Some(calls) = delta["tool_calls"].as_array() {
            for delta in calls {
                let index = delta["index"].as_u64().context("Missing tool index")?;
                ensure!(index < 32, "Too many tool calls");
                let call = self.calls.entry(index).or_default();
                if let Some(id) = delta["id"].as_str() {
                    call.id.push_str(id);
                }
                if let Some(name) = delta["function"]["name"].as_str() {
                    call.name.push_str(name);
                }
                if let Some(args) = delta["function"]["arguments"].as_str() {
                    call.arguments.push_str(args);
                }
            }
        }
        let bytes = self.completion.text.len()
            + self.completion.reasoning.as_ref().map_or(0, String::len)
            + self
                .calls
                .values()
                .map(|c| c.arguments.len() + c.name.len() + c.id.len())
                .sum::<usize>();
        ensure!(bytes <= 4_194_304, "Completion exceeds 4 MiB");
        Ok(())
    }

    pub fn finish(mut self) -> Result<Completion> {
        let reason = self
            .finish
            .context("Stream ended without a finish reason")?;
        self.completion.calls = self.calls.into_values().collect();
        ensure!(
            (reason == "tool_calls") == !self.completion.calls.is_empty(),
            "Finish reason disagrees with tool calls"
        );
        self.completion.validate()?;
        Ok(self.completion)
    }
}

pub async fn consume(mut response: reqwest::Response, ui: &Ui) -> Result<Completion> {
    let mut decoder = Decoder::default();
    let mut assembly = Assembly::default();
    while let Some(chunk) = response.chunk().await? {
        for event in decoder.feed(&chunk)? {
            if event == "[DONE]" {
                return assembly.finish();
            }
            assembly.push(&serde_json::from_str(&event)?, ui)?;
        }
    }
    bail!("Disconnected before [DONE]; partial tool calls were discarded")
}
