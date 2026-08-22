use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::{json, Map, Value};

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
    let (name, repo_url) = parse_source(&source)?;
    let path = crate::paths::known_marketplaces_json()?;
    let mut known = crate::marketplace::load_known(&path)?;

    let install_location = crate::paths::marketplaces_dir()?.join(&name);
    if !install_location.exists() {
        if let Some(parent) = install_location.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        println!(
            "Cloning {} into {} ...",
            repo_url,
            install_location.display()
        );
        crate::git::clone(&repo_url, &install_location)?;
    }

    let mut entry = Map::new();
    let github_form = source.split('/').collect::<Vec<_>>();
    let src_obj = if github_form.len() == 2 && !source.starts_with("http") {
        json!({ "source": "github", "repo": source })
    } else {
        json!({ "source": "git", "url": repo_url })
    };
    entry.insert("source".into(), src_obj);
    entry.insert(
        "installLocation".into(),
        Value::String(install_location.to_string_lossy().to_string()),
    );
    entry.insert("autoUpdate".into(), Value::Bool(true));
    // Claude Code validates `lastUpdated` as a *string* when it loads
    // known_marketplaces.json. Omit it and every `claude plugin install` fails with
    // "Marketplace configuration file is corrupted: <name>.lastUpdated: Invalid
    // input: expected string, received undefined". This field is not optional.
    entry.insert(
        "lastUpdated".into(),
        Value::String(crate::timestamp::utc_now_iso8601()),
    );
    known.insert(name.clone(), Value::Object(entry));

    crate::marketplace::save_known(&path, &known)?;

    // Mirror in settings.json -> extraKnownMarketplaces
    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    let ekm = crate::settings::extra_marketplaces_mut(&mut settings);
    ekm.insert(
        name.clone(),
        json!({ "source": if source.contains('/') && !source.contains("://") {
            json!({ "source": "github", "repo": source })
        } else {
            json!({ "source": "git", "url": repo_url })
        }}),
    );
    crate::settings::save(&settings_path, &settings)?;

    println!("{} added marketplace {}", "✓".green(), name);
    Ok(())
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

/// Recognize a remote-index entry by its JSON shape. Non-feature-gated so older configs
/// (entries written by a `skills-sh`-enabled build) are still tolerated when the feature
/// is off — we just skip them in list/update rather than crashing.
pub(crate) fn is_remote_index(entry: &Value) -> bool {
    entry
        .get("source")
        .and_then(|s| s.get("source"))
        .and_then(|v| v.as_str())
        == Some("remote-index")
}

fn parse_source(source: &str) -> Result<(String, String)> {
    if source.contains("://") {
        // git URL
        let name = source
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or(source)
            .to_string();
        Ok((name, source.to_string()))
    } else if source.contains('/') && !source.starts_with('/') {
        // owner/repo
        let name = source.split('/').next_back().unwrap_or(source).to_string();
        let url = format!("https://github.com/{}.git", source);
        Ok((name, url))
    } else {
        anyhow::bail!(
            "unrecognized marketplace source: {} (expected owner/repo or git URL)",
            source
        )
    }
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
        if is_remote_index(entry) {
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
        if known.get(n).is_some_and(is_remote_index) {
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
