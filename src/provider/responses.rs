//! OpenAI: replay complete output items, including reasoning, with matching call IDs.
use super::response_state::State;
use super::sse::Decoder;
use crate::{
    protocol::{Completion, Message, ToolCall},
    tools,
    ui::Ui,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

pub fn request(model: &str, instructions: &str, messages: &[Message], chatgpt: bool) -> Value {
    let mut input = Vec::new();
    for message in messages {
        match message {
            Message::User { content } => input.push(json!({"role":"user","content":content})),
            Message::Tool { call_id, content } => input
                .push(json!({"type":"function_call_output","call_id":call_id,"output":content})),
            Message::Assistant { items, .. } => input.extend(items.iter().cloned()),
        }
    }
    let definitions = tools::definitions();
    // The ChatGPT plan route requires function tools inside a namespace.
    let tools = if chatgpt {
        vec![
            json!({"type":"namespace","name":"nano_codex","description":"Workspace tools","tools":definitions}),
        ]
    } else {
        definitions
    };
    json!({"model":model,"instructions":instructions,"input":input,"tools":tools,
        "store":false,"stream":true,"include":["reasoning.encrypted_content"]})
}

pub fn completed(response: &Value) -> Result<Completion> {
    ensure!(
        response["status"] == "completed",
        "Response was not completed"
    );
    let items = response["output"]
        .as_array()
        .context("Missing response output")?
        .clone();
    let mut completion = Completion {
        items: items.clone(),
        usage: response["usage"].clone(),
        ..Completion::default()
    };
    for item in items {
        match item["type"].as_str() {
            Some("function_call") => completion.calls.push(ToolCall {
                id: item["call_id"].as_str().context("Missing call_id")?.into(),
                name: item["name"]
                    .as_str()
                    .context("Missing function name")?
                    .into(),
                arguments: item["arguments"]
                    .as_str()
                    .context("Missing function arguments")?
                    .into(),
            }),
            Some("message") => {
                for part in item["content"]
                    .as_array()
                    .context("Missing message content")?
                {
                    if let Some(text) = part["text"].as_str().or_else(|| part["refusal"].as_str()) {
                        completion.text.push_str(text);
                    }
                }
            }
            Some("reasoning") => {}
            other => bail!("Unsupported Responses output item: {other:?}"),
        }
    }
    completion.validate()?;
    Ok(completion)
}

pub async fn consume(mut response: reqwest::Response, ui: &Ui) -> Result<Completion> {
    let mut decoder = Decoder::default();
    let mut state = State::default();
    let mut streamed_bytes = 0;
    while let Some(chunk) = response.chunk().await? {
        streamed_bytes += chunk.len();
        ensure!(
            streamed_bytes <= 16_777_216,
            "Response stream exceeds 16 MiB"
        );
        for event in decoder.feed(&chunk)? {
            let event: Value = serde_json::from_str(&event)?;
            if let Some(response) = state.push(&event)? {
                return completed(&response);
            }
            match event["type"].as_str() {
                Some("response.output_text.delta" | "response.refusal.delta") => {
                    ui.delta(event["delta"].as_str().context("Missing text delta")?);
                }
                Some("response.failed" | "response.incomplete" | "error") => bail!(
                    "OpenAI stream failed: {}",
                    tools::truncate(&event.to_string(), 2000)
                ),
                _ => {}
            }
        }
    }
    bail!("Disconnected before response.completed; partial tool calls were discarded")
}
