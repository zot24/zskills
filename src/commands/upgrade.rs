//! `zskills upgrade [<name>...]` — single command to refresh every managed thing.
//!
//! - Marketplaces: `git pull` every tap (so Claude Code sees new plugin versions next start)
//! - Git agent skills: re-pull source + re-copy bytes
//! - npm agent skills: re-run `npm install -g <pkg>` (or custom install_cmd)

use anyhow::Result;
use owo_colors::OwoColorize;

pub fn run(filter: Vec<String>) -> Result<()> {
    let manifest_path = crate::manifest::discover();

    // ── Marketplaces ────────────────────────────────────────────────────
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    let pins = crate::marketplace::load_pins()?;
    if !known.is_empty() {
        println!("{}", "Marketplaces".bold());
        for name in known.keys() {
            if !filter.is_empty() && !filter.iter().any(|f| f == name) {
                continue;
            }
            let repo = crate::paths::marketplaces_dir()?.join(name);
            if !repo.exists() {
                continue;
            }
            print!("  {} {} ... ", "↻".cyan(), name);
            match crate::marketplace::refresh(name, &repo, pins.get(name).map(String::as_str)) {
                Ok(outcome) => println!("{}", crate::marketplace::refresh_label(&outcome).green()),
                Err(e) => println!("{} ({:#})", "fail".red(), e),
            }
        }
    }

    // ── Agent skills (from manifest) ────────────────────────────────────
    let manifest = match manifest_path.as_ref() {
        Some(p) => crate::manifest::load(p)?,
        None => crate::manifest::Manifest::default(),
    };
    if !manifest.agent_skills.is_empty() {
        println!("\n{}", "Agent Skills".bold());
        for entry in &manifest.agent_skills {
            // Apply --name filter against any of: npm, source, name
            if !filter.is_empty() {
                let candidates: Vec<&str> = [
                    entry.npm.as_deref(),
                    entry.source.as_deref(),
                    entry.name.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect();
                if !filter.iter().any(|f| candidates.contains(&f.as_str())) {
                    continue;
                }
            }

            if let Some(pkg) = entry.npm.as_deref() {
                print!("  {} npm:{} ... ", "↻".cyan(), pkg);
                match crate::agent_skill::upgrade_npm(
                    pkg,
                    entry.install_cmd.as_deref(),
                    &entry.claims,
                ) {
                    Ok(owned) => {
                        println!(
                            "{} {}",
                            "ok".green(),
                            format!("({} skills owned)", owned.len()).dimmed()
                        );
                    }
                    Err(e) => println!("{} ({})", "fail".red(), e),
                }
                continue;
            }

            if let Some(src) = entry.source.as_deref() {
                let label = entry.name.as_deref().unwrap_or(src);
                print!("  {} {} ... ", "↻".cyan(), label);

                // A source-only entry means "keep what I already own from this source",
                // not "adopt whatever it ships today". `install(src, None)` installs
                // every skill the survey finds, with no cap and no prompt — so widening
                // the survey (a new skill root, or upstream simply adding skills) would
                // otherwise make an unattended `upgrade` install things nobody asked for.
                // Refresh the names already in the inventory instead.
                let tag = entry.inventory_tag().unwrap_or_else(|| src.to_string());
                let origin = crate::agent_skill::SkillOrigin::git(src, entry.path.clone());
                let owned: Vec<String> = match entry.name.as_deref() {
                    Some(n) => vec![n.to_string()],
                    None => {
                        let inv = crate::agent_skill::load_inventory().unwrap_or_default();
                        inv.agent_skills
                            .iter()
                            .filter(|(_, e)| e.source == tag)
                            .map(|(n, _)| n.clone())
                            .collect()
                    }
                };

                if owned.is_empty() {
                    // Nothing owned from this source yet: `sync` is what adopts, and it
                    // applies the size policy. Upgrading here would bypass it.
                    println!(
                        "{}",
                        "nothing owned yet — run `zskills sync` to install".dimmed()
                    );
                    continue;
                }

                let mut failures = 0;
                for name in &owned {
                    if let Err(e) = crate::agent_skill::install_from(&origin, Some(name)) {
                        eprintln!("\n  {} {}: {}", "✗".red(), name, e);
                        failures += 1;
                    }
                }
                if failures == 0 {
                    println!("{}", format!("ok ({})", owned.len()).green());
                } else {
                    println!(
                        "{}",
                        format!("{} of {} failed", failures, owned.len()).red()
                    );
                }
                continue;
            }

            // Local-only entries have nothing to upgrade
            if let Some(name) = entry.name.as_deref() {
                println!(
                    "  {} {}  {}",
                    "·".dimmed(),
                    name,
                    "(local-only, skipped)".dimmed()
                );
            }
        }
    }

    println!(
        "\n{} Upgrade complete. Restart Claude Code to pick up new plugin bytes.",
        "✓".green()
    );
    Ok(())
}
