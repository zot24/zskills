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
/// Exactly one of `source`, `npm`, or (for local-only entries) `name` should be set.
///
/// - `source`: `owner/repo` (GitHub) or a git URL. `sync` clones/pulls and copies
///   skills under `skills/<name>/` into `~/.claude/skills/<name>/`.
/// - `npm`: npm package name. `sync` runs `npm install -g <pkg>` and trusts the
///   package's post-install to place files under `~/.claude/skills/`. After install,
///   zskills diffs the directory and tags every new skill with `source: "npm:<pkg>"`.
/// - `install_cmd`: optional override for npm packages with custom setup (e.g.,
///   `"npx some-tool install"`).
/// - `name` (optional): pick a single skill out of a multi-skill repo. For
///   local-only entries (no `source`/`npm`), `name` is required.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AgentSkillEntry {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub npm: Option<String>,
    #[serde(default)]
    pub install_cmd: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// Optional glob patterns matching skill directory names in `~/.claude/skills/`.
    /// After install, every match gets tagged with this entry's source — useful when
    /// the install command updates pre-existing files (no diff) but you want zskills
    /// to take ownership of them. Example: `claims = ["gsd-*"]`.
    #[serde(default)]
    pub claims: Vec<String>,
}

/// Find the user-level manifest. Does NOT look at `./skills.toml` — that path
/// requires explicit `--file` to use, because running `zskills sync` inside an
/// unrelated repo that happens to ship its own skills.toml has destructively
/// surprising consequences (it's the source of a v0.5.0-era data-loss incident).
pub fn discover() -> Option<PathBuf> {
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home_cfg = xdg
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?
        .join("zskills")
        .join("skills.toml");
    if home_cfg.exists() {
        return Some(home_cfg);
    }
    let platform_cfg = dirs::config_dir()?.join("zskills").join("skills.toml");
    if platform_cfg.exists() {
        return Some(platform_cfg);
    }
    None
}

/// Returns Some(path) if `./skills.toml` exists. Used by sync to warn the user
/// that a CWD manifest was *not* loaded.
pub fn cwd_skills_toml() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?.join("skills.toml");
    cwd.exists().then_some(cwd)
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
