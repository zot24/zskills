//! Walk a directory tree, find every project that declares enabled skills,
//! and report the union. The complement of `migrate`.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct ProjectScan {
    pub path: PathBuf,
    pub enabled: Vec<String>,
    pub marketplaces: Vec<String>,
}

pub fn scan_path(root: &Path, max_depth: usize) -> Result<Vec<ProjectScan>> {
    let mut out = Vec::new();

    for entry in WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_noise(e.file_name().to_string_lossy().as_ref()))
    {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        let parent = entry.path().parent().unwrap_or(Path::new(""));
        let is_settings = (name == "settings.json" || name == "settings.local.json")
            && parent.file_name().and_then(|s| s.to_str()) == Some(".claude");
        if !is_settings {
            continue;
        }

        let Ok(bytes) = std::fs::read(entry.path()) else { continue };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else { continue };

        let mut enabled = Vec::new();
        if let Some(ep) = v.get("enabledPlugins").and_then(|x| x.as_object()) {
            for (k, val) in ep {
                if val.as_bool().unwrap_or(false) {
                    enabled.push(k.clone());
                }
            }
        }

        let mut marketplaces = Vec::new();
        if let Some(ekm) = v.get("extraKnownMarketplaces").and_then(|x| x.as_object()) {
            for k in ekm.keys() {
                marketplaces.push(k.clone());
            }
        }

        if enabled.is_empty() && marketplaces.is_empty() {
            continue;
        }

        let project = parent.parent().unwrap_or(parent).to_path_buf();
        out.push(ProjectScan {
            path: project,
            enabled,
            marketplaces,
        });
    }

    Ok(out)
}

fn is_noise(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "target"
            | ".git"
            | "dist"
            | "build"
            | ".next"
            | ".cache"
            | ".venv"
            | "venv"
            | "__pycache__"
    )
}

pub fn run(path: Option<PathBuf>, depth: usize, json_out: bool) -> Result<()> {
    let root = path.unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let projects = scan_path(&root, depth)?;

    if json_out {
        let arr: Vec<_> = projects
            .iter()
            .map(|p| {
                json!({
                    "path": p.path,
                    "enabled": p.enabled,
                    "marketplaces": p.marketplaces,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
        return Ok(());
    }

    if projects.is_empty() {
        println!("No project-scope skills found under {}", root.display());
        return Ok(());
    }

    println!(
        "{}",
        format!(
            "Found {} project(s) with skills under {}",
            projects.len(),
            root.display()
        )
        .bold()
    );

    // Skill -> [project paths]
    let mut by_skill: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for p in &projects {
        for s in &p.enabled {
            by_skill.entry(s.clone()).or_default().push(p.path.clone());
        }
    }

    for p in &projects {
        println!("\n{}", p.path.display().to_string().bold().cyan());
        if !p.marketplaces.is_empty() {
            println!("  marketplaces: {}", p.marketplaces.join(", ").dimmed());
        }
        for s in &p.enabled {
            println!("  • {}", s);
        }
    }

    println!("\n{}", "Skill → projects (cross-reference)".bold().green());
    for (s, paths) in &by_skill {
        println!("  {} ({})", s, paths.len());
    }

    println!(
        "\nTo promote a project's skills to user scope: {}",
        "zskills migrate <path>".bold()
    );

    Ok(())
}
