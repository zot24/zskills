//! `zskills skill` verbs.

use anyhow::Result;
use owo_colors::OwoColorize;
use std::path::PathBuf;

pub fn install(
    specs: Vec<String>,
    interactive: bool,
    all: bool,
    skill: Option<String>,
    harness: Vec<crate::harness::Harness>,
) -> Result<()> {
    if specs
        .iter()
        .any(|s| !crate::commands::install::is_repo_spec(s))
    {
        anyhow::bail!(
            "skill install takes owner/repo or a git URL; use `zskills plugin install` for name@marketplace"
        );
    }
    crate::commands::install::run(specs, interactive, all, skill, harness)
}

pub fn upgrade(names: Vec<String>) -> Result<()> {
    crate::commands::upgrade::run(names)
}

pub fn remove(names: Vec<String>, force: bool, file: Option<PathBuf>) -> Result<()> {
    if names.is_empty() {
        anyhow::bail!("specify at least one Agent Skill name");
    }
    let manifest_path = file.or_else(crate::manifest::discover);
    let manifest = match manifest_path.as_ref() {
        Some(p) => Some(crate::manifest::load(p)?),
        None => None,
    };
    let inv = crate::agent_skill::load_inventory()?;
    let report = crate::reconcile::run()?;
    let from_plugins = crate::agent_skill::plugin_provided_skills(&report.active);

    let mut failed = 0usize;
    for name in &names {
        if let Err(e) = remove_one(
            name,
            force,
            manifest_path.as_deref(),
            manifest.as_ref(),
            &inv,
            &from_plugins,
        ) {
            eprintln!("{} {name}: {e}", "✗".red());
            failed += 1;
        }
    }
    anyhow::ensure!(failed == 0, "{failed} Agent Skill remove(s) failed");
    Ok(())
}

fn remove_one(
    name: &str,
    force: bool,
    manifest_path: Option<&std::path::Path>,
    manifest: Option<&crate::manifest::Manifest>,
    inv: &crate::agent_skill::Inventory,
    from_plugins: &std::collections::BTreeSet<String>,
) -> Result<()> {
    crate::agent_skill::validate_skill_name(name)?;
    if from_plugins.contains(name) && !force {
        anyhow::bail!(
            "Agent Skill '{name}' is also shipped by an enabled plugin; removing the user copy will not remove it — use `zskills plugin remove`, or --force"
        );
    }
    if let (Some(entry), Some(m)) = (inv.agent_skills.get(name), manifest) {
        let source_only = m
            .agent_skills
            .iter()
            .any(|e| e.name.is_none() && e.source.as_deref() == Some(entry.source.as_str()));
        if source_only && !force {
            anyhow::bail!(
                "Agent Skill '{name}' is owned by a source-only [[agent_skills]] row (source = {}); convert to a named row first, or pass --force",
                entry.source
            );
        }
    }

    if !crate::agent_skill::remove(name)? {
        anyhow::bail!("Agent Skill '{name}' is not installed");
    }
    if let Some(path) = manifest_path {
        let _ = crate::manifest::drop_named_agent_skill(path, name)?;
    }
    println!("{} removed Agent Skill {}", "-".yellow(), name.bold());
    Ok(())
}
