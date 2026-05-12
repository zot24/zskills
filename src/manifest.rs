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
/// - `source` (optional): `owner/repo` (GitHub) or a full git URL. If present,
///   `sync` will (re)install from upstream. If absent, the skill is treated as
///   local-only: tracked in inventory but never refreshed from a remote.
/// - `name` (optional): pick a specific skill out of a multi-skill repo.
///   For local-only entries (no `source`), `name` is required.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentSkillEntry {
    #[serde(default)]
    pub source: Option<String>,
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

/// Append an [[agent_skills]] entry to a manifest file, preserving existing
/// formatting and comments via toml_edit. Returns true if the entry was added,
/// false if an equivalent entry already exists.
pub fn append_agent_skill(path: &Path, entry: &AgentSkillEntry) -> Result<bool> {
    use toml_edit::{value, Array, ArrayOfTables, DocumentMut, Item, Table};

    let raw = if path.exists() {
        std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest {}", path.display()))?
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        String::new()
    };

    let mut doc: DocumentMut = raw
        .parse()
        .with_context(|| format!("parsing manifest {} as TOML", path.display()))?;

    // Check for duplicates
    if let Some(Item::ArrayOfTables(existing)) = doc.get("agent_skills") {
        for t in existing.iter() {
            let src = t.get("source").and_then(|v| v.as_str()).map(str::to_string);
            let name = t.get("name").and_then(|v| v.as_str()).map(str::to_string);
            if src == entry.source && name == entry.name {
                return Ok(false);
            }
        }
    }

    let aot = match doc
        .entry("agent_skills")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
    {
        Item::ArrayOfTables(a) => a,
        slot => {
            *slot = Item::ArrayOfTables(ArrayOfTables::new());
            match slot {
                Item::ArrayOfTables(a) => a,
                _ => unreachable!(),
            }
        }
    };

    let mut t = Table::new();
    if let Some(src) = &entry.source {
        t["source"] = value(src);
    }
    if let Some(n) = &entry.name {
        t["name"] = value(n);
    }
    let _ = Array::new(); // unused; placate compiler if toml_edit changes shape
    aot.push(t);

    std::fs::write(path, doc.to_string())
        .with_context(|| format!("writing manifest {}", path.display()))?;
    Ok(true)
}
