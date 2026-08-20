//! Delegation to the `claude` CLI for state that Claude Code alone owns.
//!
//! zskills can flip `enabledPlugins` in settings.json by itself, but a flipped flag is
//! not an install: the plugin bytes under `~/.claude/plugins/cache/` are fetched, versioned
//! and inventoried by Claude Code. Writing the flag and nothing else leaves the user in the
//! "enabled but not installed" state that `zskills doctor` then reports as a defect — a
//! defect zskills itself created. So after recording intent we hand the fetch to
//! `claude plugin install <name>@<marketplace> -s user`, which is non-interactive.
//!
//! Escape hatches:
//! - `ZSKILLS_CLAUDE_BIN` — path to the binary to invoke (also used by the test suite to
//!   substitute a recording stub).
//! - `ZSKILLS_NO_CLAUDE_CLI=1` — never shell out; fall back to reporting the pending state.

use anyhow::Result;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long to wait for `claude plugin install` before killing it. A stalled fetch
/// (network, auth, a prompt we can't answer) must not hang zskills forever with no
/// output. Override with `ZSKILLS_CLAUDE_TIMEOUT_SECS`.
const DEFAULT_TIMEOUT_SECS: u64 = 180;

fn timeout() -> Duration {
    let secs = std::env::var("ZSKILLS_CLAUDE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    Duration::from_secs(secs.max(1))
}

/// Where the `claude` binary is, or why we're not going to use it.
pub fn binary() -> Option<PathBuf> {
    if std::env::var("ZSKILLS_NO_CLAUDE_CLI").is_ok_and(|v| v != "0" && !v.is_empty()) {
        return None;
    }
    if let Ok(p) = std::env::var("ZSKILLS_CLAUDE_BIN") {
        if p.is_empty() {
            return None;
        }
        return Some(PathBuf::from(p));
    }
    which::which("claude").ok()
}

/// Outcome of trying to materialize one plugin.
#[derive(Debug)]
pub enum Outcome {
    /// `claude plugin install` exited 0.
    Installed,
    /// No usable `claude` binary — intent is recorded, bytes are not fetched.
    NoClaudeCli,
    /// The command ran and failed, timed out, or exited 0 without producing an
    /// inventory entry. Carries a short explanation.
    Failed(String),
}

/// Run `claude plugin install <qualified> -s <scope>`.
///
/// `qualified` must already be `name@marketplace`; we don't let Claude Code re-resolve a
/// bare name, because an ambiguous name would silently install from a different tap than
/// the one zskills resolved against.
pub fn install_plugin(qualified: &str, scope: &str) -> Outcome {
    // Marketplace manifests are third-party input. A plugin named `--help` (or
    // anything leading with `-`) would be read by `claude` as a flag rather than a
    // positional, turning an install into some other command entirely. There is no
    // shell here so nothing can be injected, but argv position still matters.
    if qualified.starts_with('-') {
        return Outcome::Failed(format!(
            "refusing to pass a plugin name that starts with '-': {:?}",
            qualified
        ));
    }
    if !qualified.contains('@') {
        return Outcome::Failed(format!(
            "refusing to install an unqualified plugin name: {:?} (expected name@marketplace)",
            qualified
        ));
    }
    let Some(bin) = binary() else {
        return Outcome::NoClaudeCli;
    };

    let mut cmd = Command::new(&bin);
    cmd.args(["plugin", "install", qualified, "-s", scope])
        // zskills resolves everything under CLAUDE_HOME; Claude Code reads
        // CLAUDE_CONFIG_DIR. Without this, a non-default CLAUDE_HOME means we write
        // the enable to one tree and the child writes the bytes to another, orphaning
        // the enable permanently.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(home) = crate::paths::claude_home() {
        cmd.env("CLAUDE_CONFIG_DIR", home);
    }

    let out = match run_with_timeout(cmd, timeout()) {
        Ok(o) => o,
        Err(e) => return Outcome::Failed(format!("could not run {}: {}", bin.display(), e)),
    };

    let Some(status) = out.status else {
        return Outcome::Failed(format!(
            "timed out after {}s and was killed",
            timeout().as_secs()
        ));
    };
    if !status.success() {
        let mut msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
        return Outcome::Failed(last_lines(&msg, 3));
    }

    // Exit 0 is a claim, not proof. The whole point of this change is that an enable
    // without bytes is a defect, so confirm the bytes were actually inventoried before
    // reporting success — otherwise we recreate the very lie we set out to remove.
    match is_materialized(qualified) {
        Ok(true) => Outcome::Installed,
        Ok(false) => Outcome::Failed(
            "claude reported success but no entry appeared in installed_plugins.json".to_string(),
        ),
        Err(e) => Outcome::Failed(format!("could not verify the install landed: {}", e)),
    }
}

struct Captured {
    /// `None` means the child was killed after exceeding the timeout.
    status: Option<std::process::ExitStatus>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Run `cmd` to completion or kill it at `limit`.
///
/// Both pipes are drained on their own threads: polling `try_wait` while a child
/// fills a pipe buffer we never read is a deadlock, not a timeout.
fn run_with_timeout(mut cmd: Command, limit: Duration) -> std::io::Result<Captured> {
    let mut child = cmd.spawn()?;
    let mut so = child.stdout.take();
    let mut se = child.stderr.take();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = so.as_mut() {
            s.read_to_end(&mut buf).ok();
        }
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = se.as_mut() {
            s.read_to_end(&mut buf).ok();
        }
        buf
    });

    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait()? {
            Some(st) => break Some(st),
            None => {
                if Instant::now() >= deadline {
                    child.kill().ok();
                    child.wait().ok();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };

    // Only join when the child exited on its own. After a kill, grandchildren can
    // still hold the write end of the pipe (`sh -c` dying does not kill its `sleep`),
    // so a join here would block for exactly as long as the timeout was meant to
    // avoid. Let the readers be orphaned; they die with the process.
    let (stdout, stderr) = match status {
        Some(_) => (
            out_thread.join().unwrap_or_default(),
            err_thread.join().unwrap_or_default(),
        ),
        None => (Vec::new(), Vec::new()),
    };

    Ok(Captured {
        status,
        stdout,
        stderr,
    })
}

/// Keep error output to something that fits in a terminal line or three.
fn last_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return "(no output)".to_string();
    }
    lines[lines.len().saturating_sub(n)..].join("; ")
}

/// Is `qualified` present in Claude Code's own inventory (i.e. bytes fetched)?
pub fn is_materialized(qualified: &str) -> Result<bool> {
    let inv = crate::inventory::load(&crate::paths::installed_plugins_json()?)?;
    Ok(crate::inventory::plugins(&inv).is_some_and(|p| p.contains_key(qualified)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_plugin_refuses_flag_shaped_names() {
        // Rejected before any process is spawned, so this is safe without a stub.
        assert!(matches!(
            install_plugin("--help@mp", "user"),
            Outcome::Failed(_)
        ));
    }

    #[test]
    fn install_plugin_refuses_unqualified_names() {
        assert!(matches!(install_plugin("bare", "user"), Outcome::Failed(_)));
    }

    #[test]
    fn last_lines_trims_to_the_tail() {
        assert_eq!(last_lines("a\nb\nc\nd", 2), "c; d");
        assert_eq!(last_lines("only", 3), "only");
        assert_eq!(last_lines("\n\n  \n", 3), "(no output)");
    }
}
