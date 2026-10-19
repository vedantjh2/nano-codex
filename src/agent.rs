//! The harness: model -> durable calls -> approved tools -> durable results -> model.
use crate::{
    config::Config, context, protocol::Message, provider::Provider, session::Session, tools, ui::Ui,
};
use anyhow::{Result, bail};
use serde_json::json;
use std::time::Instant;

pub async fn run(
    config: &Config,
    provider: &Provider,
    session: &mut Session,
    ui: &mut Ui,
    prompt: &str,
) -> Result<()> {
    session.repair_pending()?;
    session.push(Message::User {
        content: prompt.into(),
    })?;
    let instructions = context::instructions(config)?;
    for step in 1..=config.max_steps {
        if context::fit(session, &instructions, config.context_bytes)? {
            ui.event("compacted", json!({"messages":session.messages.len()}));
        }
        ui.event("step", json!(step));
        let started = Instant::now();
        let completion = provider
            .complete(&instructions, &session.messages, ui)
            .await?;
        completion.validate()?;
        ui.end_text();
        let calls = completion.calls.clone();
        ui.event(
            "usage",
            json!({"tokens":completion.usage,"elapsed_ms":started.elapsed().as_millis()}),
        );
        session.usage(completion.usage.clone())?;
        session.push(completion.message())?;
        if calls.is_empty() {
            ui.event("done", json!({"session":session.id,"steps":step}));
            return Ok(());
        }
        for call in calls {
            ui.event("tool_call", json!(call));
            let output = tools::execute(&call, config, ui).await;
            ui.event("tool_result", json!({"id":call.id,"output":output}));
            session.push(Message::Tool {
                call_id: call.id,
                content: output,
            })?;
        }
    }
    bail!(
        "Stopped at --max-steps={}. Resume the session to continue.",
        config.max_steps
    )
}
