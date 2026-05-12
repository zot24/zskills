//! `remove` (apt-style: disable + drop inventory entry, keep bytes)
//! `purge` (also delete bytes from ~/.claude/plugins/cache/.../<plugin>)

use anyhow::Result;
use owo_colors::OwoColorize;

pub fn run(specs: Vec<String>, purge_bytes: bool) -> Result<()> {
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    let settings_path = crate::paths::settings_json()?;
    let inventory_path = crate::paths::installed_plugins_json()?;

    let mut settings = crate::settings::load(&settings_path)?;
    let mut inventory = crate::inventory::load(&inventory_path)?;

    for spec in &specs {
        let qualified = crate::marketplace::resolve_spec(spec, &known)
            .unwrap_or_else(|_| spec.to_string());

        // Remove from enabledPlugins
        let ep = crate::settings::enabled_plugins_mut(&mut settings);
        ep.remove(&qualified);

        // Remove from inventory and collect installPaths for purge
        let mut install_paths: Vec<std::path::PathBuf> = Vec::new();
        let plugins = crate::inventory::plugins_mut(&mut inventory);
        if let Some(entries) = plugins.remove(&qualified) {
            if purge_bytes {
                if let Some(arr) = entries.as_array() {
                    for entry in arr {
                        if let Some(p) = entry.get("installPath").and_then(|v| v.as_str()) {
                            install_paths.push(std::path::PathBuf::from(p));
                        }
                    }
                }
            }
        }

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
            println!("{} purged {}", "✗".red(), qualified);
        } else {
            println!("{} removed {}", "-".yellow(), qualified);
        }
    }

    crate::settings::save(&settings_path, &settings)?;
    crate::inventory::save(&inventory_path, &inventory)?;
    Ok(())
}
