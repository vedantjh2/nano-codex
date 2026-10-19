//! A locked JSONL journal is the source of truth. Persist calls BEFORE execution.
use crate::{config::Config, protocol::Message};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Record {
    Header {
        version: u8,
        cwd: PathBuf,
        provider: String,
        model: String,
        base_url: String,
    },
    Message {
        message: Message,
    },
    Compacted {
        messages: Vec<Message>,
    },
    Usage {
        usage: Value,
    },
}

pub struct Session {
    file: File,
    pub id: String,
    pub messages: Vec<Message>,
}

impl Session {
    pub fn open(config: &Config, resume: Option<&str>) -> Result<Self> {
        let id = resume
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        uuid::Uuid::parse_str(&id).context("Session ID must be a UUID")?;
        let directory = config.cwd.join(".nano-codex/sessions");
        for path in [config.cwd.join(".nano-codex"), directory.clone()] {
            ensure!(
                !fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()),
                "Session directories may not be symlinks"
            );
            fs::create_dir_all(&path)?;
        }
        let mut options = OpenOptions::new();
        options.read(true).append(true);
        if resume.is_none() {
            options.create_new(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(directory.join(format!("{id}.jsonl")))?;
        file.try_lock_exclusive()
            .context("Session is already open in another process")?;
        let mut journal = Vec::new();
        file.read_to_end(&mut journal)?;
        let mut session = Self {
            file,
            id,
            messages: Vec::new(),
        };
        if resume.is_none() {
            session.record(&Record::Header {
                version: 1,
                cwd: config.cwd.clone(),
                provider: format!("{:?}", config.provider),
                model: config.model.clone(),
                base_url: config.base_url.clone(),
            })?;
        } else {
            // A crash may leave a partial final record. Discard only that suffix.
            let complete = journal
                .iter()
                .rposition(|b| *b == b'\n')
                .map_or(0, |i| i + 1);
            session.file.set_len(complete as u64)?;
            let journal = std::str::from_utf8(&journal[..complete])?;
            let mut records = journal.lines();
            let header: Record =
                serde_json::from_str(records.next().context("Missing session header")?)?;
            match header {
                Record::Header {
                    version,
                    cwd,
                    provider,
                    model,
                    base_url,
                } => {
                    ensure!(
                        version == 1
                            && cwd == config.cwd
                            && provider == format!("{:?}", config.provider)
                            && model == config.model
                            && base_url == config.base_url,
                        "Resume with the original workspace, provider, endpoint, and model"
                    );
                }
                _ => anyhow::bail!("Invalid session header"),
            }
            for line in records {
                match serde_json::from_str(line).context("Corrupt session journal")? {
                    Record::Message { message } => session.messages.push(message),
                    Record::Compacted { messages } => session.messages = messages,
                    Record::Usage { .. } => {}
                    Record::Header { .. } => anyhow::bail!("Unexpected second header"),
                }
            }
            session.repair_pending()?;
        }
        Ok(session)
    }

    fn record(&mut self, record: &Record) -> Result<()> {
        let mut encoded = serde_json::to_vec(record)?;
        encoded.push(b'\n');
        self.file.write_all(&encoded)?;
        self.file.sync_data()?;
        Ok(())
    }

    pub fn push(&mut self, message: Message) -> Result<()> {
        self.record(&Record::Message {
            message: message.clone(),
        })?;
        self.messages.push(message);
        Ok(())
    }

    pub fn usage(&mut self, usage: Value) -> Result<()> {
        self.record(&Record::Usage { usage })
    }

    pub fn compact(&mut self, messages: Vec<Message>) -> Result<()> {
        self.record(&Record::Compacted {
            messages: messages.clone(),
        })?;
        self.messages = messages;
        Ok(())
    }

    pub fn repair_pending(&mut self) -> Result<()> {
        let mut pending = BTreeSet::new();
        for message in &self.messages {
            match message {
                Message::Assistant { calls, .. } => {
                    ensure!(
                        pending.is_empty(),
                        "Assistant appeared before all tool results"
                    );
                    for call in calls {
                        ensure!(pending.insert(call.id.clone()), "Duplicate call ID");
                    }
                }
                Message::Tool { call_id, .. } => {
                    ensure!(pending.remove(call_id), "Orphan tool result");
                }
                Message::User { .. } => {
                    ensure!(pending.is_empty(), "User appeared before all tool results")
                }
            }
        }
        for call_id in pending {
            self.push(Message::Tool { call_id, content: "Interrupted before a durable result. The operation may have run partially or fully. Inspect the workspace before retrying; do not blindly repeat it.".into() })?;
        }
        Ok(())
    }
}
