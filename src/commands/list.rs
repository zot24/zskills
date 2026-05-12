use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::json;

pub fn run(json: bool) -> Result<()> {
    let report = crate::reconcile::run()?;

    if json {
        let out = json!({
            "active": report.active,
            "installed_disabled": report.installed_disabled,
            "enabled_orphan": report.enabled_orphan,
            "installed_orphan": report.installed_orphan,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("{}", "Active (installed + enabled)".bold().green());
    if report.active.is_empty() {
        println!("  (none)");
    } else {
        for k in &report.active {
            println!("  ✓ {}", k);
        }
    }

    if !report.installed_disabled.is_empty() {
        println!("\n{}", "Installed but disabled".bold().yellow());
        for k in &report.installed_disabled {
            println!("  • {}", k);
        }
    }

    if !report.enabled_orphan.is_empty() {
        println!("\n{}", "Enabled but NOT installed (broken)".bold().red());
        for k in &report.enabled_orphan {
            println!("  ✗ {}", k);
        }
    }

    if !report.installed_orphan.is_empty() {
        println!(
            "\n{}",
            "Installed from a marketplace that's no longer registered"
                .bold()
                .red()
        );
        for k in &report.installed_orphan {
            println!("  ✗ {}", k);
        }
    }

    Ok(())
}
