//! Kill the whole process group on timeout/cancellation; drain both pipes boundedly.
use super::OUTPUT_BYTES;
use anyhow::{Context, Result};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

struct ProcessGroup(u32);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: negative PID addresses the group we created, never our own group.
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

async fn capture(mut pipe: impl AsyncRead + Unpin) -> std::io::Result<String> {
    let mut kept = Vec::new();
    let mut total = 0;
    let mut buffer = [0; 4096];
    loop {
        let count = pipe.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        total += count;
        let retain = count.min(OUTPUT_BYTES.saturating_sub(kept.len()));
        kept.extend_from_slice(&buffer[..retain]);
    }
    let mut output = String::from_utf8_lossy(&kept).into_owned();
    if total > kept.len() {
        output.push_str(&format!("\n[truncated; {total} bytes total]"));
    }
    Ok(output)
}

pub async fn run(cwd: &Path, command: &str, seconds: u64) -> Result<String> {
    let mut process = Command::new("/bin/sh");
    process
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("OPENAI_API_KEY")
        .env_remove("NANO_CODEX_API_KEY");
    #[cfg(unix)]
    process.process_group(0);
    let mut child = process.spawn().context("Could not start /bin/sh")?;
    let _group = ProcessGroup(child.id().context("Missing child PID")?);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let completed = tokio::time::timeout(Duration::from_secs(seconds), async {
        tokio::try_join!(child.wait(), capture(stdout), capture(stderr))
    })
    .await;
    match completed {
        Ok(result) => {
            let (status, stdout, stderr) = result?;
            Ok(format!(
                "exit={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                status.code().map_or("signal".into(), |c| c.to_string())
            ))
        }
        Err(_) => anyhow::bail!(
            "Command timed out after {seconds}s; process group terminated. Changes already made may remain."
        ),
    }
}
