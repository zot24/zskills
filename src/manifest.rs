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
    #[serde(default)]
    pub skills: Vec<SkillEntry>,
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

pub fn discover() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?.join("skills.toml");
    if cwd.exists() {
        return Some(cwd);
    }
    let home_cfg = dirs::config_dir()?.join("zskills").join("skills.toml");
    if home_cfg.exists() {
        return Some(home_cfg);
    }
    None
}

pub fn load(path: &Path) -> Result<Manifest> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading manifest {}", path.display()))?;
    let m: Manifest = toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    Ok(m)
}
