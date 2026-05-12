//! Thin wrappers over `git` CLI. We shell out (not `git2`) to keep the binary small
//! and reuse the user's existing credential helpers.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub fn clone(url: &str, dest: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(dest)
        .status()
        .context("running git clone (is git installed?)")?;
    anyhow::ensure!(status.success(), "git clone failed for {}", url);
    Ok(())
}

pub fn pull(repo: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["pull", "--ff-only"])
        .status()
        .context("running git pull")?;
    anyhow::ensure!(status.success(), "git pull failed in {}", repo.display());
    Ok(())
}

#[allow(dead_code)]
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
