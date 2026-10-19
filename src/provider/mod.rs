//! Two wire adapters share a transport, not a sprawling provider framework.
pub mod chat;
mod response_state;
pub mod responses;
pub mod sse;

use crate::{
    auth,
    config::{Config, ProviderKind},
    protocol::{Completion, Message},
    tools,
    ui::Ui,
};
use anyhow::{Context, Result, ensure};
use reqwest::{Client, Response};
use serde_json::Value;
use std::time::Duration;

pub struct Provider {
    client: Client,
    kind: ProviderKind,
    base_url: String,
    model: String,
}

impl Provider {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(180))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("nano-codex/0.1")
                .build()?,
            kind: config.provider,
            base_url: config.base_url.clone(),
            model: config.model.clone(),
        })
    }

    pub async fn complete(
        &self,
        instructions: &str,
        messages: &[Message],
        ui: &Ui,
    ) -> Result<Completion> {
        let local = matches!(self.kind, ProviderKind::Vllm | ProviderKind::Sglang);
        let body = if local {
            chat::request(&self.model, instructions, messages)
        } else {
            responses::request(
                &self.model,
                instructions,
                messages,
                self.kind == ProviderKind::Chatgpt,
            )
        };
        let endpoint = if local {
            "chat/completions"
        } else {
            "responses"
        };
        let response = self.send(endpoint, Some(body)).await?;
        let completion = if local {
            chat::consume(response, ui).await?
        } else {
            responses::consume(response, ui).await?
        };
        completion.validate()?;
        Ok(completion)
    }

    pub async fn models(&self) -> Result<Vec<String>> {
        let endpoint = if self.kind == ProviderKind::Codex {
            format!("models?client_version={}", auth::codex::API_VERSION)
        } else {
            "models".into()
        };
        let value: Value = self.send(&endpoint, None).await?.json().await?;
        let models = value
            .get("data")
            .or_else(|| value.get("models"))
            .and_then(Value::as_array)
            .context("Missing model catalog")?;
        Ok(models
            .iter()
            .filter(|model| model["visibility"] != "hide")
            .filter_map(|model| model.get("id").or_else(|| model.get("slug")))
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect())
    }

    async fn send(&self, endpoint: &str, body: Option<Value>) -> Result<Response> {
        let headers = auth::headers(self.kind, &self.client).await?;
        for attempt in 0..3 {
            let url = format!("{}/{endpoint}", self.base_url);
            let request = if let Some(body) = &body {
                self.client
                    .post(url)
                    .header("accept", "text/event-stream")
                    .json(body)
            } else {
                self.client.get(url)
            };
            let mut response = request
                .headers(headers.clone())
                .send()
                .await
                .context("Provider request failed")?;
            let status = response.status();
            let retryable = matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504);
            if retryable && attempt < 2 {
                let delay = response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|h| h.parse::<u64>().ok())
                    .unwrap_or(1 << attempt)
                    .min(5);
                tokio::time::sleep(Duration::from_secs(delay)).await;
                continue;
            }
            if !status.is_success() {
                let mut bytes = Vec::new();
                while bytes.len() < 2000 {
                    let Some(chunk) = response.chunk().await? else {
                        break;
                    };
                    let count = chunk.len().min(2000 - bytes.len());
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let message = String::from_utf8_lossy(&bytes);
                anyhow::bail!(
                    "Provider HTTP {status}: {}",
                    tools::truncate(&message, 2000)
                );
            }
            if body.is_some() {
                let content_type = response
                    .headers()
                    .get("content-type")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("");
                ensure!(
                    content_type.is_empty() || content_type.starts_with("text/event-stream"),
                    "Expected an SSE stream, got {content_type}"
                );
            }
            return Ok(response);
        }
        unreachable!()
    }
}
