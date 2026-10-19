//! Path checks are useful guardrails, not an OS sandbox or a defense against races.
use anyhow::{Result, ensure};
use std::path::{Component, Path, PathBuf};

pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    ensure!(!path.is_absolute(), "Use a relative workspace path");
    for component in path.components() {
        ensure!(
            matches!(component, Component::Normal(_) | Component::CurDir),
            "Path traversal denied"
        );
        ensure!(
            component.as_os_str() != ".git" && component.as_os_str() != ".nano-codex",
            "Protected harness/git directory"
        );
    }
    let joined = root.join(path);
    // Writes allow a missing leaf, but not a missing parent or symlink escape.
    let checked = if joined.exists() {
        joined.canonicalize()?
    } else {
        joined
            .parent()
            .unwrap_or(root)
            .canonicalize()?
            .join(joined.file_name().unwrap_or_default())
    };
    ensure!(checked.starts_with(root), "Symlink leaves the workspace");
    let relative_checked = checked.strip_prefix(root)?;
    ensure!(
        !relative_checked
            .components()
            .any(|c| c.as_os_str() == ".git" || c.as_os_str() == ".nano-codex"),
        "Symlink enters a protected directory"
    );
    ensure!(
        !std::fs::symlink_metadata(&joined).is_ok_and(|m| m.file_type().is_symlink()),
        "Final path may not be a symlink"
    );
    Ok(checked)
}
