//! `migrate-skill <name>` — promote ONE agent skill across many projects.
//!
//! Walks a tree, finds every project that has `.claude/skills/<name>/`,
//! picks the first as the canonical source, copies to user scope, and
//! optionally removes from all projects.

use anyhow::Result;
use owo_colors::OwoColorize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn run(
    name: String,
    root: Option<PathBuf>,
    source: Option<String>,
    remove_from_all: bool,
    dry_run: bool,
) -> Result<()> {
    let root = root.unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let projects = crate::commands::scan::scan_path(&root, 6)?;

    let occurrences: Vec<_> = projects
        .iter()
        .filter(|p| p.agent_skills.iter().any(|s| s == &name))
        .collect();

    if occurrences.is_empty() {
        anyhow::bail!(
            "skill '{}' not found in any project under {}",
            name,
            root.display()
        );
    }

    println!(
        "{}",
        format!("Found '{}' in {} project(s)", name, occurrences.len()).bold()
    );

    // Hash each project's copy to detect divergent content.
    let mut hashes: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for p in &occurrences {
        let dir = p.path.join(".claude").join("skills").join(&name);
        let h = hash_dir(&dir).unwrap_or_else(|_| "<unreadable>".to_string());
        hashes.entry(h).or_default().push(p.path.clone());
    }

    if hashes.len() > 1 {
        println!(
            "  {} content differs across projects — using the first as canonical:",
            "!".yellow()
        );
        for (h, paths) in &hashes {
            println!("    [{}]  {} project(s)", &h[..8.min(h.len())], paths.len());
            for p in paths {
                println!("      {}", p.display());
            }
        }
    } else {
        for p in &occurrences {
            println!("  {}", p.path.display().to_string().dimmed());
        }
    }

    let canonical = occurrences[0]
        .path
        .join(".claude")
        .join("skills")
        .join(&name);
    println!(
        "\n{}",
        format!("Canonical source: {}", canonical.display())
            .bold()
            .cyan()
    );

    let user_dir = crate::paths::user_skills_dir()?.join(&name);
    let already_global = user_dir.exists();
    if already_global {
        println!(
            "  {} '{}' is already at user scope — will overwrite with canonical",
            "•".yellow(),
            name
        );
    }

    println!("\n{}", "Plan".bold());
    if let Some(src) = &source {
        println!(
            "  {} install from upstream  source = {}  (replaces local copy)",
            "+".green(),
            src
        );
    } else {
        println!(
            "  {} copy canonical to ~/.claude/skills/{}/",
            "+".green(),
            name
        );
    }
    if remove_from_all {
        println!(
            "  {} remove project copies from {} project(s)",
            "-".yellow(),
            occurrences.len()
        );
    }
    println!("  {} append [[agent_skills]] to skills.toml", "+".green());

    if dry_run {
        println!("\n(dry-run; no changes written)");
        return Ok(());
    }

    // Apply: install
    if let Some(src) = &source {
        crate::agent_skill::install(src, Some(&name))?;
    } else {
        crate::agent_skill::install_to_user_dir(&name, &canonical)?;
        let mut inv = crate::agent_skill::load_inventory()?;
        inv.agent_skills.insert(
            name.clone(),
            crate::agent_skill::Entry {
                source: "local".to_string(),
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
        crate::agent_skill::save_inventory(&inv)?;
    }
    println!("  installed agent skill {}", name.bold());

    // Apply: remove from projects
    if remove_from_all {
        for p in &occurrences {
            let dir = p.path.join(".claude").join("skills").join(&name);
            if dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    eprintln!(
                        "  {} could not remove {}: {}",
                        "!".yellow(),
                        dir.display(),
                        e
                    );
                } else {
                    println!("  removed {}", dir.display().to_string().dimmed());
                }
            }
        }
    }

    // Apply: append to manifest
    let manifest_path = crate::manifest::discover().unwrap_or_else(|| {
        let dir = dirs::home_dir()
            .map(|h| h.join(".config").join("zskills"))
            .unwrap_or_else(|| PathBuf::from(".config/zskills"));
        dir.join("skills.toml")
    });
    let entry = crate::manifest::AgentSkillEntry {
        source: source.clone(),
        name: Some(name.clone()),
        ..Default::default()
    };
    match crate::manifest::append_agent_skill(&manifest_path, &entry) {
        Ok(true) => println!("  wrote entry to {}", manifest_path.display()),
        Ok(false) => println!("  entry already present in {}", manifest_path.display()),
        Err(e) => eprintln!("  {} couldn't write manifest: {}", "!".yellow(), e),
    }

    println!("\n{} migration complete.", "✓".green());
    Ok(())
}

fn hash_dir(dir: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    paths.sort();
    for p in &paths {
        let rel = p.strip_prefix(dir).unwrap_or(p);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update(b"\0");
        if let Ok(bytes) = std::fs::read(p) {
            hasher.update(&bytes);
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}
