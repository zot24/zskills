use anyhow::Result;
use owo_colors::OwoColorize;

pub fn run(fix: bool) -> Result<()> {
    let report = crate::reconcile::run()?;
    let mut issues = 0;

    if !report.enabled_orphan.is_empty() {
        issues += report.enabled_orphan.len();
        println!(
            "{} {} plugins enabled but not installed:",
            "✗".red(),
            report.enabled_orphan.len()
        );
        for k in &report.enabled_orphan {
            println!("  - {}", k);
        }
    }

    if !report.installed_orphan.is_empty() {
        issues += report.installed_orphan.len();
        println!(
            "{} {} plugins installed from missing marketplaces:",
            "✗".red(),
            report.installed_orphan.len()
        );
        for k in &report.installed_orphan {
            println!("  - {}", k);
        }
    }

    // Agent skills: entries in inventory but missing on disk
    let inv = crate::agent_skill::load_inventory()?;
    let on_disk: std::collections::BTreeSet<String> = crate::agent_skill::installed_on_disk()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let agent_inventory_missing: Vec<String> = inv
        .agent_skills
        .keys()
        .filter(|k| !on_disk.contains(k.as_str()))
        .cloned()
        .collect();
    if !agent_inventory_missing.is_empty() {
        issues += agent_inventory_missing.len();
        println!(
            "{} {} agent skills tracked in inventory but missing on disk:",
            "✗".red(),
            agent_inventory_missing.len()
        );
        for k in &agent_inventory_missing {
            println!("  - {}", k);
        }
    }

    if issues == 0 {
        println!(
            "{} All good — disk, inventory, and settings are in sync.",
            "✓".green()
        );
        return Ok(());
    }

    if fix {
        // Plugins: remove orphan enabledPlugins entries.
        let settings_path = crate::paths::settings_json()?;
        let mut settings = crate::settings::load(&settings_path)?;
        let ep = crate::settings::enabled_plugins_mut(&mut settings);
        for k in &report.enabled_orphan {
            ep.remove(k);
            println!("  removed {} from enabledPlugins", k);
        }
        crate::settings::save(&settings_path, &settings)?;

        // Agent skills: drop inventory entries with no bytes.
        let mut inv = crate::agent_skill::load_inventory()?;
        for k in &agent_inventory_missing {
            inv.agent_skills.remove(k);
            println!("  removed {} from agent-skill inventory", k);
        }
        crate::agent_skill::save_inventory(&inv)?;

        println!("{} Fixed {} issue(s).", "✓".green(), issues);
    } else {
        println!("\nRun {} to clean up.", "zskills doctor --fix".bold());
    }

    Ok(())
}
