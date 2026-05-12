//! `sync` — apply a declarative skills.toml manifest. The headline command.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

pub fn run(file: Option<PathBuf>, dry_run: bool) -> Result<()> {
    let path = file
        .or_else(crate::manifest::discover)
        .ok_or_else(|| anyhow::anyhow!("no skills.toml found (looked in ./ and ~/.config/zskills/)"))?;
    println!("Manifest: {}", path.display().to_string().dimmed());

    let manifest = crate::manifest::load(&path)?;
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;

    let mut desired: BTreeSet<String> = BTreeSet::new();
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
        desired.insert(qualified);
    }

    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    let current: BTreeSet<String> = crate::settings::enabled_plugins(&settings)
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

    let to_add: Vec<_> = desired.difference(&current).collect();
    let to_disable: Vec<_> = current.difference(&desired).collect();

    println!("\n{}", "Plan".bold());
    if to_add.is_empty() && to_disable.is_empty() {
        println!("  (no changes — manifest matches current state)");
        return Ok(());
    }
    for k in &to_add {
        println!("  {} enable  {}", "+".green(), k);
    }
    for k in &to_disable {
        println!(
            "  {} disable {} {}",
            "-".yellow(),
            k,
            "(in settings but not in manifest)".dimmed()
        );
    }

    if dry_run {
        println!("\n(dry-run; no changes written)");
        return Ok(());
    }

    let ep = crate::settings::enabled_plugins_mut(&mut settings);
    for k in &to_add {
        ep.insert((*k).clone(), Value::Bool(true));
    }
    for k in &to_disable {
        ep.insert((*k).clone(), Value::Bool(false));
    }
    crate::settings::save(&settings_path, &settings)?;
    println!("\n{} applied.", "✓".green());
    Ok(())
}
