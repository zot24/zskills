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

    /// MCP servers — written into the runtime's `mcpServers` map at the chosen scope.
    #[serde(default)]
    pub mcps: Vec<McpEntry>,
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

/// An MCP server declaration in `skills.toml`.
///
/// Exactly one of `command` (stdio) or `url` (http/sse) must be present.
/// `transport` is optional and inferred: `command` → stdio, `url` → http
/// (set `transport = "sse"` explicitly if you need the deprecated SSE shape).
///
/// `env` and `headers` values should use `${VAR}` references — the manifest
/// is meant to be reproducible and shareable, so literal secrets land in the
/// user's shell environment, not in the TOML.
///
/// `scope` controls which file zskills writes to:
/// - `"user"` (default) → `~/.claude.json`
/// - `"project"` → `<cwd>/.mcp.json`
/// - `"local"` → `<cwd>/.claude.local/settings.json`
///
/// `"managed"` is not accepted — that scope is read-only (deployed by IT).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct McpEntry {
    pub name: String,
    /// `"stdio"` | `"http"` | `"sse"`. Inferred if absent.
    #[serde(default)]
    pub transport: Option<String>,
    // stdio fields
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    // http / sse fields
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    /// `"user"` (default) | `"project"` | `"local"`.
    #[serde(default)]
    pub scope: Option<String>,
}

impl McpEntry {
    /// Resolve the transport kind, preferring an explicit `transport` value
    /// and falling back to inference from `command` (→ stdio) or `url` (→ http).
    pub fn transport_kind(&self) -> &'static str {
        match self.transport.as_deref() {
            Some("stdio") => "stdio",
            Some("http") => "http",
            Some("sse") => "sse",
            _ if self.command.is_some() => "stdio",
            _ if self.url.is_some() => "http",
            _ => "stdio", // fallback — validate() will catch the missing fields
        }
    }

    /// Resolve the scope, defaulting to `"user"`. Returns an error if invalid.
    pub fn scope_kind(&self) -> Result<&'static str> {
        match self.scope.as_deref().unwrap_or("user") {
            "user" => Ok("user"),
            "project" => Ok("project"),
            "local" => Ok("local"),
            "managed" => anyhow::bail!(
                "scope=managed is not writable — managed settings are deployed by IT, not zskills"
            ),
            other => anyhow::bail!(
                "unknown scope {:?} (must be user, project, or local)",
                other
            ),
        }
    }

    /// Validate that the entry has the required fields for its transport.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            anyhow::bail!("mcp entry missing required field `name`");
        }
        self.scope_kind()?;
        match self.transport_kind() {
            "stdio" => {
                if self.command.is_none() {
                    anyhow::bail!("mcp `{}`: stdio transport requires `command`", self.name);
                }
                if self.url.is_some() {
                    anyhow::bail!(
                        "mcp `{}`: stdio entry has stray `url` — pick one transport",
                        self.name
                    );
                }
            }
            "http" | "sse" => {
                if self.url.is_none() {
                    anyhow::bail!("mcp `{}`: http/sse transport requires `url`", self.name);
                }
                if self.command.is_some() {
                    anyhow::bail!(
                        "mcp `{}`: http/sse entry has stray `command` — pick one transport",
                        self.name
                    );
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Convert to the JSON shape the runtime expects under `mcpServers["<name>"]`.
    pub fn to_json_value(&self) -> serde_json::Value {
        use serde_json::{json, Map, Value};
        let mut obj = Map::new();
        match self.transport_kind() {
            "stdio" => {
                obj.insert(
                    "command".into(),
                    json!(self.command.clone().unwrap_or_default()),
                );
                if !self.args.is_empty() {
                    obj.insert("args".into(), json!(self.args));
                }
                if !self.env.is_empty() {
                    obj.insert("env".into(), json!(self.env));
                }
            }
            kind @ ("http" | "sse") => {
                obj.insert("type".into(), json!(kind));
                obj.insert("url".into(), json!(self.url.clone().unwrap_or_default()));
                if !self.headers.is_empty() {
                    obj.insert("headers".into(), json!(self.headers));
                }
            }
            _ => unreachable!(),
        }
        Value::Object(obj)
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
