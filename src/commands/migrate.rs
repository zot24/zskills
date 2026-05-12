//! `migrate <project>` — promote a project's enabledPlugins + extraKnownMarketplaces
//! to user scope (~/.claude/settings.json), optionally removing them from the project.

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::Value;
use std::path::PathBuf;

pub fn run(project: PathBuf, remove_from_project: bool, dry_run: bool) -> Result<()> {
    let candidates = [
        project.join(".claude").join("settings.json"),
        project.join(".claude").join("settings.local.json"),
    ];
    let project_settings_path = candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .with_context(|| {
            format!(
                "no .claude/settings.json or settings.local.json found in {}",
                project.display()
            )
        })?;

    let project_settings = crate::settings::load(&project_settings_path)?;
    let user_settings_path = crate::paths::settings_json()?;
    let mut user_settings = crate::settings::load(&user_settings_path)?;

    let project_enabled = crate::settings::enabled_plugins(&project_settings)
        .cloned()
        .unwrap_or_default();
    let project_marketplaces = project_settings
        .get("extraKnownMarketplaces")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    if project_enabled.is_empty() && project_marketplaces.is_empty() {
        println!(
            "Nothing to migrate from {}.",
            project_settings_path.display()
        );
        return Ok(());
    }

    println!("{}", "Plan".bold());
    println!("  source: {}", project_settings_path.display());
    println!("  target: {}", user_settings_path.display());

    let user_ep = crate::settings::enabled_plugins(&user_settings)
        .cloned()
        .unwrap_or_default();
    let mut promote_skills = Vec::new();
    for (k, v) in &project_enabled {
        if v.as_bool() == Some(true) && !user_ep.contains_key(k) {
            promote_skills.push(k.clone());
        }
    }
    let mut promote_markets = Vec::new();
    let user_ekm = user_settings
        .get("extraKnownMarketplaces")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    for (k, _) in &project_marketplaces {
        if !user_ekm.contains_key(k) {
            promote_markets.push(k.clone());
        }
    }

    if promote_skills.is_empty() && promote_markets.is_empty() {
        println!("  (everything in the project is already present at user scope — nothing to do)");
    } else {
        for k in &promote_markets {
            println!("  {} promote marketplace {}", "+".green(), k);
        }
        for k in &promote_skills {
            println!("  {} promote skill       {}", "+".green(), k);
        }
    }

    if remove_from_project {
        let count = project_enabled
            .values()
            .filter(|v| v.as_bool() == Some(true))
            .count();
        if count > 0 {
            println!(
                "  {} clear {} skill(s) from project settings",
                "-".yellow(),
                count
            );
        }
    }

    if dry_run {
        println!("\n(dry-run; no changes written)");
        return Ok(());
    }

    // Apply: promote to user
    let user_ep_mut = crate::settings::enabled_plugins_mut(&mut user_settings);
    for k in &promote_skills {
        user_ep_mut.insert(k.clone(), Value::Bool(true));
    }
    let user_ekm_mut = crate::settings::extra_marketplaces_mut(&mut user_settings);
    for k in &promote_markets {
        if let Some(v) = project_marketplaces.get(k) {
            user_ekm_mut.insert(k.clone(), v.clone());
        }
    }
    crate::settings::save(&user_settings_path, &user_settings)?;

    if remove_from_project {
        let mut p = project_settings.clone();
        crate::settings::enabled_plugins_mut(&mut p).clear();
        // Also clear extraKnownMarketplaces, since the user is centralizing them.
        if p.contains_key("extraKnownMarketplaces") {
            crate::settings::extra_marketplaces_mut(&mut p).clear();
        }
        crate::settings::save(&project_settings_path, &p)?;
    }

    println!("\n{} migration complete.", "✓".green());
    Ok(())
}
