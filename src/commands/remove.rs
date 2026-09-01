//! `remove` (apt-style: disable + drop inventory + drop `[[skills]]` intent, keep bytes)
//! `purge` (also delete bytes from ~/.claude/plugins/cache/.../<plugin>)

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Map;
use serde_json::Value;

/// Identity for plugin remove/enable/disable: what is installed, not the catalogue.
pub(crate) fn resolve_installed_plugin(
    spec: &str,
    ep: &Map<String, Value>,
    plugins: &Map<String, Value>,
    known: &Map<String, Value>,
) -> Result<String> {
    if ep.contains_key(spec) || plugins.contains_key(spec) {
        return Ok(spec.to_string());
    }
    match crate::marketplace::resolve_spec(spec, known) {
        Ok(q) if ep.contains_key(&q) || plugins.contains_key(&q) => Ok(q),
        Ok(q) => anyhow::bail!("plugin '{q}' is not installed"),
        Err(_) => {
            let prefix = format!("{spec}@");
            let mut hits: Vec<String> = ep
                .keys()
                .chain(plugins.keys())
                .filter(|k| k.as_str() == spec || k.starts_with(&prefix))
                .cloned()
                .collect();
            hits.sort();
            hits.dedup();
            match hits.len() {
                1 => Ok(hits.remove(0)),
                0 => anyhow::bail!("plugin '{spec}' is not installed"),
                _ => anyhow::bail!(
                    "plugin '{spec}' is ambiguous — qualify with @marketplace (matches: {})",
                    hits.join(", ")
                ),
            }
        }
    }
}

pub fn run(specs: Vec<String>, interactive: bool, purge_bytes: bool) -> Result<()> {
    if interactive && specs.is_empty() {
        return run_interactive(purge_bytes);
    }

    if specs.is_empty() {
        anyhow::bail!("specify at least one plugin name, or use -i/--interactive to browse");
    }

    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    let settings_path = crate::paths::settings_json()?;
    let inventory_path = crate::paths::installed_plugins_json()?;

    let mut settings = crate::settings::load(&settings_path)?;
    let mut inventory = crate::inventory::load(&inventory_path)?;
    let mut removed: Vec<String> = Vec::new();
    let mut failed = 0usize;

    for spec in &specs {
        let ep = crate::settings::enabled_plugins(&settings)
            .cloned()
            .unwrap_or_default();
        let plugs = crate::inventory::plugins(&inventory)
            .cloned()
            .unwrap_or_default();
        let qualified = match resolve_installed_plugin(spec, &ep, &plugs, &known) {
            Ok(q) => q,
            Err(e) => {
                eprintln!("{} {e}", "✗".red());
                failed += 1;
                continue;
            }
        };

        let ep = crate::settings::enabled_plugins_mut(&mut settings);
        let had_ep = ep.remove(&qualified).is_some();

        let mut install_paths: Vec<std::path::PathBuf> = Vec::new();
        let plugins = crate::inventory::plugins_mut(&mut inventory);
        let had_inv = if let Some(entries) = plugins.remove(&qualified) {
            if purge_bytes {
                if let Some(arr) = entries.as_array() {
                    for entry in arr {
                        if let Some(p) = entry.get("installPath").and_then(|v| v.as_str()) {
                            install_paths.push(std::path::PathBuf::from(p));
                        }
                    }
                }
            }
            true
        } else {
            false
        };

        if !had_ep && !had_inv && install_paths.is_empty() {
            eprintln!("{} plugin '{qualified}' is not installed", "✗".red());
            failed += 1;
            continue;
        }
        removed.push(qualified.clone());

        if purge_bytes {
            for p in &install_paths {
                if p.exists() {
                    if let Err(e) = std::fs::remove_dir_all(p) {
                        eprintln!("{} could not delete {}: {}", "!".yellow(), p.display(), e);
                    } else {
                        println!("  deleted {}", p.display().to_string().dimmed());
                    }
                }
            }
            println!("{} purged plugin {}", "✗".red(), qualified);
        } else {
            println!("{} removed plugin {}", "-".yellow(), qualified);
        }
    }

    if !removed.is_empty() {
        // Intent first: a later state-save failure must not leave a [[skills]]
        // row that `sync` would re-enable.
        if let Some(path) = crate::manifest::discover() {
            for q in &removed {
                let (name, mp) = match q.rsplit_once('@') {
                    Some((n, m)) => (n, Some(m)),
                    None => (q.as_str(), None),
                };
                crate::manifest::drop_skill(&path, name, mp)?;
            }
        }
        crate::settings::save(&settings_path, &settings)?;
        crate::inventory::save(&inventory_path, &inventory)?;
    }
    anyhow::ensure!(failed == 0, "{failed} plugin remove(s) failed");
    Ok(())
}

fn run_interactive(purge_bytes: bool) -> Result<()> {
    use crate::interactive::Item;

    let settings_path = crate::paths::settings_json()?;
    let settings = crate::settings::load(&settings_path)?;

    let ep_keys: Vec<String> = crate::settings::enabled_plugins(&settings)
        .map(|ep| ep.keys().cloned().collect())
        .unwrap_or_default();

    if ep_keys.is_empty() {
        println!("{}", "No enabled plugins to remove.".yellow());
        return Ok(());
    }

    let items: Vec<Item> = ep_keys.iter().map(|k| Item::new(k.clone(), "")).collect();
    let selected = crate::interactive::pick_many("Remove plugins (space to select)", &items)?;

    if selected.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    let names: Vec<String> = selected.iter().map(|&i| ep_keys[i].clone()).collect();
    run(names, false, purge_bytes)
}
