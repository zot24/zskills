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

/// ~/.claude/skills/ — where Agent Skills (the older format, raw SKILL.md trees) live.
pub fn user_skills_dir() -> Result<PathBuf> {
    Ok(claude_home()?.join("skills"))
}

/// ~/.claude/skills/.zskills.json — our inventory of which Agent Skills we manage and where they came from.
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
