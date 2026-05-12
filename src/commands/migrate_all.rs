//! `migrate-all <dir>` — interactive sweep.
//!
//! Scans the tree, groups by skill name, sorts by occurrence count, and asks
//! per-skill whether to promote it, what its upstream source is (if any), and
//! whether to clear project copies.

use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Confirm, Input};
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn run(dir: PathBuf, threshold: usize, assume_yes: bool, dry_run: bool) -> Result<()> {
    let projects = crate::commands::scan::scan_path(&dir, 6)?;
    if projects.is_empty() {
        println!("No project-scope skills found under {}", dir.display());
        return Ok(());
    }

    // Group: skill name -> list of projects
    let mut by_skill: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for p in &projects {
        for s in &p.agent_skills {
            by_skill.entry(s.clone()).or_default().push(p.path.clone());
        }
    }

    // Sort: most-duplicated first
    let mut entries: Vec<(String, Vec<PathBuf>)> = by_skill.into_iter().collect();
    entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));

    // Filter by threshold (skills appearing in >= threshold projects)
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|(_, v)| v.len() >= threshold)
        .collect();
    if entries.is_empty() {
        println!("No agent skills found with ≥{} occurrence(s).", threshold);
        return Ok(());
    }

    println!(
        "{}",
        format!(
            "Found {} unique agent skill(s) with ≥{} occurrence(s) under {}",
            entries.len(),
            threshold,
            dir.display()
        )
        .bold()
    );
    println!();

    let theme = ColorfulTheme::default();
    let mut promoted = 0usize;

    for (name, paths) in &entries {
        println!(
            "{}  {}",
            format!("[{} project(s)]", paths.len()).dimmed(),
            name.bold().cyan()
        );
        for p in paths {
            println!("    {}", p.display().to_string().dimmed());
        }

        let user_dir = crate::paths::user_skills_dir()?.join(name);
        if user_dir.exists() {
            println!(
                "    {} already exists at user scope ({})",
                "•".yellow(),
                user_dir.display()
            );
        }

        // Prompt: promote?
        let prompt_promote = if assume_yes {
            true
        } else {
            Confirm::with_theme(&theme)
                .with_prompt(format!("  Promote '{}' to user scope?", name))
                .default(true)
                .interact()
                .unwrap_or(false)
        };
        if !prompt_promote {
            println!();
            continue;
        }

        // Prompt: source?
        let source: Option<String> = if assume_yes {
            None
        } else {
            let raw: String = Input::with_theme(&theme)
                .with_prompt("  Upstream source [owner/repo, URL, or blank for local-only]")
                .allow_empty(true)
                .interact_text()
                .unwrap_or_default();
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        // Prompt: remove from projects?
        let remove = if assume_yes {
            false
        } else {
            Confirm::with_theme(&theme)
                .with_prompt(format!(
                    "  Remove project copies from {} project(s)?",
                    paths.len()
                ))
                .default(false)
                .interact()
                .unwrap_or(false)
        };

        if dry_run {
            println!(
                "    {} would promote (source={:?}, remove={})",
                "·".dimmed(),
                source,
                remove
            );
            println!();
            continue;
        }

        // Apply
        match crate::commands::migrate_skill::run(
            name.clone(),
            Some(dir.clone()),
            source.clone(),
            remove,
            false,
        ) {
            Ok(()) => {
                promoted += 1;
            }
            Err(e) => {
                eprintln!("    {} failed: {}", "✗".red(), e);
            }
        }
        println!();
    }

    println!("\n{} {} skill(s) promoted.", "✓".green(), promoted);
    Ok(())
}
