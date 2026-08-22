//! Agent Skills (the raw-SKILL.md format).
//!
//! Install model:
//! - source repos live at $XDG_CACHE_HOME/zskills/agent-skills/<owner>-<repo>/
//! - installed skill trees live at ~/.agents/skills/<name>/ (cross-client convention
//!   from <https://agentskills.io/integrate-skills>; visible to Claude Code, Grok CLI,
//!   and any other compliant client)
//! - our inventory lives at ~/.agents/skills/.zskills.json
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
/// Returns `(name, source_dir)` pairs, sorted by name.
///
/// Two layouts are supported:
/// - flat:   `skills/<name>/SKILL.md`
/// - nested: `skills/<category>/<name>/SKILL.md`
///
/// Nested is what larger collections use to group skills (e.g. `engineering/`,
/// `productivity/`). A directory holding its own `SKILL.md` is always treated as
/// a skill and is never descended into, so a skill containing helper
/// subdirectories cannot be mistaken for a category.
///
/// Names are unique in the result: if two categories expose the same skill name,
/// the first by sorted path wins and the other is dropped.
/// Roots a repository may keep its Agent Skills under, in precedence order.
///
/// `.agents/skills/` is the cross-client convention from the Agent Skills spec and is
/// what `warpdotdev/common-skills` uses; `skills/` is the older layout this tool shipped
/// with. A repository may legitimately use either, so both are walked.
const SKILL_ROOTS: &[&str] = &[".agents/skills", "skills"];

/// List the skill directories a cloned repo provides. Returns `(name, source_dir)` pairs,
/// sorted by name and de-duplicated.
///
/// Each root is walked the same way: a directory holding its own `SKILL.md` is a skill and
/// is never descended into, so a skill with helper subdirectories is not mistaken for a
/// category; a directory without one is treated as a category and searched one level
/// deeper. When no root yields anything, a repository-root `SKILL.md` makes the clone
/// itself the single skill.
pub fn skills_in_cache(cache: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for root in SKILL_ROOTS {
        collect_skills_under(&cache.join(root), &mut out);
    }

    if out.is_empty() {
        // Fallback: single-skill repo with SKILL.md at the root.
        if cache.join("SKILL.md").exists() {
            if let Some(name) = cache.file_name().and_then(|n| n.to_str()) {
                out.push((name.to_string(), cache.to_path_buf()));
            }
        }
        return out;
    }

    // Sorting by (name, path) then de-duplicating by name keeps the winner stable when
    // two roots offer the same name: `.agents/skills` sorts before `skills`, so the
    // cross-client layout wins, and the choice does not depend on read_dir order.
    out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Walk one skill root, appending every skill directory it holds.
fn collect_skills_under(root: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    fn push_skill(out: &mut Vec<(String, PathBuf)>, dir: &Path) {
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            out.push((name.to_string(), dir.to_path_buf()));
        }
    }

    // Sort the top level so category traversal is deterministic across platforms.
    let mut top: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    top.sort();

    for p in top {
        if p.join("SKILL.md").exists() {
            push_skill(out, &p);
            continue;
        }
        // Category directory: look one level deeper for <category>/<name>/SKILL.md.
        let Ok(inner) = std::fs::read_dir(&p) else {
            continue;
        };
        let mut nested: Vec<PathBuf> = inner
            .flatten()
            .map(|e| e.path())
            .filter(|q| q.is_dir() && q.join("SKILL.md").exists())
            .collect();
        nested.sort();
        for q in nested {
            push_skill(out, &q);
        }
    }
}

/// Copy a skill directory into ~/.agents/skills/<name>/ (deletes existing first).
pub fn install_to_user_dir(skill_name: &str, src_dir: &Path) -> Result<()> {
    let dest = crate::paths::user_skills_dir()?.join(skill_name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&dest)?;
    copy_dir_recursive(src_dir, &dest)?;
    Ok(())
}

/// Skill dirs that are copied wholesale for a root-level skill even when
/// SKILL.md doesn't link to them — the conventional layout from the spec.
const CONVENTIONAL_SKILL_DIRS: &[&str] = &["references", "assets", "scripts"];

/// Sparse-install a **root-level** skill (SKILL.md at the repo root of a larger
/// project): materialize only SKILL.md, the conventional skill dirs, and the
/// relative paths SKILL.md references — never the whole source tree.
pub fn install_root_skill_sparse(skill_name: &str, cache: &Path) -> Result<()> {
    let dest = crate::paths::user_skills_dir()?.join(skill_name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&dest)?;
    for rel in sparse_root_paths(cache) {
        let src = cache.join(&rel);
        let target = dest.join(&rel);
        if src.is_dir() {
            copy_dir_recursive(&src, &target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &target)?;
        }
    }
    Ok(())
}

/// Relative paths (against the repo root) to materialize for a root-level
/// skill: SKILL.md, present conventional dirs, and referenced paths that
/// actually exist in the clone.
pub(crate) fn sparse_root_paths(cache: &Path) -> Vec<PathBuf> {
    let mut set: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    set.insert(PathBuf::from("SKILL.md"));
    for dir in CONVENTIONAL_SKILL_DIRS {
        if cache.join(dir).is_dir() {
            set.insert(PathBuf::from(dir));
        }
    }
    if let Ok(md) = std::fs::read_to_string(cache.join("SKILL.md")) {
        for rel in referenced_relative_paths(&md) {
            if cache.join(&rel).exists() {
                set.insert(PathBuf::from(rel));
            }
        }
    }
    set.into_iter().collect()
}

/// Extract candidate relative paths from SKILL.md: markdown link targets and
/// inline-code spans. Liberal by design — callers filter candidates through
/// "does this path exist in the clone?" — but rejects anything unsafe or
/// clearly not a repo path (URLs, absolute paths, `..` escapes, anchors).
pub(crate) fn referenced_relative_paths(skill_md: &str) -> Vec<String> {
    fn accept(candidate: &str) -> Option<String> {
        let c = candidate.trim().trim_start_matches("./");
        let c = c.split('#').next().unwrap_or("");
        let c = c.trim_end_matches('/');
        if c.is_empty()
            || c.contains("://")
            || c.starts_with('/')
            || c.starts_with("mailto:")
            || c.contains(char::is_whitespace)
        {
            return None;
        }
        let escapes = Path::new(c)
            .components()
            .any(|comp| matches!(comp, std::path::Component::ParentDir));
        if escapes {
            return None;
        }
        Some(c.to_string())
    }

    let mut out = Vec::new();

    // Markdown link targets: `](target)`.
    let mut rest = skill_md;
    while let Some(i) = rest.find("](") {
        rest = &rest[i + 2..];
        let Some(j) = rest.find(')') else { break };
        if let Some(p) = accept(&rest[..j]) {
            out.push(p);
        }
        rest = &rest[j + 1..];
    }

    // Inline code spans on prose lines: `path/to/thing`. Skip fenced blocks,
    // and require the span to look path-like (a `/` or an extension dot) so
    // bare words like `install` don't become candidates.
    let mut in_fence = false;
    for line in skill_md.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for (idx, span) in line.split('`').enumerate() {
            if idx % 2 == 1 && (span.contains('/') || span.contains('.')) {
                if let Some(p) = accept(span) {
                    out.push(p);
                }
            }
        }
    }

    out
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    let walker = walkdir::WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || e.file_name() != ".git");
    for entry in walker {
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

/// Remove an installed agent skill from ~/.agents/skills/<name>/.
pub fn remove_from_user_dir(skill_name: &str) -> Result<()> {
    let dest = crate::paths::user_skills_dir()?.join(skill_name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    Ok(())
}

/// Install an npm-based agent skill. Runs `npm install -g <pkg>` (or `install_cmd`),
/// then determines ownership of on-disk skills via:
///
/// 1. diff `~/.agents/skills/` before/after (catches packages that place new files)
/// 2. glob-match `claims` patterns (catches packages that update pre-existing files)
/// 3. preserve existing inventory tags for this source
///
/// Returns the list of skills now claimed (sorted).
pub fn install_npm(
    package: &str,
    install_cmd: Option<&str>,
    claims: &[String],
) -> Result<Vec<String>> {
    if which::which("npm").is_err() && install_cmd.is_none() {
        anyhow::bail!("npm not found on PATH. Install Node.js, or set install_cmd for this entry.");
    }

    let before: std::collections::BTreeSet<String> = installed_on_disk()
        .unwrap_or_default()
        .into_iter()
        .collect();

    run_install_command(package, install_cmd)?;

    let after: std::collections::BTreeSet<String> = installed_on_disk()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let now = format!(
        "@{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let pkg_version = npm_installed_version(package).unwrap_or_else(|_| "unknown".to_string());
    let source_tag = format!("npm:{}", package);

    let mut owned: std::collections::BTreeSet<String> =
        after.difference(&before).cloned().collect();

    for pattern in claims {
        for name in &after {
            if glob_match(pattern, name) {
                owned.insert(name.clone());
            }
        }
    }

    let mut inv = load_inventory()?;
    for (name, entry) in &inv.agent_skills {
        if entry.source == source_tag && after.contains(name) {
            owned.insert(name.clone());
        }
    }

    let to_drop: Vec<String> = inv
        .agent_skills
        .iter()
        .filter(|(name, e)| e.source == source_tag && !after.contains(name.as_str()))
        .map(|(name, _)| name.clone())
        .collect();
    for name in &to_drop {
        inv.agent_skills.remove(name);
    }

    for n in &owned {
        inv.agent_skills.insert(
            n.clone(),
            Entry {
                source: source_tag.clone(),
                installed_at: now.clone(),
                head_sha: pkg_version.clone(),
            },
        );
    }
    save_inventory(&inv)?;

    let mut out: Vec<String> = owned.into_iter().collect();
    out.sort();
    Ok(out)
}

/// Re-run install (idempotent; same logic). Re-claims `claims` patterns each time.
pub fn upgrade_npm(
    package: &str,
    install_cmd: Option<&str>,
    claims: &[String],
) -> Result<Vec<String>> {
    install_npm(package, install_cmd, claims)
}

/// Minimal glob: `*` matches any sequence within a name (no `/`). Enough for `gsd-*` etc.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    if !name.starts_with(parts[0]) {
        return false;
    }
    let mut pos = parts[0].len();
    for seg in &parts[1..parts.len() - 1] {
        if seg.is_empty() {
            continue;
        }
        match name[pos..].find(seg) {
            Some(i) => pos += i + seg.len(),
            None => return false,
        }
    }
    name[pos..].ends_with(parts[parts.len() - 1])
}

fn run_install_command(package: &str, install_cmd: Option<&str>) -> Result<()> {
    if let Some(cmd_line) = install_cmd {
        let mut parts = cmd_line.split_whitespace();
        let bin = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty install_cmd"))?;
        let args: Vec<&str> = parts.collect();
        let status = std::process::Command::new(bin)
            .args(&args)
            .status()
            .with_context(|| format!("running custom install_cmd: {}", cmd_line))?;
        anyhow::ensure!(status.success(), "install_cmd failed: {}", cmd_line);
        return Ok(());
    }

    // `--no-fund --no-audit` silences npm's footer chatter so zskills's own
    // output stays signal-only; we don't manage funding info or audit advice.
    let status = std::process::Command::new("npm")
        .args(["install", "-g", "--no-fund", "--no-audit", package])
        .status()
        .with_context(|| format!("running npm install -g {}", package))?;
    anyhow::ensure!(status.success(), "npm install -g {} failed", package);
    Ok(())
}

fn npm_installed_version(package: &str) -> Result<String> {
    let out = std::process::Command::new("npm")
        .args(["list", "-g", "--depth=0", "--json", package])
        .output()
        .context("running npm list")?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let ver = v
        .pointer(&format!("/dependencies/{}/version", package))
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("npm list did not report a version for {}", package))?;
    Ok(ver)
}

/// What's currently present in ~/.agents/skills/ (directories with SKILL.md).
/// Skill names shipped by plugins that are installed **and** enabled.
///
/// A plugin carries its skills inside its own cache directory, so a copy of the same
/// name under `~/.agents/skills/` is not an orphan waiting to be adopted — the plugin
/// already owns that name. Reporting it as unmanaged asks the user to write a manifest
/// entry that would fight the plugin for it.
///
/// Only *active* plugins count. A disabled plugin contributes nothing at runtime, so a
/// skill left on disk after it was disabled really is unmanaged.
pub fn plugin_provided_skills(active: &[String]) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let Ok(cache) = crate::paths::plugins_dir().map(|p| p.join("cache")) else {
        return out;
    };
    // Which version of each plugin is actually installed. The cache keeps old versions
    // alongside the current one, and unioning across all of them would hide a genuinely
    // orphaned copy forever the first time an upgrade *drops* a skill: the name would
    // still be found under the stale version directory.
    let installed: std::collections::BTreeMap<String, String> =
        crate::paths::installed_plugins_json()
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| v.get("plugins").cloned())
            .and_then(|p| p.as_object().cloned())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| {
                        let ver = v.as_array()?.first()?.get("version")?.as_str()?.to_string();
                        Some((k.clone(), ver))
                    })
                    .collect()
            })
            .unwrap_or_default();

    for qualified in active {
        let Some((plugin, marketplace)) = qualified.rsplit_once('@') else {
            continue;
        };
        // cache/<marketplace>/<plugin>/<version>/skills/<name>
        let base = cache.join(marketplace).join(plugin);
        let dirs: Vec<std::path::PathBuf> = match installed.get(qualified) {
            Some(v) => vec![base.join(v)],
            // No recorded version: fall back to every cached version rather than
            // reporting a plugin's own skills as orphans.
            None => std::fs::read_dir(&base)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .collect(),
        };
        for d in dirs {
            let Ok(skills) = std::fs::read_dir(d.join("skills")) else {
                continue;
            };
            for sk in skills.flatten() {
                if sk.path().is_dir() {
                    if let Some(n) = sk.file_name().to_str() {
                        out.insert(n.to_string());
                    }
                }
            }
        }
    }
    out
}

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
        if src_dir == &cache {
            // Root-level SKILL.md in a larger project — materialize sparsely
            // instead of copying the whole source tree.
            install_root_skill_sparse(skill_name, &cache)?;
        } else {
            install_to_user_dir(skill_name, src_dir)?;
        }
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

#[cfg(test)]
mod tests {
    use super::{glob_match, referenced_relative_paths, sparse_root_paths};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn referenced_paths_from_markdown_links() {
        let md = "See [the guide](references/guide.md) and [docs](docs/setup.md).\n";
        let got = referenced_relative_paths(md);
        assert!(got.contains(&"references/guide.md".to_string()));
        assert!(got.contains(&"docs/setup.md".to_string()));
    }

    #[test]
    fn referenced_paths_from_inline_code_spans() {
        let md = "Run `scripts/setup.sh` first, then read `NOTES.md`.\n";
        let got = referenced_relative_paths(md);
        assert!(got.contains(&"scripts/setup.sh".to_string()));
        assert!(got.contains(&"NOTES.md".to_string()));
    }

    #[test]
    fn referenced_paths_reject_urls_absolute_and_escapes() {
        let md = "[web](https://example.com/x.md) [abs](/etc/passwd) \
                  [up](../secrets.md) [anchor](#section) `run --all`\n";
        assert!(referenced_relative_paths(md).is_empty());
    }

    #[test]
    fn referenced_paths_strip_anchor_and_leading_dot_slash() {
        let md = "[a](./references/a.md#top)\n";
        assert_eq!(referenced_relative_paths(md), vec!["references/a.md"]);
    }

    #[test]
    fn referenced_paths_skip_fenced_code_blocks() {
        let md = "```\n`vendor/blob.bin`\n```\n`assets/logo.png`\n";
        assert_eq!(referenced_relative_paths(md), vec!["assets/logo.png"]);
    }

    #[test]
    fn referenced_paths_ignore_bare_words_in_code_spans() {
        let md = "Use `install` and `skills` commands.\n";
        assert!(referenced_relative_paths(md).is_empty());
    }

    #[test]
    fn sparse_root_paths_picks_skill_md_conventional_dirs_and_referenced() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("SKILL.md"),
            "---\nname: x\n---\nSee [guide](docs/guide.md) and [gone](docs/missing.md).\n",
        )
        .unwrap();
        fs::create_dir_all(tmp.path().join("references")).unwrap();
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::write(tmp.path().join("docs/guide.md"), "g").unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "fn main(){}").unwrap();

        let got = sparse_root_paths(tmp.path());
        assert!(got.contains(&PathBuf::from("SKILL.md")));
        assert!(got.contains(&PathBuf::from("references")));
        assert!(got.contains(&PathBuf::from("docs/guide.md")));
        assert!(!got.contains(&PathBuf::from("docs/missing.md")));
        assert!(!got.iter().any(|p| p.starts_with("src")));
    }

    #[test]
    fn glob_prefix() {
        assert!(glob_match("gsd-*", "gsd-add-tests"));
        assert!(glob_match("gsd-*", "gsd-"));
        assert!(!glob_match("gsd-*", "foo-bar"));
    }

    #[test]
    fn glob_suffix() {
        assert!(glob_match("*-skill", "my-skill"));
        assert!(!glob_match("*-skill", "skill"));
    }

    #[test]
    fn glob_middle() {
        assert!(glob_match("a-*-b", "a-foo-b"));
        assert!(!glob_match("a-*-b", "x-foo-b"));
    }

    #[test]
    fn glob_exact_no_wildcard() {
        assert!(glob_match("foo", "foo"));
        assert!(!glob_match("foo", "foobar"));
    }
}
