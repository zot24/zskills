use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::json;
use std::collections::BTreeMap;

pub fn run(json_out: bool, verbose: bool) -> Result<()> {
    let report = crate::reconcile::run()?;
    let inv = crate::agent_skill::load_inventory()?;
    let on_disk = crate::agent_skill::installed_on_disk().unwrap_or_default();

    let managed_names: Vec<&String> = inv.agent_skills.keys().collect();
    let untracked: Vec<String> = on_disk
        .iter()
        .filter(|n| !inv.agent_skills.contains_key(n.as_str()))
        .cloned()
        .collect();

    // Group managed agent skills by their `source` field.
    let mut by_source: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in &managed_names {
        let src = inv
            .agent_skills
            .get(name.as_str())
            .map(|e| e.source.as_str())
            .unwrap_or("?")
            .to_string();
        by_source.entry(src).or_default().push((*name).clone());
    }
    for v in by_source.values_mut() {
        v.sort();
    }

    if json_out {
        let groups: Vec<_> = by_source
            .iter()
            .map(|(src, names)| {
                json!({
                    "source": src,
                    "skills": names,
                    "count": names.len(),
                })
            })
            .collect();
        let out = json!({
            "plugins": {
                "active": report.active,
                "installed_disabled": report.installed_disabled,
                "enabled_orphan": report.enabled_orphan,
                "installed_orphan": report.installed_orphan,
            },
            "agent_skills": {
                "managed": groups,
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
    if by_source.is_empty() {
        println!("  (none)");
    } else {
        for (src, names) in &by_source {
            print_group(src, names, verbose);
        }
    }

    if !untracked.is_empty() {
        println!(
            "\n{}",
            "Agent Skills — on disk but not managed by zskills"
                .bold()
                .yellow()
        );
        // Hint when many untracked skills share a common prefix — likely one package.
        if let Some((prefix, count)) = common_prefix_summary(&untracked) {
            if count >= 5 {
                println!(
                    "  {} {} skill(s) share the prefix '{}' — likely from a single package.",
                    "ℹ".cyan(),
                    count,
                    prefix
                );
            }
        }
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

fn print_group(source: &str, names: &[String], verbose: bool) {
    let count = names.len();
    if count == 1 {
        println!("  ✓ {}  {}", names[0], format!("← {}", source).dimmed());
        return;
    }
    println!(
        "  ✓ {}  {}",
        source.bold(),
        format!("({} skills)", count).dimmed()
    );
    if verbose || count <= 5 {
        for n in names {
            println!("      • {}", n);
        }
    } else {
        let preview: Vec<&str> = names.iter().take(5).map(|s| s.as_str()).collect();
        println!(
            "      {} {}",
            preview.join(", ").dimmed(),
            format!("… [-v to list all {}]", count).dimmed()
        );
    }
}

/// Find the longest prefix shared by ≥3 entries; report how many share it.
/// Heuristic for the "this looks like one package" hint in the untracked section.
fn common_prefix_summary(names: &[String]) -> Option<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for n in names {
        if let Some((prefix, _)) = n.split_once('-') {
            *counts.entry(format!("{}-", prefix)).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .filter(|(_, c)| *c >= 3)
}
