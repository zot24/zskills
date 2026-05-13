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
            if !crate::git::is_git_repo(&repo) {
                println!(
                    "{}",
                    "skipped (not a git working tree — managed by Claude Code)".dimmed()
                );
                continue;
            }
            match crate::git::pull(&repo) {
                Ok(()) => println!("{}", "ok".green()),
                Err(e) => println!("{} ({})", "fail".red(), e),
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
                match crate::agent_skill::install(src, entry.name.as_deref()) {
                    Ok(_) => println!("{}", "ok".green()),
                    Err(e) => println!("{} ({})", "fail".red(), e),
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
