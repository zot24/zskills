//! Thin wrappers over `git` CLI. We shell out (not `git2`) to keep the binary small
//! and reuse the user's existing credential helpers.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub fn clone(url: &str, dest: &Path) -> Result<()> {
    let out = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(dest)
        .output()
        .context("running git clone (is git installed?)")?;
    anyhow::ensure!(
        out.status.success(),
        "git clone failed for {}: {}",
        url,
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

/// Pull from a git repo. Captures output; only surfaces errors. The destination
/// must be a git working tree — call `is_git_repo` first if uncertain.
pub fn pull(repo: &Path) -> Result<()> {
    if !is_git_repo(repo) {
        anyhow::bail!("{} is not a git working tree", repo.display());
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["pull", "--ff-only", "--quiet"])
        .output()
        .context("running git pull")?;
    anyhow::ensure!(
        out.status.success(),
        "git pull failed in {}: {}",
        repo.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

/// Cheap check whether `<path>/.git` (or `<path>` itself for bare repos) exists.
pub fn is_git_repo(repo: &Path) -> bool {
    repo.join(".git").exists()
}

pub fn head_sha(repo: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .context("running git rev-parse")?;
    anyhow::ensure!(out.status.success(), "git rev-parse failed");
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}
