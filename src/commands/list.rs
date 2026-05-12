use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::json;

pub fn run(json_out: bool) -> Result<()> {
    let report = crate::reconcile::run()?;
    let inv = crate::agent_skill::load_inventory()?;
    let on_disk = crate::agent_skill::installed_on_disk().unwrap_or_default();

    let managed: Vec<&String> = inv.agent_skills.keys().collect();
    let untracked: Vec<String> = on_disk
        .iter()
        .filter(|n| !inv.agent_skills.contains_key(n.as_str()))
        .cloned()
        .collect();

    if json_out {
        let out = json!({
            "plugins": {
                "active": report.active,
                "installed_disabled": report.installed_disabled,
                "enabled_orphan": report.enabled_orphan,
                "installed_orphan": report.installed_orphan,
            },
            "agent_skills": {
                "managed": managed,
                "untracked": untracked,
            }
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!(
        "{}",
        "Plugins — active (enabled + installed)".bold().green()
    );
    if report.active.is_empty() {
        println!("  (none)");
    } else {
        for k in &report.active {
            println!("  ✓ {}", k);
        }
    }

    if !report.installed_disabled.is_empty() {
        println!("\n{}", "Plugins — installed but disabled".bold().yellow());
        for k in &report.installed_disabled {
            println!("  • {}", k);
        }
    }

    if !report.enabled_orphan.is_empty() {
        println!(
            "\n{}",
            "Plugins — enabled but NOT installed (broken)".bold().red()
        );
        for k in &report.enabled_orphan {
            println!("  ✗ {}", k);
        }
    }

    if !report.installed_orphan.is_empty() {
        println!(
            "\n{}",
            "Plugins — installed from a marketplace that's no longer registered"
                .bold()
                .red()
        );
        for k in &report.installed_orphan {
            println!("  ✗ {}", k);
        }
    }

    println!("\n{}", "Agent Skills — managed by zskills".bold().green());
    if managed.is_empty() {
        println!("  (none)");
    } else {
        for k in &managed {
            let src = inv.agent_skills.get(k.as_str()).map(|e| e.source.as_str());
            match src {
                Some(s) => println!("  ✓ {}  {}", k, format!("← {}", s).dimmed()),
                None => println!("  ✓ {}", k),
            }
        }
    }

    if !untracked.is_empty() {
        println!(
            "\n{}",
            "Agent Skills — on disk but not managed by zskills"
                .bold()
                .yellow()
        );
        for k in &untracked {
            println!("  • {}", k);
        }
        println!(
            "  {}",
            "(add a [[agent_skills]] entry to your manifest to take ownership)".dimmed()
        );
    }

    Ok(())
}
