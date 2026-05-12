//! `sync` — apply a declarative skills.toml manifest. The headline command.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

pub fn run(file: Option<PathBuf>, dry_run: bool) -> Result<()> {
    let path = file.or_else(crate::manifest::discover).ok_or_else(|| {
        anyhow::anyhow!("no skills.toml found (looked in ./ and ~/.config/zskills/)")
    })?;
    println!("Manifest: {}", path.display().to_string().dimmed());

    let manifest = crate::manifest::load(&path)?;

    // -------- 1) Plugin reconciliation --------
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;

    let mut desired_plugins: BTreeSet<String> = BTreeSet::new();
    for entry in &manifest.skills {
        let qualified = match entry.qualified() {
            Some(q) => q,
            None => match crate::marketplace::resolve_spec(&entry.name, &known) {
                Ok(q) => q,
                Err(e) => {
                    eprintln!("{} {}: {}", "✗".red(), entry.name, e);
                    continue;
                }
            },
        };
        desired_plugins.insert(qualified);
    }

    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    let current_plugins: BTreeSet<String> = crate::settings::enabled_plugins(&settings)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| {
                    if v.as_bool().unwrap_or(false) {
                        Some(k.clone())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let plugins_to_enable: Vec<_> = desired_plugins.difference(&current_plugins).collect();
    let plugins_to_disable: Vec<_> = current_plugins.difference(&desired_plugins).collect();

    // -------- 2) Agent Skills reconciliation --------
    // The manifest carries (source, optional name). We need to compare against the inventory,
    // which carries (skill_name -> source). Build a desired-names set, but we also need to
    // remember the source for each so we can install.
    let mut desired_named: BTreeSet<String> = BTreeSet::new();
    let mut deferred_sources: Vec<&crate::manifest::AgentSkillEntry> = Vec::new();
    for entry in &manifest.agent_skills {
        match (&entry.name, &entry.source) {
            (Some(n), _) => {
                desired_named.insert(n.clone());
            }
            (None, Some(_)) => {
                // Source without an explicit name — every skill in `skills/` of that repo.
                deferred_sources.push(entry);
            }
            (None, None) => {
                // Invalid: report below at apply time
            }
        }
    }

    let inv = crate::agent_skill::load_inventory()?;
    let current_managed: BTreeSet<String> = inv.agent_skills.keys().cloned().collect();
    let on_disk: BTreeSet<String> = crate::agent_skill::installed_on_disk()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let agent_to_install_named: Vec<_> = desired_named
        .iter()
        .filter(|n| !on_disk.contains(*n))
        .cloned()
        .collect();
    let agent_to_refresh_named: Vec<_> = desired_named
        .iter()
        .filter(|n| on_disk.contains(*n))
        .cloned()
        .collect();
    let agent_to_remove: Vec<_> = current_managed
        .iter()
        .filter(|n| !desired_named.contains(*n))
        // Don't remove a skill that's in inventory but came from a source-only entry.
        // We'll re-resolve those when applying; planning here is conservative.
        .filter(|n| {
            let src = inv.agent_skills.get(*n).map(|e| e.source.clone());
            !deferred_sources
                .iter()
                .any(|e| e.source.as_ref() == src.as_ref())
        })
        .cloned()
        .collect();

    // -------- 3) Print plan --------
    println!("\n{}", "Plan".bold());
    let nothing = plugins_to_enable.is_empty()
        && plugins_to_disable.is_empty()
        && agent_to_install_named.is_empty()
        && agent_to_remove.is_empty()
        && deferred_sources.is_empty()
        && agent_to_refresh_named.is_empty();
    if nothing {
        println!("  (no changes — manifest matches current state)");
        return Ok(());
    }

    for k in &plugins_to_enable {
        println!("  {} enable  plugin  {}", "+".green(), k);
    }
    for k in &plugins_to_disable {
        println!(
            "  {} disable plugin  {} {}",
            "-".yellow(),
            k,
            "(in settings but not in manifest)".dimmed()
        );
    }
    for n in &agent_to_install_named {
        println!("  {} install agent   {}", "+".green(), n);
    }
    for entry in &deferred_sources {
        if let Some(s) = &entry.source {
            println!(
                "  {} install agent   {} {}",
                "+".green(),
                s,
                "(all skills in repo)".dimmed()
            );
        }
    }
    for n in &agent_to_refresh_named {
        println!("  {} refresh agent   {}", "~".cyan(), n);
    }
    for n in &agent_to_remove {
        println!(
            "  {} remove  agent   {} {}",
            "-".yellow(),
            n,
            "(installed but not in manifest)".dimmed()
        );
    }

    if dry_run {
        println!("\n(dry-run; no changes written)");
        return Ok(());
    }

    // -------- 4) Apply --------
    let ep = crate::settings::enabled_plugins_mut(&mut settings);
    for k in &plugins_to_enable {
        ep.insert((*k).clone(), Value::Bool(true));
    }
    for k in &plugins_to_disable {
        ep.insert((*k).clone(), Value::Bool(false));
    }
    crate::settings::save(&settings_path, &settings)?;

    for entry in &manifest.agent_skills {
        match (entry.source.as_deref(), entry.name.as_deref()) {
            (Some(src), name) => match crate::agent_skill::install(src, name) {
                Ok(names) => {
                    for n in &names {
                        println!("  installed agent skill {}", n.bold());
                    }
                }
                Err(e) => {
                    eprintln!("{} {}: {}", "✗".red(), src, e);
                }
            },
            (None, Some(name)) => {
                // Local-only entry: register in inventory if present on disk; don't fetch.
                let mut inv = crate::agent_skill::load_inventory()?;
                if !inv.agent_skills.contains_key(name) {
                    inv.agent_skills.insert(
                        name.to_string(),
                        crate::agent_skill::Entry {
                            source: "local".to_string(),
                            installed_at: format!(
                                "@{}",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0)
                            ),
                            head_sha: "local".to_string(),
                        },
                    );
                    crate::agent_skill::save_inventory(&inv)?;
                    println!("  tracked local agent skill {}", name.bold());
                }
            }
            (None, None) => {
                eprintln!(
                    "{} agent_skill entry needs either `source` or `name`",
                    "✗".red()
                );
            }
        }
    }

    for n in &agent_to_remove {
        match crate::agent_skill::remove(n) {
            Ok(_) => println!("  removed agent skill {}", n.bold()),
            Err(e) => eprintln!("{} {}: {}", "✗".red(), n, e),
        }
    }

    println!("\n{} applied.", "✓".green());
    Ok(())
}
