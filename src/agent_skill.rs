//! Agent Skills (the older raw-SKILL.md format).
//!
//! Install model:
//! - source repos live at $XDG_CACHE_HOME/zskills/agent-skills/<owner>-<repo>/
//! - installed skill trees live at ~/.claude/skills/<name>/
//! - our inventory lives at ~/.claude/skills/.zskills.json
//!
//! Repo convention we recognize: `skills/<skill-name>/SKILL.md` under the source repo.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Inventory {
    pub version: u32,
    #[serde(default)]
    pub agent_skills: BTreeMap<String, Entry>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Entry {
    pub source: String,
    pub installed_at: String,
    pub head_sha: String,
}

pub fn load_inventory() -> Result<Inventory> {
    let path = crate::paths::agent_skills_inventory()?;
    if !path.exists() {
        return Ok(Inventory {
            version: 1,
            agent_skills: BTreeMap::new(),
        });
    }
    let bytes = std::fs::read(&path)?;
    Ok(serde_json::from_slice(&bytes).unwrap_or(Inventory {
        version: 1,
        agent_skills: BTreeMap::new(),
    }))
}

pub fn save_inventory(inv: &Inventory) -> Result<()> {
    let path = crate::paths::agent_skills_inventory()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(serde_json::to_string_pretty(inv)?.as_bytes())?;
    tmp.write_all(b"\n")?;
    tmp.flush()?;
    tmp.persist(&path)?;
    Ok(())
}

/// Parse `owner/repo` or a full git URL into (clone_url, cache_dir_name).
pub fn parse_source(source: &str) -> Result<(String, String)> {
    if source.contains("://") || source.starts_with("git@") {
        let stem = source
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or(source)
            .to_string();
        Ok((source.to_string(), sanitize(&stem)))
    } else if source.contains('/') && !source.starts_with('/') {
        let url = format!("https://github.com/{}.git", source);
        Ok((url, sanitize(source)))
    } else {
        anyhow::bail!(
            "unrecognized agent-skill source: {} (expected owner/repo or git URL)",
            source
        )
    }
}

fn sanitize(s: &str) -> String {
    s.replace(['/', ':', '@'], "-")
}

/// Ensure the source repo is cloned/up-to-date in cache; return the cache path.
pub fn ensure_cache(source: &str) -> Result<PathBuf> {
    let (url, cache_name) = parse_source(source)?;
    let cache_root = crate::paths::agent_skills_cache_dir()?;
    std::fs::create_dir_all(&cache_root).ok();
    let cache = cache_root.join(&cache_name);
    if cache.exists() {
        crate::git::pull(&cache).ok(); // best-effort
    } else {
        crate::git::clone(&url, &cache).context("cloning agent-skill source repo")?;
    }
    Ok(cache)
}

/// List the skill directories present under <cache>/skills/.
/// Returns `(name, source_dir)` pairs.
pub fn skills_in_cache(cache: &Path) -> Vec<(String, PathBuf)> {
    let skills_root = cache.join("skills");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&skills_root) else {
        // Fallback: single-skill repo at root level
        if cache.join("SKILL.md").exists() {
            if let Some(name) = cache.file_name().and_then(|n| n.to_str()) {
                out.push((name.to_string(), cache.to_path_buf()));
            }
        }
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("SKILL.md").exists() {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                out.push((name.to_string(), p));
            }
        }
    }
    out
}

/// Copy a skill directory into ~/.claude/skills/<name>/ (deletes existing first).
pub fn install_to_user_dir(skill_name: &str, src_dir: &Path) -> Result<()> {
    let dest = crate::paths::user_skills_dir()?.join(skill_name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&dest)?;
    copy_dir_recursive(src_dir, &dest)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(src).follow_links(false) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Remove an installed agent skill from ~/.claude/skills/<name>/.
pub fn remove_from_user_dir(skill_name: &str) -> Result<()> {
    let dest = crate::paths::user_skills_dir()?.join(skill_name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    Ok(())
}

/// What's currently present in ~/.claude/skills/ (directories with SKILL.md).
pub fn installed_on_disk() -> Result<Vec<String>> {
    let dir = crate::paths::user_skills_dir()?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() && p.join("SKILL.md").exists() {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

/// Install (or refresh) an Agent Skill from a source repo. If `name` is given,
/// only that skill is installed; otherwise all skills under `skills/` are.
/// Returns the list of installed skill names.
pub fn install(source: &str, name: Option<&str>) -> Result<Vec<String>> {
    let cache = ensure_cache(source)?;
    let head_sha = crate::git::head_sha(&cache).unwrap_or_else(|_| "unknown".to_string());
    let installed_at = chrono_now_iso();
    let available = skills_in_cache(&cache);
    if available.is_empty() {
        anyhow::bail!(
            "no skills found in {} (expected skills/<name>/SKILL.md)",
            source
        );
    }
    let chosen: Vec<_> = match name {
        Some(n) => available
            .into_iter()
            .filter(|(k, _)| k == n)
            .collect::<Vec<_>>(),
        None => available,
    };
    if chosen.is_empty() {
        anyhow::bail!(
            "skill '{}' not found in {} (skills/<name>/ not present)",
            name.unwrap_or("?"),
            source
        );
    }
    let mut inv = load_inventory()?;
    let mut installed_names = Vec::new();
    for (skill_name, src_dir) in &chosen {
        install_to_user_dir(skill_name, src_dir)?;
        inv.agent_skills.insert(
            skill_name.clone(),
            Entry {
                source: source.to_string(),
                installed_at: installed_at.clone(),
                head_sha: head_sha.clone(),
            },
        );
        installed_names.push(skill_name.clone());
    }
    save_inventory(&inv)?;
    Ok(installed_names)
}

pub fn remove(skill_name: &str) -> Result<bool> {
    let mut inv = load_inventory()?;
    let removed_from_inventory = inv.agent_skills.remove(skill_name).is_some();
    remove_from_user_dir(skill_name)?;
    save_inventory(&inv)?;
    Ok(removed_from_inventory)
}

fn chrono_now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("@{}", now)
}
