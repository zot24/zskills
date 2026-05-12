use anyhow::Result;
use owo_colors::OwoColorize;

pub fn run(fix: bool) -> Result<()> {
    let report = crate::reconcile::run()?;
    let mut issues = 0;

    if !report.enabled_orphan.is_empty() {
        issues += report.enabled_orphan.len();
        println!(
            "{} {} skills enabled but not installed:",
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
            "{} {} skills installed from missing marketplaces:",
            "✗".red(),
            report.installed_orphan.len()
        );
        for k in &report.installed_orphan {
            println!("  - {}", k);
        }
    }

    if issues == 0 {
        println!("{} All good — disk, inventory, and settings are in sync.", "✓".green());
        return Ok(());
    }

    if fix {
        let settings_path = crate::paths::settings_json()?;
        let mut settings = crate::settings::load(&settings_path)?;
        let ep = crate::settings::enabled_plugins_mut(&mut settings);
        for k in &report.enabled_orphan {
            ep.remove(k);
            println!("  removed {} from enabledPlugins", k);
        }
        crate::settings::save(&settings_path, &settings)?;
        println!("{} Fixed {} issue(s).", "✓".green(), report.enabled_orphan.len());
    } else {
        println!(
            "\nRun {} to clean up.",
            "zskills doctor --fix".bold()
        );
    }

    Ok(())
}
