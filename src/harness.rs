//! Which coding harness can see a name.
//!
//! Tokens are harnesses (`claude`, `pi`, `hermes`, `kimi`, `grok`, `codex`),
//! not directories. Grok scans the shared hub `~/.agents/skills/<name>/`.
//! Pi scans the hub only after the absolute hub path is listed in
//! `~/.pi/agent/settings.json` `skills: []`. Plugin nested skills are **copied**
//! into the hub (a plugin cache path is version-stamped, so a link would break
//! on upgrade). Hub → harness is a **symlink** into `~/.claude/skills/<name>`,
//! `~/.codex/skills/<name>`, or `~/.hermes/skills/<category>/<name>/`, because
//! the hub path is stable and `skill upgrade` must propagate.

use anyhow::{Context, Result};
use clap::ValueEnum;
use owo_colors::OwoColorize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, ValueEnum)]
pub enum Harness {
    Claude,
    Pi,
    Hermes,
    Kimi,
    Grok,
    Codex,
}

/// How a hub copy at `~/.agents/skills/<name>/` relates to this harness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HubSufficiency {
    /// The harness scans the hub by itself.
    Always,
    /// The harness scans the hub only after the hub path is in its settings.
    WhenRegistered,
    /// The harness does not scan the hub.
    Never,
}

/// Outcome of ensuring the hub path is in Pi's `skills` array.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PiHubRegister {
    /// Wrote the file (created the key, added the path, or dropped duplicates).
    Wrote,
    /// Path was already present exactly once; the file was not rewritten.
    Already,
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

    /// How this harness learns about skills that live on the hub.
    pub fn hub_sufficiency(self) -> HubSufficiency {
        match self {
            Self::Grok => HubSufficiency::Always,
            Self::Pi => HubSufficiency::WhenRegistered,
            Self::Claude | Self::Hermes | Self::Kimi | Self::Codex => HubSufficiency::Never,
        }
    }

    /// True when this harness can use a hub copy, possibly after registration.
    pub fn uses_hub(self) -> bool {
        !matches!(self.hub_sufficiency(), HubSufficiency::Never)
    }

    /// True when a hub copy at `~/.agents/skills/<name>/SKILL.md` is enough
    /// for this harness to load the skill *right now*.
    ///
    /// Grok and Codex scan the hub natively. Pi scans the hub only after
    /// [`register_pi_hub`] has listed the absolute hub path in
    /// `~/.pi/agent/settings.json`. Returning true for unregistered Pi is a lie.
    pub fn hub_is_enough(self) -> bool {
        match self.hub_sufficiency() {
            HubSufficiency::Always => true,
            HubSufficiency::WhenRegistered => pi_hub_is_registered(),
            HubSufficiency::Never => false,
        }
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
            Self::Kimi => {
                Some("unsupported (no cited skills directory under ~/.kimi-code/; not inventing a folder)")
            }
            _ => None,
        }
    }

    /// Home directory used to detect whether this harness is present.
    pub fn home_dir(self) -> Result<PathBuf> {
        match self {
            Self::Claude => crate::paths::claude_home(),
            Self::Pi => crate::paths::pi_home(),
            Self::Grok => crate::paths::grok_home(),
            Self::Hermes => crate::paths::hermes_home(),
            Self::Kimi => crate::paths::kimi_home(),
            Self::Codex => crate::paths::codex_home(),
        }
    }

    pub fn home_dir_exists(self) -> bool {
        self.home_dir().map(|p| p.is_dir()).unwrap_or(false)
    }

    /// Per-harness skill directory for `name`, or `None` when the hub is enough
    /// (Pi, Grok) or the harness is unsupported (Kimi).
    pub fn skill_root(self, name: &str, category: &str) -> Result<Option<PathBuf>> {
        crate::agent_skill::validate_skill_name(name)?;
        match self {
            Self::Pi | Self::Grok | Self::Kimi => Ok(None),
            Self::Claude => Ok(Some(crate::paths::claude_home()?.join("skills").join(name))),
            Self::Codex => Ok(Some(crate::paths::codex_home()?.join("skills").join(name))),
            Self::Hermes => {
                validate_hermes_category(category)?;
                Ok(Some(
                    crate::paths::hermes_home()?
                        .join("skills")
                        .join(category)
                        .join(name),
                ))
            }
        }
    }

    /// True when a hub copy is required as the symlink source or as the scan root.
    pub fn needs_hub_copy(self) -> bool {
        self.uses_hub() || matches!(self, Self::Codex | Self::Hermes)
    }
}

/// Default Hermes category. `--category` overrides this. Do not invent others.
pub const DEFAULT_HERMES_CATEGORY: &str = "software-development";

pub fn validate_hermes_category(category: &str) -> Result<()> {
    anyhow::ensure!(!category.is_empty(), "category must not be empty");
    anyhow::ensure!(
        category != "." && category != "..",
        "category must not be . or .."
    );
    anyhow::ensure!(
        !category.contains('/') && !category.contains('\\'),
        "category must be a single path segment"
    );
    Ok(())
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

/// Agent Skill with no `[defaults].harnesses` and no `--harness`: every
/// harness whose home directory exists, minus those with `skill_skip_reason`.
pub fn default_skill() -> Vec<Harness> {
    [
        Harness::Claude,
        Harness::Pi,
        Harness::Hermes,
        Harness::Kimi,
        Harness::Grok,
        Harness::Codex,
    ]
    .into_iter()
    .filter(|h| h.home_dir_exists() && h.skill_skip_reason().is_none())
    .collect()
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

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn hub_path_string() -> Result<String> {
    Ok(crate::paths::user_skills_dir()?
        .to_string_lossy()
        .into_owned())
}

/// True when Pi's settings list the absolute hub path at least once.
pub fn pi_hub_is_registered() -> bool {
    let Ok(path) = crate::paths::pi_settings_json() else {
        return false;
    };
    let Ok(hub) = hub_path_string() else {
        return false;
    };
    let Ok(map) = crate::settings::load(&path) else {
        return false;
    };
    map.get("skills")
        .and_then(|v| v.as_array())
        .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(hub.as_str())))
}

/// If `harnesses` includes Pi, ensure the hub path is listed once in Pi settings.
pub fn ensure_pi_hub_if_targeted(harnesses: &[Harness]) -> Result<()> {
    if harnesses.contains(&Harness::Pi) {
        register_pi_hub()?;
    }
    Ok(())
}

/// Ensure the absolute hub path is present exactly once in
/// `~/.pi/agent/settings.json` `skills: []`. Creates the key if absent.
/// Preserves every other key. Does not rewrite when the path is already unique.
pub fn register_pi_hub() -> Result<PiHubRegister> {
    let path = crate::paths::pi_settings_json()?;
    let hub = hub_path_string()?;

    if path.exists() {
        // Hash before rewriting a file we did not last write. Failure here
        // refuses the rewrite.
        sha256_file(&path)?;
    }

    let mut map = crate::settings::load(&path)?;
    match map.get("skills") {
        None => {}
        Some(Value::Array(_)) => {}
        Some(_) => anyhow::bail!(
            "{} has a non-array `skills` key; refusing to overwrite it",
            path.display()
        ),
    }

    let skills = map
        .entry("skills")
        .or_insert_with(|| Value::Array(Vec::new()));
    let arr = skills
        .as_array_mut()
        .expect("skills is an array after the check above");

    let n = arr
        .iter()
        .filter(|v| v.as_str() == Some(hub.as_str()))
        .count();
    if n == 1 {
        println!(
            "  {} hub path {} already in {} skills[]",
            "·".dimmed(),
            hub,
            path.display()
        );
        return Ok(PiHubRegister::Already);
    }

    if n == 0 {
        arr.push(Value::String(hub.clone()));
    } else {
        let mut kept = false;
        arr.retain(|v| {
            if v.as_str() == Some(hub.as_str()) {
                if kept {
                    return false;
                }
                kept = true;
                true
            } else {
                true
            }
        });
    }

    crate::settings::save(&path, &map)?;
    println!(
        "  {} registered {} in {} skills[]",
        "+".green(),
        hub,
        path.display()
    );
    Ok(PiHubRegister::Wrote)
}

fn skill_md_resolves(dir: &Path) -> bool {
    dir.join("SKILL.md").is_file()
}

fn claude_skills_dir() -> Result<PathBuf> {
    Ok(crate::paths::claude_home()?.join("skills"))
}

/// True when Claude's user-skill root has `name`. A hub copy sitting at the
/// same path (collapsed `CLAUDE_HOME`/`AGENTS_HOME` in tests) does not count
/// unless that path is a symlink.
fn claude_has_skill(name: &str) -> bool {
    let Ok(dest) = claude_skills_dir().map(|d| d.join(name)) else {
        return false;
    };
    if !skill_md_resolves(&dest) {
        return false;
    }
    let Ok(hub) = crate::paths::user_skills_dir() else {
        return true;
    };
    if dest.parent() == Some(hub.as_path()) {
        return dest
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
    }
    true
}

/// Symlink hub `<name>` into each targeted harness that has a `skill_root`.
/// Plugin → hub stays a copy; this is hub → harness only.
pub fn link_hub_to_harnesses(
    names: &[String],
    harnesses: &[Harness],
    category: &str,
) -> Result<()> {
    let hub_root = crate::paths::user_skills_dir()?;
    for h in unique(harnesses) {
        if h.skill_skip_reason().is_some() {
            continue;
        }
        for name in names {
            let hub = hub_root.join(name);
            if !skill_md_resolves(&hub) {
                continue;
            }
            let Some(dest) = h.skill_root(name, category)? else {
                continue;
            };
            symlink_hub_into(&hub, &dest)?;
        }
    }
    Ok(())
}

fn symlink_hub_into(hub: &Path, dest: &Path) -> Result<()> {
    if dest == hub {
        return Ok(());
    }
    if let Ok(meta) = dest.symlink_metadata() {
        if meta.file_type().is_symlink() {
            if let Ok(target) = std::fs::read_link(dest) {
                if target == hub {
                    return Ok(());
                }
            }
            std::fs::remove_file(dest)
                .with_context(|| format!("replacing symlink at {}", dest.display()))?;
        } else {
            println!(
                "  {} leaving {} (not a symlink; hub → harness will not clobber it)",
                "·".dimmed(),
                dest.display()
            );
            return Ok(());
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(hub, dest)
        .with_context(|| format!("linking {} → {}", dest.display(), hub.display()))?;
    #[cfg(not(unix))]
    anyhow::bail!(
        "hub → harness symlink is not supported on this OS ({} → {})",
        dest.display(),
        hub.display()
    );
    println!(
        "  {} {} → {} (link)",
        "+".green(),
        dest.display(),
        hub.display()
    );
    Ok(())
}

/// Copy nested plugin skill trees into the shared Agent Skill hub.
/// Prints a skip line for harnesses that need a tree we will not invent.
pub fn materialize_hub(
    qualified: &str,
    harnesses: &[Harness],
    category: &str,
) -> Result<Vec<String>> {
    ensure_pi_hub_if_targeted(harnesses)?;
    let want_hub = harnesses.iter().any(|h| h.needs_hub_copy());
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
        let names: Vec<String> = copied.iter().cloned().collect();
        link_hub_to_harnesses(&names, harnesses, category)?;
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
    visibility(active, on_hub, &names)
}

pub fn visibility_for_skill(name: &str) -> Visibility {
    visibility(claude_has_skill(name), hub_has(name), &[name.to_string()])
}

fn visibility(claude_active: bool, on_hub: bool, names: &[String]) -> Visibility {
    let mut visible = Vec::new();
    let mut skipped = Vec::new();
    if claude_active {
        visible.push(Harness::Claude);
    }
    for h in [Harness::Pi, Harness::Grok] {
        if on_hub && h.hub_is_enough() {
            visible.push(h);
        }
    }
    if names.iter().any(|n| {
        Harness::Codex
            .skill_root(n, DEFAULT_HERMES_CATEGORY)
            .ok()
            .flatten()
            .is_some_and(|p| skill_md_resolves(&p))
    }) {
        visible.push(Harness::Codex);
    }
    if names.iter().any(|n| {
        Harness::Hermes
            .skill_root(n, DEFAULT_HERMES_CATEGORY)
            .ok()
            .flatten()
            .is_some_and(|p| skill_md_resolves(&p))
    }) {
        visible.push(Harness::Hermes);
    }
    if let Some(r) = Harness::Kimi.skill_skip_reason() {
        skipped.push((Harness::Kimi, r));
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

    fn with_pi_home<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
        let _guard = crate::paths::HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let agents = home.join("agents");
        std::fs::create_dir_all(agents.join("skills")).unwrap();
        // SAFETY: held under HOME_ENV_LOCK; restored before the guard drops.
        let prev_pi = std::env::var_os("PI_HOME");
        let prev_agents = std::env::var_os("AGENTS_HOME");
        std::env::set_var("PI_HOME", home);
        std::env::set_var("AGENTS_HOME", &agents);
        let out = f(home);
        match prev_pi {
            Some(v) => std::env::set_var("PI_HOME", v),
            None => std::env::remove_var("PI_HOME"),
        }
        match prev_agents {
            Some(v) => std::env::set_var("AGENTS_HOME", v),
            None => std::env::remove_var("AGENTS_HOME"),
        }
        out
    }

    #[test]
    fn hub_sufficiency_marks_pi_conditional() {
        assert_eq!(
            Harness::Pi.hub_sufficiency(),
            HubSufficiency::WhenRegistered
        );
        assert_eq!(Harness::Grok.hub_sufficiency(), HubSufficiency::Always);
        assert_eq!(Harness::Codex.hub_sufficiency(), HubSufficiency::Never);
        assert_eq!(Harness::Claude.hub_sufficiency(), HubSufficiency::Never);
        assert_eq!(Harness::Hermes.hub_sufficiency(), HubSufficiency::Never);
        assert_eq!(Harness::Kimi.hub_sufficiency(), HubSufficiency::Never);
    }

    #[test]
    fn hub_is_enough_for_grok_not_unregistered_pi_or_codex() {
        with_pi_home(|_| {
            assert!(!Harness::Pi.hub_is_enough());
            assert!(Harness::Grok.hub_is_enough());
            assert!(!Harness::Codex.hub_is_enough());
            assert!(!Harness::Claude.hub_is_enough());
            assert!(!Harness::Hermes.hub_is_enough());
            assert!(!Harness::Kimi.hub_is_enough());
        });
    }

    #[test]
    fn kimi_skip_reason_names_kimi_code_not_kimi() {
        let reason = Harness::Kimi.skill_skip_reason().unwrap();
        assert!(reason.contains("~/.kimi-code/"), "{reason}");
        assert!(!reason.contains("~/.kimi/"), "{reason}");
        assert!(Harness::Hermes.skill_skip_reason().is_none());
    }

    #[test]
    fn register_pi_hub_is_idempotent() {
        with_pi_home(|home| {
            assert!(!Harness::Pi.hub_is_enough());
            assert_eq!(register_pi_hub().unwrap(), PiHubRegister::Wrote);
            assert!(Harness::Pi.hub_is_enough());
            assert_eq!(register_pi_hub().unwrap(), PiHubRegister::Already);
            assert_eq!(register_pi_hub().unwrap(), PiHubRegister::Already);
            let settings: serde_json::Value = serde_json::from_slice(
                &std::fs::read(home.join("agent").join("settings.json")).unwrap(),
            )
            .unwrap();
            let hub = hub_path_string().unwrap();
            let count = settings["skills"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v.as_str() == Some(hub.as_str()))
                .count();
            assert_eq!(count, 1);
        });
    }

    #[test]
    fn register_pi_hub_preserves_other_keys() {
        with_pi_home(|home| {
            let path = home.join("agent").join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                r#"{
  "defaultModel": "grok-4.6",
  "defaultProvider": "xai",
  "theme": "dark"
}
"#,
            )
            .unwrap();
            register_pi_hub().unwrap();
            let settings: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(settings["defaultModel"], "grok-4.6");
            assert_eq!(settings["defaultProvider"], "xai");
            assert_eq!(settings["theme"], "dark");
            let hub = hub_path_string().unwrap();
            assert_eq!(
                settings["skills"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|v| v.as_str() == Some(hub.as_str()))
                    .count(),
                1
            );
        });
    }

    #[test]
    fn default_skill_hybrid_present_homes_minus_skipped() {
        let _guard = crate::paths::HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("claude")).unwrap();
        std::fs::create_dir_all(root.join("grok")).unwrap();
        std::fs::create_dir_all(root.join("kimi-code")).unwrap();
        let keys = [
            ("CLAUDE_HOME", root.join("claude")),
            ("PI_HOME", root.join("pi")),
            ("GROK_HOME", root.join("grok")),
            ("HERMES_HOME", root.join("hermes")),
            ("KIMI_HOME", root.join("kimi-code")),
            ("CODEX_HOME", root.join("codex")),
        ];
        // SAFETY: held under HOME_ENV_LOCK; restored before the guard drops.
        let prev: Vec<_> = keys
            .iter()
            .map(|(k, v)| {
                let old = std::env::var_os(k);
                std::env::set_var(k, v);
                (*k, old)
            })
            .collect();
        let got = default_skill();
        for (k, old) in prev {
            match old {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        assert_eq!(got, vec![Harness::Claude, Harness::Grok]);
    }

    #[test]
    fn skill_root_paths() {
        let _guard = crate::paths::HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let keys = [
            ("CLAUDE_HOME", root.join("claude")),
            ("CODEX_HOME", root.join("codex")),
            ("HERMES_HOME", root.join("hermes")),
        ];
        // SAFETY: held under HOME_ENV_LOCK; restored before the guard drops.
        let prev: Vec<_> = keys
            .iter()
            .map(|(k, v)| {
                let old = std::env::var_os(k);
                std::env::set_var(k, v);
                (*k, old)
            })
            .collect();
        let claude = Harness::Claude
            .skill_root("demo", DEFAULT_HERMES_CATEGORY)
            .unwrap()
            .unwrap();
        let codex = Harness::Codex
            .skill_root("demo", DEFAULT_HERMES_CATEGORY)
            .unwrap()
            .unwrap();
        let hermes = Harness::Hermes
            .skill_root("demo", DEFAULT_HERMES_CATEGORY)
            .unwrap()
            .unwrap();
        let custom = Harness::Hermes
            .skill_root("demo", "devops")
            .unwrap()
            .unwrap();
        assert!(Harness::Pi
            .skill_root("demo", DEFAULT_HERMES_CATEGORY)
            .unwrap()
            .is_none());
        assert!(Harness::Grok
            .skill_root("demo", DEFAULT_HERMES_CATEGORY)
            .unwrap()
            .is_none());
        for (k, old) in prev {
            match old {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        assert_eq!(claude, root.join("claude").join("skills").join("demo"));
        assert_eq!(codex, root.join("codex").join("skills").join("demo"));
        assert_eq!(
            hermes,
            root.join("hermes")
                .join("skills")
                .join("software-development")
                .join("demo")
        );
        assert_eq!(
            custom,
            root.join("hermes")
                .join("skills")
                .join("devops")
                .join("demo")
        );
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
