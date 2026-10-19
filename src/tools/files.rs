use super::{FILE_BYTES, OUTPUT_BYTES, paths::resolve, truncate};
use anyhow::{Result, ensure};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn text(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    ensure!(file.metadata()?.is_file(), "Expected a regular file");
    let mut bytes = Vec::new();
    file.take(FILE_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= FILE_BYTES,
        "File exceeds 1 MiB; use a bounded shell command"
    );
    Ok(String::from_utf8(bytes)?)
}

pub fn list(root: &Path, path: &str) -> Result<String> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(resolve(root, path)?)?.take(1001) {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == ".nano-codex" {
            continue;
        }
        entries.push(format!(
            "{name}{}",
            if entry.file_type()?.is_dir() { "/" } else { "" }
        ));
    }
    entries.sort();
    Ok(truncate(&entries.join("\n"), OUTPUT_BYTES))
}

pub fn read(root: &Path, path: &str, offset: usize, lines: usize) -> Result<String> {
    ensure!(
        (1..=1000).contains(&lines),
        "lines must be between 1 and 1000"
    );
    let content = text(&resolve(root, path)?)?;
    Ok(content
        .lines()
        .enumerate()
        .skip(offset)
        .take(lines)
        .map(|(i, line)| format!("{}: {line}\n", i + 1))
        .collect())
}

pub fn search(root: &Path, query: &str) -> Result<String> {
    ensure!(!query.is_empty(), "Search query must not be empty");
    let mut output = String::new();
    let walker = ignore::WalkBuilder::new(root)
        .follow_links(false)
        .filter_entry(|e| e.file_name() != ".nano-codex" && e.file_name() != ".git")
        .sort_by_file_path(|a, b| a.cmp(b))
        .build();
    for entry in walker.take(10_000) {
        let entry = entry?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(content) = text(entry.path()) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            if line.contains(query) {
                output.push_str(&format!(
                    "{}:{}:{line}\n",
                    entry.path().strip_prefix(root)?.display(),
                    i + 1
                ));
                if output.len() >= OUTPUT_BYTES {
                    return Ok(truncate(&output, OUTPUT_BYTES));
                }
            }
        }
    }
    Ok(if output.is_empty() {
        "No matches (search visits at most 10000 entries).".into()
    } else {
        output
    })
}

pub fn write(root: &Path, path: &str, content: &str) -> Result<String> {
    ensure!(content.len() <= FILE_BYTES, "New file exceeds 1 MiB");
    let path = resolve(root, path)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(content.as_bytes())?;
    file.sync_data()?;
    Ok(format!(
        "Created {} ({} bytes)",
        path.strip_prefix(root)?.display(),
        content.len()
    ))
}

pub fn edit(root: &Path, path: &str, old: &str, new: &str) -> Result<String> {
    ensure!(!old.is_empty(), "old_text must not be empty");
    let path = resolve(root, path)?;
    let content = text(&path)?;
    ensure!(
        content.match_indices(old).count() == 1,
        "old_text must match exactly once; read the file and include more context"
    );
    let updated = content.replacen(old, new, 1);
    ensure!(updated.len() <= FILE_BYTES, "Updated file exceeds 1 MiB");
    // Rename prevents partial edits if the process stops while writing.
    let temporary = path.with_file_name(format!(".nano-edit-{}", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| -> Result<()> {
        file.set_permissions(fs::metadata(&path)?.permissions())?;
        file.write_all(updated.as_bytes())?;
        file.sync_data()?;
        fs::rename(&temporary, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(format!("Edited {}", path.strip_prefix(root)?.display()))
}
