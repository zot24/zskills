//! Walk a directory tree, find every project that declares enabled skills,
//! and report the union. The complement of `migrate`.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct ScanMcp {
    pub name: String,
    pub kind: String,
}

#[derive(Debug)]
pub struct ProjectScan {
    pub path: PathBuf,
    pub enabled: Vec<String>,
    pub marketplaces: Vec<String>,
    /// Names of Agent Skills installed at `.claude/skills/<name>/`
    pub agent_skills: Vec<String>,
    /// MCP servers from `.mcp.json` and `.claude/settings*.json`. Empty unless
    /// the walk requested `--mcp`.
    pub mcps: Vec<ScanMcp>,
}

impl ProjectScan {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            enabled: vec![],
            marketplaces: vec![],
            agent_skills: vec![],
            mcps: vec![],
        }
    }
}

fn project_entry(
    by_project: &mut BTreeMap<PathBuf, ProjectScan>,
    project: PathBuf,
) -> &mut ProjectScan {
    by_project
        .entry(project.clone())
        .or_insert_with(|| ProjectScan::new(project))
}

fn merge_mcps(entry: &mut ProjectScan, servers: Vec<crate::mcp::McpServer>) {
    for s in servers {
        if !entry.mcps.iter().any(|m| m.name == s.name) {
            entry.mcps.push(ScanMcp {
                name: s.name,
                kind: s.transport.kind().to_string(),
            });
        }
    }
}

pub fn scan_path(root: &Path, max_depth: usize) -> Result<Vec<ProjectScan>> {
    scan_path_inner(root, max_depth, false)
}

fn scan_path_inner(root: &Path, max_depth: usize, include_mcp: bool) -> Result<Vec<ProjectScan>> {
    let mut by_project: BTreeMap<PathBuf, ProjectScan> = BTreeMap::new();

    for entry in WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_noise(e.file_name().to_string_lossy().as_ref()))
    {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let parent = path.parent().unwrap_or(Path::new(""));

        // Match 1: .claude/settings*.json files
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            let is_settings = (name == "settings.json" || name == "settings.local.json")
                && parent.file_name().and_then(|s| s.to_str()) == Some(".claude");
            if is_settings {
                let Ok(bytes) = std::fs::read(path) else {
                    continue;
                };
                let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                    continue;
                };

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

                let project = parent.parent().unwrap_or(parent).to_path_buf();
                let servers = if include_mcp {
                    crate::mcp::load_project_dir(&project)
                } else {
                    vec![]
                };

                if !enabled.is_empty() || !marketplaces.is_empty() || !servers.is_empty() {
                    let entry = project_entry(&mut by_project, project);
                    entry.enabled.extend(enabled);
                    for mp in marketplaces {
                        if !entry.marketplaces.contains(&mp) {
                            entry.marketplaces.push(mp);
                        }
                    }
                    merge_mcps(entry, servers);
                }
                continue;
            }

            // Match 1b: <project>/.mcp.json (wrapped or flat). Decoy `mcp.json`
            // without the leading dot is ignored on purpose.
            if include_mcp && name == ".mcp.json" {
                let project = parent.to_path_buf();
                let servers = crate::mcp::load_project_dir(&project);
                if !servers.is_empty() {
                    let entry = project_entry(&mut by_project, project);
                    merge_mcps(entry, servers);
                }
                continue;
            }
        }

        // Match 2: .claude/skills/<name>/SKILL.md  → agent skill at project scope
        if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
            // path: <project>/.claude/skills/<name>/SKILL.md
            // parent = <project>/.claude/skills/<name>
            let skill_name = match parent.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let grandparent = parent.parent();
            let greatgrand = grandparent.and_then(|p| p.parent());
            let is_dotclaude_skills = grandparent
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                == Some("skills")
                && greatgrand
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    == Some(".claude");
            if !is_dotclaude_skills {
                continue;
            }
            let project = greatgrand
                .and_then(|p| p.parent())
                .unwrap_or(Path::new(""))
                .to_path_buf();
            let entry = project_entry(&mut by_project, project);
            if !entry.agent_skills.contains(&skill_name) {
                entry.agent_skills.push(skill_name);
            }
        }
    }

    let mut out: Vec<_> = by_project.into_values().collect();
    for p in &mut out {
        p.enabled.sort();
        p.enabled.dedup();
        p.marketplaces.sort();
        p.marketplaces.dedup();
        p.agent_skills.sort();
        p.mcps.sort_by(|a, b| a.name.cmp(&b.name));
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

pub fn run(path: Option<PathBuf>, depth: usize, json_out: bool, include_mcp: bool) -> Result<()> {
    let root = path.unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let projects = scan_path_inner(&root, depth, include_mcp)?;

    if json_out {
        let arr: Vec<_> = projects
            .iter()
            .map(|p| {
                let mut obj = json!({
                    "path": p.path,
                    "enabled": p.enabled,
                    "marketplaces": p.marketplaces,
                    "agent_skills": p.agent_skills,
                });
                if include_mcp {
                    obj["mcps"] = json!(p
                        .mcps
                        .iter()
                        .map(|m| json!({ "name": m.name, "kind": m.kind }))
                        .collect::<Vec<_>>());
                }
                obj
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
        for s in &p.agent_skills {
            by_skill
                .entry(format!("{} (agent skill)", s))
                .or_default()
                .push(p.path.clone());
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
        for s in &p.agent_skills {
            println!("  ◦ {}  {}", s, "(agent skill)".dimmed());
        }
        for m in &p.mcps {
            println!("  mcp {} ({})", m.name, m.kind);
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
