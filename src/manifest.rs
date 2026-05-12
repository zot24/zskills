//! skills.toml — declarative manifest of "what should be installed."
//!
//! Example:
//!
//! ```toml
//! [[skills]]
//! name = "umbrel-app"
//! marketplace = "zot24-skills"
//!
//! [[skills]]
//! name = "github"
//! marketplace = "claude-plugins-official"
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Manifest {
    /// Claude Code plugins (managed via marketplaces, written to settings.json -> enabledPlugins).
    #[serde(default)]
    pub skills: Vec<SkillEntry>,

    /// Agent Skills (the older raw-SKILL.md format, installed into ~/.claude/skills/).
    #[serde(default)]
    pub agent_skills: Vec<AgentSkillEntry>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SkillEntry {
    pub name: String,
    #[serde(default)]
    pub marketplace: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

impl SkillEntry {
    pub fn qualified(&self) -> Option<String> {
        self.marketplace
            .as_ref()
            .map(|m| format!("{}@{}", self.name, m))
    }
}

/// An Agent Skill declaration.
///
/// `source` is an `owner/repo` (GitHub) or a full git URL. The repo can contain
/// multiple skills under `skills/<name>/SKILL.md`. If `name` is omitted, every
/// skill found under `skills/` in the source repo gets installed.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentSkillEntry {
    pub source: String,
    #[serde(default)]
    pub name: Option<String>,
}

pub fn discover() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?.join("skills.toml");
    if cwd.exists() {
        return Some(cwd);
    }
    // XDG-style first: $XDG_CONFIG_HOME/zskills/skills.toml, then ~/.config/zskills/skills.toml.
    // Most CLI tools (cargo, starship, atuin) use ~/.config on macOS too rather than
    // dirs::config_dir()'s ~/Library/Application Support default.
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home_cfg = xdg
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?
        .join("zskills")
        .join("skills.toml");
    if home_cfg.exists() {
        return Some(home_cfg);
    }
    // Fall back to the platform default if a user has actively chosen that location.
    let platform_cfg = dirs::config_dir()?.join("zskills").join("skills.toml");
    if platform_cfg.exists() {
        return Some(platform_cfg);
    }
    None
}

pub fn load(path: &Path) -> Result<Manifest> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading manifest {}", path.display()))?;
    let m: Manifest =
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    Ok(m)
}
