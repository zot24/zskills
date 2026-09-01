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

/// `git remote get-url origin`, or `None` when `repo` is not a git clone or
/// has no `origin`. Used to refuse reusing a marketplace directory that
/// already holds a different repo.
pub fn origin_url(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!url.is_empty()).then_some(url)
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

/// Fetch refs and tags from `origin` without touching the working tree.
///
/// A pinned marketplace never pulls. It may still need one fetch to learn about a
/// tag it does not have yet, and a fetch cannot move `HEAD`.
///
/// `--force` is required, not cosmetic. When upstream moves a tag, a plain
/// `git fetch --tags` rejects **every** tag with "would clobber existing tag" and exits
/// non-zero, so one moved tag upstream would make an otherwise valid pin unresolvable.
/// The real `llm-wiki` clone is in that state today. Tags are upstream's to define, so
/// taking their version is the right resolution.
pub fn fetch_all(repo: &Path) -> Result<()> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["fetch", "--tags", "--force", "--quiet", "origin"])
        .output()
        .context("running git fetch")?;
    anyhow::ensure!(
        out.status.success(),
        "git fetch failed in {}: {}",
        repo.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

/// Resolve a tag, branch, or sha to a full commit sha, using only what is already
/// in the repository. Returns `None` when the ref is unknown locally.
pub fn resolve_commit(repo: &Path, reference: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{}^{{commit}}", reference))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

/// Check out `sha` as a detached HEAD.
///
/// Detached is deliberate. A pinned marketplace is not tracking a branch, and leaving
/// it on one invites the next `git pull` to move it.
pub fn checkout_detached(repo: &Path, sha: &str) -> Result<()> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["checkout", "--quiet", "--detach", sha])
        .output()
        .context("running git checkout")?;
    anyhow::ensure!(
        out.status.success(),
        "git checkout {} failed in {}: {}",
        sha,
        repo.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
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
