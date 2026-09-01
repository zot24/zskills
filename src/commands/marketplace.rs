use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::Value;
#[cfg(feature = "skills-sh")]
use serde_json::{json, Map};
use std::path::Path;

use crate::cli::MarketplaceCmd;

/// Built-in remote indexes. Registered without a git clone; search/install dispatch on
/// `source.source == "remote-index"` and `source.url`. Each driver lives behind its own
/// cargo feature so the binary stays vanilla unless you opt in.
#[cfg(feature = "skills-sh")]
const REMOTE_INDEX_SKILLS_SH: &str = "skills.sh";

pub fn run(cmd: MarketplaceCmd) -> Result<()> {
    match cmd {
        MarketplaceCmd::Add { source } => add(source),
        MarketplaceCmd::AddRecommended => add_recommended(),
        MarketplaceCmd::Remove { name } => remove(name),
        MarketplaceCmd::List { json: as_json } => list(as_json),
        MarketplaceCmd::Update { name } => update(name),
    }
}

fn add(source: String) -> Result<()> {
    #[cfg(feature = "skills-sh")]
    if source == REMOTE_INDEX_SKILLS_SH {
        return add_remote_index(REMOTE_INDEX_SKILLS_SH, "https://skills.sh");
    }
    let (fallback, repo_url) = crate::marketplace::parse_source(&source)?;
    let parent = crate::paths::marketplaces_dir()?;
    std::fs::create_dir_all(&parent).ok();

    // Clone to a staging dir first: the tap name lives in the manifest, which
    // only exists after the clone, and the final directory must carry that name.
    let staging_root = tempfile::Builder::new()
        .prefix(".zskills-add-")
        .tempdir_in(&parent)
        .with_context(|| format!("creating staging dir under {}", parent.display()))?;
    let staging = staging_root.path().join("src");
    crate::git::clone(&repo_url, &staging)?;

    let name = crate::marketplace::name_from_clone(&staging, &fallback);
    let dest = parent.join(&name);
    crate::marketplace::refuse_conflicting_registration(&name, &source, &repo_url)?;
    if !dest.exists() {
        std::fs::rename(&staging, &dest)
            .with_context(|| format!("moving clone into {}", dest.display()))?;
    }

    let install_location = crate::marketplace::register(&name, &source)?;
    println!("{} added marketplace {}", "✓".green(), name);
    print_add_followup(&name, &install_location);
    Ok(())
}

/// Print plugins from marketplace.json and extra Agent Skill trees under
/// `plugins/*/skills/`. Do not write the manifest.
fn print_add_followup(name: &str, install_location: &Path) {
    let manifest_path = install_location
        .join(".claude-plugin")
        .join("marketplace.json");
    let mut plugin_sources: Vec<String> = Vec::new();
    if !manifest_path.exists() {
        println!(
            "  {} no .claude-plugin/marketplace.json — plugin install and search will not find anything here",
            "!".yellow()
        );
    } else {
        match crate::marketplace::load_manifest(&manifest_path) {
            Ok(m) if m.plugins.is_empty() => {
                println!("  {} marketplace.json lists 0 plugins", "!".yellow());
            }
            Ok(m) => {
                plugin_sources = m
                    .plugins
                    .iter()
                    .filter_map(|p| local_plugin_source(&p.source))
                    .collect();
                let names: Vec<&str> = m.plugins.iter().map(|p| p.name.as_str()).collect();
                if names.len() <= 8 {
                    println!(
                        "  {} plugin{}: {}",
                        names.len(),
                        if names.len() == 1 { "" } else { "s" },
                        names.join(", ")
                    );
                } else {
                    println!("  {} plugins", names.len());
                }
                if names.len() == 1 {
                    println!("  Next: zskills plugin install {}@{}", names[0], name);
                } else {
                    println!("  Next: zskills plugin install <plugin>@{}", name);
                    println!("        zskills plugin install -i");
                }
            }
            Err(e) => {
                println!("  {} could not read marketplace.json ({e})", "!".yellow());
            }
        }
    }
    print_agent_skill_hint(name, install_location, &plugin_sources);
}

/// Local relative plugin source from marketplace.json (`"./claude-plugin"`).
/// Remote object sources have no tree in this clone to exclude.
fn local_plugin_source(source: &Option<Value>) -> Option<String> {
    let Value::String(s) = source.as_ref()? else {
        return None;
    };
    let s = s.trim().trim_start_matches("./").trim_end_matches('/');
    if s.is_empty() || s == "." || s.starts_with('/') || s.contains("://") {
        return None;
    }
    Some(s.to_string())
}

/// Walk `plugins/` only: nested skills of the Claude plugin live under its
/// source tree (`claude-plugin/skills/`, `./p/skills/`, …), not this glob.
fn extra_agent_skill_trees(
    install_location: &Path,
    plugin_sources: &[String],
) -> Vec<(String, Vec<String>)> {
    let plugins_dir = install_location.join("plugins");
    let Ok(entries) = std::fs::read_dir(&plugins_dir) else {
        return Vec::new();
    };
    let mut packs: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    packs.sort();

    let mut out = Vec::new();
    for pack in packs {
        let Some(pack_name) = pack.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if pack_name.starts_with('.') {
            continue;
        }
        let skills_dir = pack.join("skills");
        if !skills_dir.is_dir() {
            continue;
        }
        let rel = format!("plugins/{pack_name}/skills");
        if plugin_sources.iter().any(|src| path_is_inside(&rel, src)) {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&skills_dir) else {
            continue;
        };
        let mut names: Vec<String> = children
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.join("SKILL.md").is_file())
            .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
            .collect();
        names.sort();
        if !names.is_empty() {
            out.push((rel, names));
        }
    }
    out
}

fn path_is_inside(rel: &str, src: &str) -> bool {
    let src = src.trim_end_matches('/');
    rel == src
        || rel
            .strip_prefix(src)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn print_agent_skill_hint(marketplace: &str, install_location: &Path, plugin_sources: &[String]) {
    for (path, names) in extra_agent_skill_trees(install_location, plugin_sources) {
        println!("  Agent Skills under {path}: {}", names.join(", "));
        for skill in &names {
            println!("    [[agent_skills]]");
            println!("    marketplace = \"{marketplace}\"");
            println!("    path = \"{path}\"");
            println!("    name = \"{skill}\"");
        }
    }
}

fn add_recommended() -> Result<()> {
    println!("Seeding recommended marketplaces ...");
    if let Err(e) = add("anthropics/claude-plugins-official".to_string()) {
        eprintln!("  {} anthropics/claude-plugins-official: {}", "✗".red(), e);
    }
    #[cfg(feature = "skills-sh")]
    println!(
        "\n{}",
        "Tip: skills.sh federation is opt-in. Add it with `zskills marketplace add skills.sh` and set ZSKILLS_SKILLS_SH_API_KEY."
            .dimmed()
    );
    Ok(())
}

#[cfg(feature = "skills-sh")]
fn add_remote_index(name: &str, url: &str) -> Result<()> {
    let path = crate::paths::known_marketplaces_json()?;
    let mut known = crate::marketplace::load_known(&path)?;

    let mut entry = Map::new();
    entry.insert(
        "source".into(),
        json!({ "source": "remote-index", "url": url }),
    );
    entry.insert("autoUpdate".into(), Value::Bool(false));
    // Same file, same validator — see `add()`.
    entry.insert(
        "lastUpdated".into(),
        Value::String(crate::timestamp::utc_now_iso8601()),
    );
    known.insert(name.to_string(), Value::Object(entry));
    crate::marketplace::save_known(&path, &known)?;

    // Don't mirror into settings.json extraKnownMarketplaces — Claude Code won't recognize
    // remote-index entries. They're purely zskills-internal.
    println!(
        "{} added remote index {} ({})",
        "✓".green(),
        name.bold(),
        url
    );
    Ok(())
}

fn remove(name: String) -> Result<()> {
    let path = crate::paths::known_marketplaces_json()?;
    let mut known = crate::marketplace::load_known(&path)?;
    known.remove(&name);
    crate::marketplace::save_known(&path, &known)?;

    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    crate::settings::extra_marketplaces_mut(&mut settings).remove(&name);
    crate::settings::save(&settings_path, &settings)?;

    let dir = crate::paths::marketplaces_dir()?.join(&name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).ok();
    }
    println!("{} removed marketplace {}", "-".yellow(), name);
    Ok(())
}

fn list(as_json: bool) -> Result<()> {
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    // `list` is read-only, so a manifest it cannot parse costs the user the pin
    // column and nothing else. The mutating commands use `load_pins()?` instead:
    // there, treating an unparseable manifest as "no pins" would float every pinned
    // marketplace, which is the failure the pin exists to prevent.
    let pins = crate::marketplace::load_pins().unwrap_or_default();
    if as_json {
        let mut out = known.clone();
        // Surface the pin in the JSON view without writing it to
        // known_marketplaces.json — that file belongs to Claude Code.
        for (name, pin) in &pins {
            if let Some(entry) = out.get_mut(name).and_then(|e| e.as_object_mut()) {
                entry.insert("zskillsPin".into(), Value::String(pin.clone()));
            }
        }
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    if known.is_empty() {
        println!("(no marketplaces registered)");
        return Ok(());
    }
    for (name, entry) in &known {
        if crate::marketplace::is_remote_index(entry) {
            let url = entry
                .get("source")
                .and_then(|s| s.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!(
                "  {}  {} {}",
                name.bold(),
                "[remote-index]".cyan(),
                url.dimmed()
            );
            continue;
        }
        let count = crate::marketplace::load_manifest(&crate::paths::marketplace_manifest(name)?)
            .map(|m| m.plugins.len())
            .unwrap_or(0);
        let auto = entry
            .get("autoUpdate")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let pin_note = match pins.get(name) {
            Some(pin) => format!("  [pinned {}]", pin).yellow().to_string(),
            None if auto => "  [autoUpdate]".dimmed().to_string(),
            None => String::new(),
        };
        println!("  {}  {} plugin(s){}", name.bold(), count, pin_note);
    }
    Ok(())
}

fn update(name: Option<String>) -> Result<()> {
    let known_path = crate::paths::known_marketplaces_json()?;
    let mut known = crate::marketplace::load_known(&known_path)?;
    let targets: Vec<String> = match name {
        Some(n) => vec![n],
        None => known.keys().cloned().collect(),
    };
    let pins = crate::marketplace::load_pins()?;
    let mut dirty = false;
    for n in &targets {
        if known
            .get(n)
            .is_some_and(crate::marketplace::is_remote_index)
        {
            continue;
        }
        let repo = crate::paths::marketplaces_dir()?.join(n);
        if !repo.exists() {
            continue;
        }
        print!("Updating {} ... ", n);
        match crate::marketplace::refresh(n, &repo, pins.get(n).map(String::as_str)) {
            Ok(outcome) => {
                println!("{}", crate::marketplace::refresh_label(&outcome).green());
                // Only stamp when the clone actually moved. A pin that was already
                // satisfied changed nothing, and `lastUpdated` should not claim it did.
                let moved = !matches!(
                    outcome,
                    crate::marketplace::Refresh::Pinned { moved: false, .. }
                );
                if moved && crate::marketplace::stamp_last_updated(&mut known, n) {
                    dirty = true;
                }
            }
            Err(e) => println!("{} ({:#})", "fail".red(), e),
        }
    }
    if dirty {
        crate::marketplace::save_known(&known_path, &known)?;
    }
    Ok(())
}
