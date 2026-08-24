//! Which coding harness can see a name.
//!
//! Tokens are harnesses (`claude`, `pi`, `hermes`, `kimi`, `grok`, `codex`),
//! not directories. Pi, Grok, and the Agent Skills spec already scan the
//! shared hub `~/.agents/skills/<name>/`. This module copies a nested
//! marketplace `skills/<name>/` tree into that hub. It does not symlink.
//! Extra per-harness trees are not invented when the hub is enough.

use anyhow::{Context, Result};
use clap::ValueEnum;
use owo_colors::OwoColorize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, ValueEnum)]
pub enum Harness {
    Claude,
    Pi,
    Hermes,
    Kimi,
    Grok,
    Codex,
}

impl Harness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::Hermes => "hermes",
            Self::Kimi => "kimi",
            Self::Grok => "grok",
            Self::Codex => "codex",
        }
    }

    pub fn parse_name(s: &str) -> Result<Self> {
        match s {
            "claude" => Ok(Self::Claude),
            "pi" => Ok(Self::Pi),
            "hermes" => Ok(Self::Hermes),
            "kimi" => Ok(Self::Kimi),
            "grok" => Ok(Self::Grok),
            "codex" => Ok(Self::Codex),
            other => anyhow::bail!(
                "unknown harness {other:?} (must be claude, pi, hermes, kimi, grok, or codex)"
            ),
        }
    }

    /// True when a hub copy at `~/.agents/skills/<name>/SKILL.md` is enough
    /// for this harness to load the skill. Cited from the harness's own docs
    /// or from this crate's Agent Skills path. Hermes and Kimi are not.
    pub fn hub_is_enough(self) -> bool {
        matches!(self, Self::Pi | Self::Grok | Self::Codex)
    }

    pub fn mcp_supported(self) -> bool {
        matches!(self, Self::Claude)
    }

    pub fn mcp_skip_reason(self) -> &'static str {
        match self {
            Self::Claude => "",
            Self::Pi => "no built-in MCP",
            Self::Grok => "runtime CLI owns ~/.grok/config.toml",
            Self::Hermes => "no cited MCP map",
            Self::Kimi => "no cited MCP map",
            Self::Codex => "not writing ~/.codex/config.toml this PR",
        }
    }

    pub fn skill_skip_reason(self) -> Option<&'static str> {
        match self {
            Self::Hermes => {
                Some("unsupported (category tree ~/.hermes/skills/<cat>/<name>/; not writing)")
            }
            Self::Kimi => Some("unsupported (no cited skills directory; not inventing a folder)"),
            _ => None,
        }
    }
}

pub fn parse_names(names: &[String]) -> Result<Vec<Harness>> {
    let mut out = Vec::new();
    for n in names {
        let h = Harness::parse_name(n)?;
        if !out.contains(&h) {
            out.push(h);
        }
    }
    Ok(out)
}

pub fn unique(hs: &[Harness]) -> Vec<Harness> {
    let mut out = Vec::new();
    for h in hs {
        if !out.contains(h) {
            out.push(*h);
        }
    }
    out
}

/// Plugin with no `[defaults].harnesses` and no `--harness`: Claude only.
pub fn default_plugin() -> Vec<Harness> {
    vec![Harness::Claude]
}

/// Agent Skill with no `[defaults].harnesses` and no `--harness`: hub only.
pub fn default_skill() -> Vec<Harness> {
    vec![Harness::Pi, Harness::Grok, Harness::Codex]
}

/// CLI `--harness` wins. Else `[defaults].harnesses`. Else today's default.
pub fn resolve(
    cli: &[Harness],
    defaults: &[String],
    row: &[String],
    fallback: Vec<Harness>,
) -> Result<Vec<Harness>> {
    if !cli.is_empty() {
        return Ok(unique(cli));
    }
    if !row.is_empty() {
        return parse_names(row);
    }
    if !defaults.is_empty() {
        return parse_names(defaults);
    }
    Ok(fallback)
}

pub fn load_defaults() -> (Vec<String>, Vec<String>) {
    let Some(path) = crate::manifest::discover() else {
        return (Vec::new(), Vec::new());
    };
    match crate::manifest::load(&path) {
        Ok(m) => (m.defaults.harnesses, m.defaults.mcp_harnesses),
        Err(_) => (Vec::new(), Vec::new()),
    }
}

/// Nested `skills/<name>/SKILL.md` trees inside a marketplace plugin.
pub fn plugin_skill_trees(qualified: &str) -> Result<Vec<(String, PathBuf)>> {
    let root = plugin_root(qualified)?;
    let skills = root.join("skills");
    let mut out = Vec::new();
    let rd = std::fs::read_dir(&skills).with_context(|| {
        format!(
            "plugin {qualified} has no skills/ directory at {}",
            root.display()
        )
    })?;
    for ent in rd.flatten() {
        let p = ent.path();
        if !(p.is_dir() && p.join("SKILL.md").is_file()) {
            continue;
        }
        let Some(n) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        crate::agent_skill::validate_skill_name(n)?;
        out.push((n.to_string(), p));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    if out.is_empty() {
        anyhow::bail!(
            "plugin {qualified} has no nested skills/<name>/SKILL.md under {}",
            root.display()
        );
    }
    Ok(out)
}

fn plugin_root(qualified: &str) -> Result<PathBuf> {
    if let Some(p) = installed_plugin_path(qualified) {
        if p.join("skills").is_dir() {
            return Ok(p);
        }
    }
    if let Some(p) = marketplace_plugin_dir(qualified) {
        if p.join("skills").is_dir() {
            return Ok(p);
        }
    }
    if let Some(p) = cached_plugin_dir(qualified) {
        if p.join("skills").is_dir() {
            return Ok(p);
        }
    }
    anyhow::bail!("could not find the plugin source tree for {qualified}")
}

fn installed_plugin_path(qualified: &str) -> Option<PathBuf> {
    let raw = std::fs::read(crate::paths::installed_plugins_json().ok()?).ok()?;
    let v: Value = serde_json::from_slice(&raw).ok()?;
    let path = v
        .get("plugins")?
        .get(qualified)?
        .as_array()?
        .first()?
        .get("installPath")?
        .as_str()?;
    let p = PathBuf::from(path);
    p.exists().then_some(p)
}

fn marketplace_plugin_dir(qualified: &str) -> Option<PathBuf> {
    let (name, mp) = qualified.rsplit_once('@')?;
    let known =
        crate::marketplace::load_known(&crate::paths::known_marketplaces_json().ok()?).ok()?;
    let loc = known
        .get(mp)?
        .get("installLocation")?
        .as_str()
        .map(PathBuf::from)?;
    let rel = plugin_source_rel(mp, name).unwrap_or_else(|| format!("skills/{name}"));
    Some(loc.join(rel))
}

fn plugin_source_rel(marketplace: &str, plugin: &str) -> Option<String> {
    let path = crate::paths::marketplace_manifest(marketplace).ok()?;
    let manifest = crate::marketplace::load_manifest(&path).ok()?;
    let entry = manifest.plugins.iter().find(|p| p.name == plugin)?;
    match &entry.source {
        Some(Value::String(s)) => Some(s.trim_start_matches("./").to_string()),
        _ => None,
    }
}

fn cached_plugin_dir(qualified: &str) -> Option<PathBuf> {
    let (plugin, mp) = qualified.rsplit_once('@')?;
    let base = crate::paths::plugins_dir()
        .ok()?
        .join("cache")
        .join(mp)
        .join(plugin);
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&base)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    versions.sort();
    versions.pop()
}

fn plugin_skill_names(qualified: &str) -> Vec<String> {
    plugin_skill_trees(qualified)
        .map(|v| v.into_iter().map(|(n, _)| n).collect())
        .unwrap_or_else(|_| {
            qualified
                .split_once('@')
                .map(|(n, _)| vec![n.to_string()])
                .unwrap_or_default()
        })
}

fn hub_has(name: &str) -> bool {
    crate::paths::user_skills_dir()
        .map(|root| root.join(name).join("SKILL.md").is_file())
        .unwrap_or(false)
}

/// Copy nested plugin skill trees into the shared Agent Skill hub.
/// Prints a skip line for harnesses that need a tree we will not invent.
pub fn materialize_hub(qualified: &str, harnesses: &[Harness]) -> Result<Vec<String>> {
    let want_hub = harnesses.iter().any(|h| h.hub_is_enough());
    for h in harnesses {
        if let Some(reason) = h.skill_skip_reason() {
            println!("  {} {}: {reason}", "·".dimmed(), h.as_str());
        }
    }
    if !want_hub {
        return Ok(Vec::new());
    }
    let trees = plugin_skill_trees(qualified)?;
    let root = crate::paths::user_skills_dir()?;
    let mut copied = BTreeSet::new();
    for (name, src) in &trees {
        crate::agent_skill::install_to_root(&root, name, src)?;
        copied.insert(name.clone());
        println!(
            "  {} {} → {} (hub)",
            "+".green(),
            name,
            root.join(name).display()
        );
    }
    if !copied.is_empty() {
        record_plugin_copies(qualified, &copied)?;
    }
    Ok(copied.into_iter().collect())
}

fn record_plugin_copies(qualified: &str, names: &BTreeSet<String>) -> Result<()> {
    let mut inv = crate::agent_skill::load_inventory()?;
    let now = crate::agent_skill::inventory_now();
    for name in names {
        let entry =
            inv.agent_skills
                .entry(name.clone())
                .or_insert_with(|| crate::agent_skill::Entry {
                    source: format!("plugin:{qualified}"),
                    installed_at: now.clone(),
                    head_sha: String::new(),
                    to: vec!["agents".into()],
                });
        if !entry.source.starts_with("plugin:") {
            anyhow::bail!(
                "refusing to overwrite Agent Skill '{name}' (source {}) with a plugin projection of {qualified}",
                entry.source
            );
        }
        entry.source = format!("plugin:{qualified}");
        if !entry.to.iter().any(|t| t == "agents") {
            entry.to.push("agents".into());
        }
    }
    crate::agent_skill::save_inventory(&inv)
}

pub struct Visibility {
    pub visible: Vec<Harness>,
    pub skipped: Vec<(Harness, &'static str)>,
}

impl Visibility {
    pub fn format_human(&self) -> String {
        let vis = self
            .visible
            .iter()
            .map(|h| h.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        let vis = if vis.is_empty() {
            "nowhere".into()
        } else {
            vis
        };
        if self.skipped.is_empty() {
            return format!("  [{vis}]");
        }
        let skip = self
            .skipped
            .iter()
            .map(|(h, _)| h.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("  [{vis} | skipped {skip}]")
    }

    pub fn to_json(&self) -> serde_json::Value {
        let skipped: serde_json::Map<String, serde_json::Value> = self
            .skipped
            .iter()
            .map(|(h, r)| {
                (
                    h.as_str().to_string(),
                    serde_json::Value::String((*r).into()),
                )
            })
            .collect();
        serde_json::json!({
            "visible": self.visible.iter().map(|h| h.as_str()).collect::<Vec<_>>(),
            "skipped": skipped,
        })
    }
}

/// Harnesses that can see a marketplace plugin. `claude` is true when the
/// plugin is enabled and inventoried. Hub-backed harnesses are true when any
/// nested skill name has `SKILL.md` in `~/.agents/skills/`.
pub fn visibility_for_plugin(qualified: &str, active: bool) -> Visibility {
    let names = plugin_skill_names(qualified);
    let on_hub = names.iter().any(|n| hub_has(n));
    visibility(active, on_hub)
}

pub fn visibility_for_skill(name: &str) -> Visibility {
    visibility(false, hub_has(name))
}

fn visibility(claude_active: bool, on_hub: bool) -> Visibility {
    let mut visible = Vec::new();
    let mut skipped = Vec::new();
    if claude_active {
        visible.push(Harness::Claude);
    }
    for h in [Harness::Pi, Harness::Grok, Harness::Codex] {
        if on_hub {
            visible.push(h);
        }
    }
    for h in [Harness::Hermes, Harness::Kimi] {
        if let Some(r) = h.skill_skip_reason() {
            skipped.push((h, r));
        }
    }
    Visibility { visible, skipped }
}

pub fn print_mcp_skips(harnesses: &[Harness]) {
    for h in harnesses {
        if h.mcp_supported() {
            continue;
        }
        println!(
            "  {} mcp: {} unsupported ({})",
            "·".dimmed(),
            h.as_str(),
            h.mcp_skip_reason()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_name_accepts_six_harnesses() {
        assert_eq!(Harness::parse_name("claude").unwrap(), Harness::Claude);
        assert_eq!(Harness::parse_name("codex").unwrap(), Harness::Codex);
    }

    #[test]
    fn parse_name_rejects_directory_tokens() {
        assert!(Harness::parse_name("agents").is_err());
        assert!(Harness::parse_name("project").is_err());
    }

    #[test]
    fn hub_is_enough_for_pi_grok_codex() {
        assert!(Harness::Pi.hub_is_enough());
        assert!(Harness::Grok.hub_is_enough());
        assert!(Harness::Codex.hub_is_enough());
        assert!(!Harness::Claude.hub_is_enough());
        assert!(!Harness::Hermes.hub_is_enough());
        assert!(!Harness::Kimi.hub_is_enough());
    }

    #[test]
    fn resolve_cli_wins() {
        let got = resolve(
            &[Harness::Pi],
            &["claude".into()],
            &["grok".into()],
            default_plugin(),
        )
        .unwrap();
        assert_eq!(got, vec![Harness::Pi]);
    }
}
