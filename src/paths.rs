use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn claude_home() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CLAUDE_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".claude"))
}

pub fn settings_json() -> Result<PathBuf> {
    Ok(claude_home()?.join("settings.json"))
}

/// `~/.claude.json` — Claude Code's user-scope state file, sibling of `~/.claude/`.
/// This is where `claude mcp add --scope user` writes its `mcpServers` map, distinct
/// from `~/.claude/settings.json`. Both can contain `mcpServers`; zskills reads both.
pub fn claude_json() -> Result<PathBuf> {
    let home = claude_home()?;
    let parent = home
        .parent()
        .context("claude_home has no parent directory")?;
    Ok(parent.join(".claude.json"))
}

pub fn plugins_dir() -> Result<PathBuf> {
    Ok(claude_home()?.join("plugins"))
}

pub fn installed_plugins_json() -> Result<PathBuf> {
    Ok(plugins_dir()?.join("installed_plugins.json"))
}

pub fn known_marketplaces_json() -> Result<PathBuf> {
    Ok(plugins_dir()?.join("known_marketplaces.json"))
}

pub fn marketplaces_dir() -> Result<PathBuf> {
    Ok(plugins_dir()?.join("marketplaces"))
}

pub fn marketplace_manifest(name: &str) -> Result<PathBuf> {
    Ok(marketplaces_dir()?
        .join(name)
        .join(".claude-plugin")
        .join("marketplace.json"))
}

/// `~/.agents/` — the cross-client agent home from the Agent Skills spec,
/// sibling of `~/.claude/`. Override with `AGENTS_HOME` (mirrors `CLAUDE_HOME`).
pub fn agents_home() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("AGENTS_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".agents"))
}

/// ~/.agents/skills/ — the cross-client Agent Skills location per the
/// [Agent Skills spec](https://agentskills.io/integrate-skills). Skills installed here are
/// visible to any compliant client (Claude Code, Grok CLI, …), not just Claude.
pub fn user_skills_dir() -> Result<PathBuf> {
    Ok(agents_home()?.join("skills"))
}

/// ~/.agents/skills/.zskills.json — our inventory of which Agent Skills we manage and where they came from.
pub fn agent_skills_inventory() -> Result<PathBuf> {
    Ok(user_skills_dir()?.join(".zskills.json"))
}

/// Cache for cloned agent-skill source repos.
pub fn agent_skills_cache_dir() -> Result<PathBuf> {
    let base = if let Ok(p) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(p)
    } else {
        dirs::home_dir()
            .context("could not determine home directory")?
            .join(".cache")
    };
    Ok(base.join("zskills").join("agent-skills"))
}
