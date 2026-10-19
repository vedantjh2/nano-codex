//! Let the official auth manager renew its isolated login, including key rotation.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

async fn reply(reader: &mut BufReader<tokio::process::ChildStdout>, id: u64) -> Result<Value> {
    loop {
        let mut line = Vec::new();
        loop {
            let buffer = reader.fill_buf().await?;
            ensure!(!buffer.is_empty(), "Codex auth service disconnected");
            let count = buffer
                .iter()
                .position(|b| *b == b'\n')
                .map_or(buffer.len(), |i| i + 1);
            line.extend_from_slice(&buffer[..count]);
            reader.consume(count);
            ensure!(line.len() < 1_048_576, "Codex auth message too large");
            if line.ends_with(b"\n") {
                break;
            }
        }
        let value: Value = serde_json::from_slice(&line)?;
        if value["id"] == id {
            ensure!(
                value.get("error").is_none(),
                "Codex account RPC failed; run nano-codex login"
            );
            return Ok(value["result"].clone());
        }
    }
}

pub async fn refresh(home: &Path) -> Result<()> {
    let mut command = Command::new("codex");
    command.env("CODEX_HOME", home).args([
        "-c",
        "cli_auth_credentials_store=\"file\"",
        "app-server",
    ]);
    renew(command).await
}

async fn renew(mut command: Command) -> Result<()> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("Could not start Codex's auth manager")?;
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        let initialize = json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"nano_codex","version":"0.1.0"}}});
        input.write_all(format!("{initialize}\n").as_bytes()).await?;
        reply(&mut output, 1).await?;
        let account = json!({"id":2,"method":"account/read","params":{"refreshToken":true}});
        input.write_all(format!("{{\"method\":\"initialized\"}}\n{account}\n").as_bytes()).await?;
        let result = reply(&mut output, 2).await?;
        ensure!(!result["account"].is_null(), "No Codex account; run nano-codex login");
        Ok::<_, anyhow::Error>(())
    }).await.context("Codex token renewal timed out")?;
    let _ = child.kill().await;
    result
}

#[cfg(test)]
#[path = "codex_rpc_tests.rs"]
mod tests;
