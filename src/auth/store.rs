//! One active account, protected by a process lock and atomic 0600 credential writes.
use crate::config;
use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
};

#[derive(Serialize, Deserialize)]
pub struct Account {
    pub client_id: String,
    pub subject: String,
    pub email: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub scope: String,
    pub expires_at: u64,
}

pub struct Store {
    directory: PathBuf,
    _lock: File,
}

fn private_file(path: &std::path::Path, create_new: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

impl Store {
    pub fn open() -> Result<Self> {
        let directory = config::home()?;
        fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        let lock = private_file(&directory.join("auth.lock"), false)?;
        lock.try_lock_exclusive().context(
            "Another nano-codex process is using ChatGPT credentials; try again shortly",
        )?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }

    pub fn load(&self) -> Result<Option<Account>> {
        let path = self.directory.join("auth.json");
        match fs::read(path) {
            Ok(bytes) => Ok(Some(
                serde_json::from_slice(&bytes).context("Invalid nano-codex auth.json")?,
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, account: &Account) -> Result<()> {
        crate::storage::write_private(
            &self.directory.join("auth.json"),
            &serde_json::to_vec(account)?,
        )
    }

    pub fn host_id(&self) -> Result<String> {
        let path = self.directory.join("host-id");
        if path.exists() {
            return Ok(fs::read_to_string(path)?);
        }
        let id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
        let mut file = private_file(&path, true)?;
        file.write_all(id.as_bytes())?;
        file.sync_all()?;
        Ok(id)
    }
}
