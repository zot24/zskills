//! `migrate <project>` — promote a project's enabledPlugins + extraKnownMarketplaces
//! to user scope (~/.claude/settings.json), optionally removing them from the project.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;
use std::path::PathBuf;

pub fn run(project: PathBuf, remove_from_project: bool, dry_run: bool) -> Result<()> {
    let candidates = [
        project.join(".claude").join("settings.json"),
        project.join(".claude").join("settings.local.json"),
    ];
    let project_settings_path = candidates.iter().find(|p| p.exists()).cloned();

    // Project-scope agent skills under .claude/skills/<name>/
    let project_agent_dir = project.join(".claude").join("skills");
    let project_agent_skills: Vec<(String, PathBuf)> = if project_agent_dir.exists() {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&project_agent_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && p.join("SKILL.md").exists() {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        out.push((name.to_string(), p));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    } else {
        Vec::new()
    };

    if project_settings_path.is_none() && project_agent_skills.is_empty() {
        anyhow::bail!(
            "no .claude/settings*.json or .claude/skills/ found in {}",
            project.display()
        );
    }

    let user_settings_path = crate::paths::settings_json()?;
    let mut user_settings = crate::settings::load(&user_settings_path)?;

    let (project_settings, project_enabled, project_marketplaces) =
        if let Some(p) = project_settings_path.as_ref() {
            let s = crate::settings::load(p)?;
            let ep = crate::settings::enabled_plugins(&s)
                .cloned()
                .unwrap_or_default();
            let mp = s
                .get("extraKnownMarketplaces")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            (Some(s), ep, mp)
        } else {
            (None, Default::default(), Default::default())
        };

    println!("{}", "Plan".bold());
    if let Some(p) = project_settings_path.as_ref() {
        println!("  settings source: {}", p.display());
    }
    if !project_agent_skills.is_empty() {
        println!("  agent-skills source: {}", project_agent_dir.display());
    }
    println!("  target: {}", user_settings_path.display());

    // Plan: plugins
    let user_ep = crate::settings::enabled_plugins(&user_settings)
        .cloned()
        .unwrap_or_default();
    let mut promote_skills = Vec::new();
    for (k, v) in &project_enabled {
        if v.as_bool() == Some(true) && !user_ep.contains_key(k) {
            promote_skills.push(k.clone());
        }
    }
    let user_ekm = user_settings
        .get("extraKnownMarketplaces")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut promote_markets = Vec::new();
    for k in project_marketplaces.keys() {
        if !user_ekm.contains_key(k) {
            promote_markets.push(k.clone());
        }
    }

    // Plan: agent skills
    let user_agent_on_disk: std::collections::BTreeSet<String> =
        crate::agent_skill::installed_on_disk()
            .unwrap_or_default()
            .into_iter()
            .collect();
    let mut promote_agent_skills: Vec<(String, PathBuf)> = Vec::new();
    for (name, dir) in &project_agent_skills {
        if !user_agent_on_disk.contains(name) {
            promote_agent_skills.push((name.clone(), dir.clone()));
        }
    }

    if promote_skills.is_empty() && promote_markets.is_empty() && promote_agent_skills.is_empty() {
        println!("  (everything in the project is already present at user scope — nothing to do)");
        if !remove_from_project {
            return Ok(());
        }
    } else {
        for k in &promote_markets {
            println!("  {} promote marketplace {}", "+".green(), k);
        }
        for k in &promote_skills {
            println!("  {} promote plugin      {}", "+".green(), k);
        }
        for (k, _) in &promote_agent_skills {
            println!("  {} promote agent skill {}", "+".green(), k);
        }
    }

    if remove_from_project {
        let count = project_enabled
            .values()
            .filter(|v| v.as_bool() == Some(true))
            .count();
        if count > 0 {
            println!(
                "  {} clear {} plugin(s) from project settings",
                "-".yellow(),
                count
            );
        }
        if !project_agent_skills.is_empty() {
            println!(
                "  {} clear {} agent skill(s) from project .claude/skills/",
                "-".yellow(),
                project_agent_skills.len()
            );
        }
    }

    if dry_run {
        println!("\n(dry-run; no changes written)");
        return Ok(());
    }

    // Apply: plugins
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

    // Apply: agent skills (copy directories + write inventory)
    if !promote_agent_skills.is_empty() {
        let mut inv = crate::agent_skill::load_inventory()?;
        for (name, dir) in &promote_agent_skills {
            crate::agent_skill::install_to_user_dir(name, dir)?;
            inv.agent_skills.insert(
                name.clone(),
                crate::agent_skill::Entry {
                    source: format!("local://{}", dir.to_string_lossy()),
                    installed_at: format!(
                        "@{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
                    ),
                    head_sha: "local".to_string(),
                },
            );
        }
        crate::agent_skill::save_inventory(&inv)?;
    }

    if remove_from_project {
        if let Some(p_settings) = project_settings {
            let mut p = p_settings.clone();
            crate::settings::enabled_plugins_mut(&mut p).clear();
            if p.contains_key("extraKnownMarketplaces") {
                crate::settings::extra_marketplaces_mut(&mut p).clear();
            }
            if let Some(path) = project_settings_path.as_ref() {
                crate::settings::save(path, &p)?;
            }
        }
        for (_, dir) in &project_agent_skills {
            if dir.exists() {
                std::fs::remove_dir_all(dir).ok();
            }
        }
    }

    println!("\n{} migration complete.", "✓".green());
    Ok(())
}
