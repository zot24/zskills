//! Unified interactive picker: fzf when available, dialoguer otherwise.
//!
//! fzf is auto-detected via `$PATH`. Set `ZSKILLS_NO_FZF=1` to force the
//! dialoguer path even when fzf is installed.

use anyhow::Result;

pub struct Item {
    pub label: String,
    pub description: String,
}

impl Item {
    pub fn new(label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
        }
    }
}

pub fn fzf_available() -> bool {
    which::which("fzf").is_ok()
        && !std::env::var("ZSKILLS_NO_FZF").is_ok_and(|v| !v.is_empty())
}

/// Prompt the user to pick one item. Returns `None` if cancelled.
pub fn pick_one(prompt: &str, items: &[Item]) -> Result<Option<usize>> {
    if items.is_empty() {
        return Ok(None);
    }
    if fzf_available() {
        pick_one_fzf(prompt, items)
    } else {
        pick_one_dialoguer(prompt, items)
    }
}

/// Prompt the user to pick zero or more items.
pub fn pick_many(prompt: &str, items: &[Item]) -> Result<Vec<usize>> {
    if items.is_empty() {
        return Ok(vec![]);
    }
    if fzf_available() {
        pick_many_fzf(prompt, items)
    } else {
        pick_many_dialoguer(prompt, items)
    }
}

fn display(item: &Item) -> String {
    if item.description.is_empty() {
        item.label.clone()
    } else {
        format!("{}  — {}", item.label, item.description)
    }
}

/// Spawn fzf with the given args, write `lines` to its stdin, return selected lines
/// (empty Vec == user cancelled).
fn fzf_pick(fzf_args: &[&str], lines: &[String]) -> Result<Vec<String>> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let input = lines.join("\n");
    let mut child = Command::new("fzf")
        .args(fzf_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin.write_all(input.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Ok(vec![]);
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

fn pick_one_fzf(prompt: &str, items: &[Item]) -> Result<Option<usize>> {
    let lines: Vec<String> = items.iter().map(display).collect();
    let fzf_prompt = format!("{}> ", prompt);
    let selected = fzf_pick(
        &["--prompt", &fzf_prompt, "--height=40%", "--reverse"],
        &lines,
    )?;
    Ok(selected
        .first()
        .and_then(|sel| lines.iter().position(|l| l == sel)))
}

fn pick_many_fzf(prompt: &str, items: &[Item]) -> Result<Vec<usize>> {
    let lines: Vec<String> = items.iter().map(display).collect();
    let fzf_prompt = format!("{}> ", prompt);
    let selected = fzf_pick(
        &[
            "--multi",
            "--prompt",
            &fzf_prompt,
            "--height=40%",
            "--reverse",
        ],
        &lines,
    )?;
    Ok(selected
        .iter()
        .filter_map(|sel| lines.iter().position(|l| l == sel))
        .collect())
}

fn pick_one_dialoguer(prompt: &str, items: &[Item]) -> Result<Option<usize>> {
    use dialoguer::FuzzySelect;
    let labels: Vec<String> = items.iter().map(display).collect();
    Ok(FuzzySelect::new()
        .with_prompt(prompt)
        .items(&labels)
        .interact_opt()?)
}

fn pick_many_dialoguer(prompt: &str, items: &[Item]) -> Result<Vec<usize>> {
    use dialoguer::MultiSelect;
    let labels: Vec<String> = items.iter().map(display).collect();
    Ok(MultiSelect::new()
        .with_prompt(prompt)
        .items(&labels)
        .interact_opt()?
        .unwrap_or_default())
}
