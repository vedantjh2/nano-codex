//! Like the SDK: retain finalized items when the terminal envelope omits output.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct State {
    response_id: Option<String>,
    added: BTreeMap<u64, Value>,
    done: BTreeMap<u64, Value>,
}

fn compatible(before: &Value, after: &Value) -> bool {
    ["type", "id", "call_id", "name", "namespace", "role"]
        .into_iter()
        .all(|field| {
            before
                .get(field)
                .zip(after.get(field))
                .is_none_or(|(a, b)| a == b)
        })
}

impl State {
    pub fn push(&mut self, event: &Value) -> Result<Option<Value>> {
        let kind = event["type"].as_str().unwrap_or("");
        if matches!(
            kind,
            "response.created" | "response.in_progress" | "response.completed"
        ) && let Some(id) = event["response"]["id"].as_str()
        {
            ensure!(
                self.response_id.as_deref().is_none_or(|old| old == id),
                "Response identity changed during the stream"
            );
            self.response_id = Some(id.into());
        }
        if matches!(
            kind,
            "response.output_item.added" | "response.output_item.done"
        ) {
            let index = event["output_index"]
                .as_u64()
                .unwrap_or(self.done.len() as u64);
            ensure!(index < 256, "Too many response items");
            let item = event.get("item").context("Missing streamed output item")?;
            ensure!(item.is_object(), "Invalid streamed output item");
            if let Some(old) = self.added.get(&index) {
                ensure!(compatible(old, item), "Output item identity changed");
            }
            if let Some(old) = self.done.get(&index) {
                ensure!(old == item, "Conflicting finalized output item");
            }
            if kind.ends_with(".added") {
                self.added.insert(index, item.clone());
            } else {
                self.done.insert(index, item.clone());
            }
        }
        if kind != "response.completed" {
            return Ok(None);
        }
        let mut response = event["response"].clone();
        ensure!(response.is_object(), "Missing completed response");
        if let Some(status) = response["status"].as_str() {
            ensure!(status == "completed", "Response was not completed");
        }
        response["status"] = json!("completed");
        let final_items = response["output"].as_array().filter(|a| !a.is_empty());
        if let Some(items) = final_items {
            for (index, old) in self.added.iter().chain(self.done.iter()) {
                let item = items
                    .get(*index as usize)
                    .context("Final response lost an output item")?;
                ensure!(compatible(old, item), "Final output identity changed");
            }
        } else {
            ensure!(
                self.added.keys().all(|i| self.done.contains_key(i)),
                "Cannot complete a response with unfinished items"
            );
            response["output"] = json!(self.done.values().collect::<Vec<_>>());
        }
        Ok(Some(response))
    }
}
