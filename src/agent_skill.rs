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
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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
    /// Scan roots this copy was written to (`agents`, `pi`, `project`).
    /// Empty means today's default: `agents` only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub to: Vec<String>,
}

/// Where an Agent Skill tree is copied from.
#[derive(Debug, Clone)]
pub struct SkillOrigin {
    pub kind: OriginKind,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum OriginKind {
    Git { source: String },
    Marketplace { name: String },
}

impl SkillOrigin {
    pub fn git(source: impl Into<String>, path: Option<String>) -> Self {
        Self {
            kind: OriginKind::Git {
                source: source.into(),
            },
            path,
        }
    }

    pub fn marketplace(name: impl Into<String>, path: Option<String>) -> Self {
        Self {
            kind: OriginKind::Marketplace { name: name.into() },
            path,
        }
    }

    /// Origin for a git or marketplace row. `npm` and local-only rows return `None`.
    pub fn from_entry(entry: &crate::manifest::AgentSkillEntry) -> Option<Self> {
        if entry.npm.is_some() {
            return None;
        }
        if let Some(mp) = &entry.marketplace {
            return Some(Self::marketplace(mp, entry.path.clone()));
        }
        if let Some(src) = &entry.source {
            return Some(Self::git(src, entry.path.clone()));
        }
        None
    }
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
            "unrecognized Agent Skill source: {} (expected owner/repo or git URL)",
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
        crate::git::clone(&url, &cache).context("cloning Agent Skill source repo")?;
    }
    Ok(cache)
}

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

/// Walk one directory the same way `skills_in_cache` walks each `SKILL_ROOTS`
/// entry: a dir with `SKILL.md` is a skill; a dir without is a one-level category.
pub fn skills_in_dir(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    collect_skills_under(root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Discover Agent Skills under an explicit `path` selector.
///
/// If `root/SKILL.md` exists, `root` itself is the skill (name = last segment).
/// `collect_skills_under` would miss that case because it lists children, not the root.
pub fn skills_at_path(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    if root.join("SKILL.md").is_file() {
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid skill directory name"))?;
        validate_skill_name(name)?;
        return Ok(vec![(name.to_string(), root.to_path_buf())]);
    }
    let found = skills_in_dir(root);
    if found.is_empty() {
        anyhow::bail!("no Agent Skills under path {}", root.display());
    }
    Ok(found)
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
/// `allowed_root` for this helper is `src_dir` itself (local migrate copies).
pub fn install_to_user_dir(skill_name: &str, src_dir: &Path) -> Result<()> {
    install_to_root(
        &crate::paths::user_skills_dir()?,
        skill_name,
        src_dir,
        src_dir,
    )
}

/// Copy a skill directory into `<root>/<name>/`. Replaces an existing directory
/// or leftover symlink. Never creates a symlink. Follows in-tree symlinks only
/// when the canonical target stays inside `allowed_root`. On copy error the
/// dest is removed so a half-written hub name is not left behind.
pub fn install_to_root(
    root: &Path,
    skill_name: &str,
    src_dir: &Path,
    allowed_root: &Path,
) -> Result<()> {
    validate_skill_name(skill_name)?;
    let dest = root.join(skill_name);
    if let Ok(meta) = dest.symlink_metadata() {
        if meta.file_type().is_symlink() {
            std::fs::remove_file(&dest)
                .with_context(|| format!("refusing to keep a symlink at {}", dest.display()))?;
        } else if meta.is_dir() {
            std::fs::remove_dir_all(&dest)?;
        } else {
            std::fs::remove_file(&dest)?;
        }
    }
    std::fs::create_dir_all(&dest)?;
    if let Err(e) = copy_dir_recursive(src_dir, &dest, allowed_root) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(e);
    }
    anyhow::ensure!(
        !dest
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        "install produced a symlink at {} — copies must be real directories",
        dest.display()
    );
    Ok(())
}

/// Skill dirs that are copied wholesale for a root-level skill even when
/// SKILL.md doesn't link to them — the conventional layout from the spec.
const CONVENTIONAL_SKILL_DIRS: &[&str] = &["references", "assets", "scripts"];

/// Sparse-install a **root-level** skill (SKILL.md at the repo root of a larger
/// project): materialize only SKILL.md, the conventional skill dirs, and the
/// relative paths SKILL.md references — never the whole source tree.
pub fn install_root_skill_sparse_to(root: &Path, skill_name: &str, cache: &Path) -> Result<()> {
    validate_skill_name(skill_name)?;
    let dest = root.join(skill_name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&dest)?;
    if let Err(e) = copy_sparse_root(cache, &dest) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(e);
    }
    Ok(())
}

fn copy_sparse_root(cache: &Path, dest: &Path) -> Result<()> {
    let allowed = cache
        .canonicalize()
        .with_context(|| format!("canonicalizing allowed_root {}", cache.display()))?;
    let mut visiting = BTreeSet::new();
    for rel in sparse_root_paths(cache) {
        let src = cache.join(&rel);
        let target = dest.join(&rel);
        let meta = src
            .symlink_metadata()
            .with_context(|| format!("reading {}", src.display()))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            copy_symlink(&src, &target, &allowed, &mut visiting)?;
        } else if ft.is_dir() {
            copy_dir_recursive(&src, &target, cache)?;
        } else if ft.is_file() {
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

/// Copy `src` into `dst`. Follow a file or directory symlink only when its
/// canonical target stays inside `allowed_root`. Copy those targets as real
/// files, never as a symlink.
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path, allowed_root: &Path) -> Result<()> {
    let allowed = allowed_root
        .canonicalize()
        .with_context(|| format!("canonicalizing allowed_root {}", allowed_root.display()))?;
    let mut visiting = BTreeSet::new();
    copy_dir_recursive_inner(src, dst, &allowed, &mut visiting)
}

fn copy_dir_recursive_inner(
    src: &Path,
    dst: &Path,
    allowed: &Path,
    visiting: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let src_canon = src.canonicalize().ok();
    if let Some(c) = &src_canon {
        if !visiting.insert(c.clone()) {
            anyhow::bail!("symlink cycle at {}", src.display());
        }
    }
    let result = copy_dir_recursive_walk(src, dst, allowed, visiting);
    if let Some(c) = src_canon {
        visiting.remove(&c);
    }
    result
}

fn copy_dir_recursive_walk(
    src: &Path,
    dst: &Path,
    allowed: &Path,
    visiting: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let walker = walkdir::WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || e.file_name() != ".git");
    for entry in walker {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        let target = dst.join(rel);
        let ft = entry.file_type();
        if ft.is_symlink() {
            copy_symlink(entry.path(), &target, allowed, visiting)?;
        } else if ft.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if ft.is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn copy_symlink(
    src: &Path,
    dst: &Path,
    allowed: &Path,
    visiting: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let canon = src
        .canonicalize()
        .with_context(|| format!("resolving symlink {}", src.display()))?;
    if !is_within(&canon, allowed) {
        anyhow::bail!(
            "symlink {} escapes allowed root {} (target {})",
            src.display(),
            allowed.display(),
            canon.display()
        );
    }
    let meta = std::fs::metadata(&canon)?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        copy_dir_recursive_inner(&canon, dst, allowed, visiting)?;
    } else if meta.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&canon, dst)?;
    }
    Ok(())
}

/// Component-wise prefix on canonical paths. Avoids the `/foo` vs `/foo-evil`
/// string-prefix hole and the macOS `/tmp` vs `/private/tmp` alias.
fn is_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

/// Join `rel` onto `clone` and require the canonical result to stay inside
/// the canonical clone root.
pub fn resolve_path_in_clone(clone: &Path, rel: &str) -> Result<PathBuf> {
    let rel = crate::manifest::normalize_skill_path(rel)?;
    let joined = clone.join(&rel);
    let clone_canon = clone
        .canonicalize()
        .with_context(|| format!("canonicalizing clone {}", clone.display()))?;
    let dest_canon = joined
        .canonicalize()
        .with_context(|| format!("path {rel} not found in clone"))?;
    if !is_within(&dest_canon, &clone_canon) {
        anyhow::bail!("path {rel} is not inside the clone");
    }
    Ok(dest_canon)
}

/// HEAD of an already-cloned Agent Skill cache, if any. Does not clone or pull.
pub fn cached_head(source: &str) -> Option<String> {
    let (_, name) = parse_source(source).ok()?;
    let cache = crate::paths::agent_skills_cache_dir().ok()?.join(name);
    if !cache.exists() {
        return None;
    }
    crate::git::head_sha(&cache).ok()
}

/// Marketplace clone directory. Hard error if the clone is missing — this
/// must not fall through to the local-only apply arm.
pub fn resolve_marketplace_clone(name: &str) -> Result<PathBuf> {
    let dir = crate::paths::marketplaces_dir()?.join(name);
    if !dir.is_dir() {
        anyhow::bail!(
            "marketplace '{name}' is not registered — add repo = on [[marketplaces]] and run sync, or: zskills marketplace add <owner/repo>"
        );
    }
    Ok(dir)
}

/// HEAD of a registered marketplace clone, or `"unknown"` for a tarball.
/// `None` when the clone directory is missing.
pub fn marketplace_head(name: &str) -> Option<String> {
    let dir = crate::paths::marketplaces_dir().ok()?.join(name);
    if !dir.is_dir() {
        return None;
    }
    Some(crate::git::head_sha(&dir).unwrap_or_else(|_| "unknown".to_string()))
}

/// Live clone HEAD for plan/apply skip. `None` if the clone is not on disk.
pub fn live_head(entry: &crate::manifest::AgentSkillEntry) -> Option<String> {
    if let Some(mp) = entry.marketplace.as_deref() {
        return marketplace_head(mp);
    }
    entry.source.as_deref().and_then(cached_head)
}

/// Names an `[[agent_skills]]` row claims on the hub. Explicit `name` wins;
/// unnamed path rows walk the clone. A missing clone claims nothing so the
/// later hard error can fire.
pub fn names_claimed_by(entries: &[crate::manifest::AgentSkillEntry]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in entries {
        if let Some(n) = &entry.name {
            out.insert(n.clone());
            continue;
        }
        let Some(origin) = SkillOrigin::from_entry(entry) else {
            continue;
        };
        if let Some(names) = try_discover_names(&origin) {
            out.extend(names);
        }
    }
    out
}

fn try_discover_names(origin: &SkillOrigin) -> Option<Vec<String>> {
    let clone = existing_clone(origin)?;
    let found = match origin.path.as_deref() {
        Some(rel) => {
            let root = resolve_path_in_clone(&clone, rel).ok()?;
            skills_at_path(&root).ok()?
        }
        None => skills_in_cache(&clone),
    };
    Some(found.into_iter().map(|(n, _)| n).collect())
}

fn existing_clone(origin: &SkillOrigin) -> Option<PathBuf> {
    match &origin.kind {
        OriginKind::Git { source } => {
            let (_, name) = parse_source(source).ok()?;
            let cache = crate::paths::agent_skills_cache_dir().ok()?.join(name);
            cache.is_dir().then_some(cache)
        }
        OriginKind::Marketplace { name } => {
            let dir = crate::paths::marketplaces_dir().ok()?.join(name);
            dir.is_dir().then_some(dir)
        }
    }
}

/// Reject names that would make `user_skills_dir().join(name)` escape that
/// directory. Empty, `.`, `..`, and any path separator are refused before
/// `remove_dir_all`. `sync --prune` calls this through [`remove`].
pub fn validate_skill_name(skill_name: &str) -> Result<()> {
    if skill_name.is_empty() {
        anyhow::bail!("invalid Agent Skill name: empty");
    }
    if skill_name == "." || skill_name == ".." {
        anyhow::bail!("invalid Agent Skill name: {skill_name:?}");
    }
    if skill_name.contains('/')
        || skill_name.contains('\\')
        || skill_name.contains(std::path::MAIN_SEPARATOR)
    {
        anyhow::bail!("invalid Agent Skill name {skill_name:?}: path separator not allowed");
    }
    Ok(())
}

/// Remove an installed agent skill from ~/.agents/skills/<name>/.
pub fn remove_from_user_dir(skill_name: &str) -> Result<()> {
    validate_skill_name(skill_name)?;
    let base = crate::paths::user_skills_dir()?;
    let dest = base.join(skill_name);
    if dest.parent() != Some(base.as_path()) {
        anyhow::bail!(
            "refusing to delete {}: not a direct child of {}",
            dest.display(),
            base.display()
        );
    }
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
                to: vec!["agents".into()],
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

/// Install (or refresh) an Agent Skill from a git source with no `path`.
/// Thin wrapper around [`install_from`].
pub fn install(source: &str, name: Option<&str>) -> Result<Vec<String>> {
    install_from(&SkillOrigin::git(source, None), name)
}

struct ResolvedOrigin {
    clone: PathBuf,
    head_sha: String,
    source_tag: String,
    available: Vec<(String, PathBuf)>,
    label: String,
}

fn resolve_origin(origin: &SkillOrigin) -> Result<ResolvedOrigin> {
    match &origin.kind {
        OriginKind::Git { source } => {
            let cache = ensure_cache(source)?;
            let head_sha = crate::git::head_sha(&cache).unwrap_or_else(|_| "unknown".to_string());
            if let Some(rel) = origin.path.as_deref() {
                let rel = crate::manifest::normalize_skill_path(rel)?;
                let root = resolve_path_in_clone(&cache, &rel)?;
                let found = skills_at_path(&root)?;
                Ok(ResolvedOrigin {
                    clone: cache,
                    head_sha,
                    source_tag: format!("source:{source}:{rel}"),
                    available: found,
                    label: source.clone(),
                })
            } else {
                let found = skills_in_cache(&cache);
                if found.is_empty() {
                    anyhow::bail!(
                        "no skills found in {} (expected skills/<name>/SKILL.md)",
                        source
                    );
                }
                Ok(ResolvedOrigin {
                    clone: cache,
                    head_sha,
                    source_tag: source.clone(),
                    available: found,
                    label: source.clone(),
                })
            }
        }
        OriginKind::Marketplace { name } => {
            let clone = resolve_marketplace_clone(name)?;
            let head_sha = crate::git::head_sha(&clone).unwrap_or_else(|_| "unknown".to_string());
            if let Some(rel) = origin.path.as_deref() {
                let rel = crate::manifest::normalize_skill_path(rel)?;
                let root = resolve_path_in_clone(&clone, &rel)?;
                let found = skills_at_path(&root)?;
                Ok(ResolvedOrigin {
                    clone,
                    head_sha,
                    source_tag: format!("marketplace:{name}:{rel}"),
                    available: found,
                    label: format!("marketplace:{name}"),
                })
            } else {
                let found = skills_in_cache(&clone);
                if found.is_empty() {
                    anyhow::bail!(
                        "no Agent Skills in marketplace '{name}' (expected skills/<name>/SKILL.md)"
                    );
                }
                Ok(ResolvedOrigin {
                    clone,
                    head_sha,
                    source_tag: format!("marketplace:{name}"),
                    available: found,
                    label: format!("marketplace:{name}"),
                })
            }
        }
    }
}

/// Disk ∩ inventory decision before `install_to_root` deletes dest.
#[derive(Clone, Copy)]
enum DestAction {
    Copy,
    Skip,
    Takeover,
}

fn dest_action(
    hub: &Path,
    skill_name: &str,
    origin: &SkillOrigin,
    inv: &Inventory,
    tag: &str,
    head_sha: &str,
) -> Result<DestAction> {
    let dest = hub.join(skill_name);
    if !dest.exists() {
        return Ok(DestAction::Copy);
    }
    match inv.agent_skills.get(skill_name).map(|e| e.source.as_str()) {
        None => anyhow::bail!(
            "Agent Skill '{skill_name}' already exists on the hub with no inventory entry; \
             run `zskills skill remove --force {skill_name}` or rename it"
        ),
        Some(src) if src == tag => {
            if inv
                .agent_skills
                .get(skill_name)
                .is_some_and(|e| e.head_sha == head_sha)
            {
                Ok(DestAction::Skip)
            } else {
                Ok(DestAction::Copy)
            }
        }
        Some(src) if is_same_marketplace_plugin(origin, src) => Ok(DestAction::Takeover),
        Some(src) => {
            anyhow::bail!("refusing to overwrite Agent Skill '{skill_name}' (source {src})")
        }
    }
}

fn is_same_marketplace_plugin(origin: &SkillOrigin, inv_source: &str) -> bool {
    let OriginKind::Marketplace { name } = &origin.kind else {
        return false;
    };
    inv_source
        .strip_prefix("plugin:")
        .and_then(|q| q.rsplit_once('@'))
        .is_some_and(|(_, mp)| mp == name)
}

/// Install (or refresh) an Agent Skill from `origin`. If `name` is given, only
/// that skill is installed; otherwise every skill the origin yields.
pub fn install_from(origin: &SkillOrigin, name: Option<&str>) -> Result<Vec<String>> {
    let resolved = resolve_origin(origin)?;
    let chosen: Vec<_> = match name {
        Some(n) => resolved
            .available
            .into_iter()
            .filter(|(k, _)| k == n)
            .collect::<Vec<_>>(),
        None => resolved.available,
    };
    if chosen.is_empty() {
        if let Some(rel) = origin.path.as_deref() {
            anyhow::bail!(
                "skill '{}' not found under path '{}' in {}",
                name.unwrap_or("?"),
                rel,
                resolved.label
            );
        }
        anyhow::bail!(
            "skill '{}' not found in {} (skills/<name>/ not present)",
            name.unwrap_or("?"),
            resolved.label
        );
    }
    let hub = crate::paths::user_skills_dir()?;
    let mut inv = load_inventory()?;
    let installed_at = chrono_now_iso();
    // Decide every dest before any copy. An unnamed row that yields
    // wiki-manager then wiki-query must not copy wiki-manager if wiki-query
    // will refuse: dest_action? after a copy would leave that dest untagged.
    let mut planned: Vec<(&String, &PathBuf, DestAction)> = Vec::new();
    for (skill_name, src_dir) in &chosen {
        let action = dest_action(
            &hub,
            skill_name,
            origin,
            &inv,
            &resolved.source_tag,
            &resolved.head_sha,
        )?;
        planned.push((skill_name, src_dir, action));
    }
    let mut installed_names = Vec::new();
    for (skill_name, src_dir, action) in planned {
        match action {
            DestAction::Skip => {
                installed_names.push(skill_name.clone());
                continue;
            }
            DestAction::Takeover => {
                println!(
                    "  {} {}: hub taken over by [[agent_skills]] path",
                    "·".dimmed(),
                    skill_name
                );
            }
            DestAction::Copy => {}
        }
        if src_dir == &resolved.clone {
            // Root-level SKILL.md in a larger project — materialize sparsely
            // instead of copying the whole source tree.
            install_root_skill_sparse_to(&hub, skill_name, &resolved.clone)?;
        } else {
            install_to_root(&hub, skill_name, src_dir, &resolved.clone)?;
        }
        inv.agent_skills.insert(
            skill_name.clone(),
            Entry {
                source: resolved.source_tag.clone(),
                installed_at: installed_at.clone(),
                head_sha: resolved.head_sha.clone(),
                to: vec!["agents".into()],
            },
        );
        installed_names.push(skill_name.clone());
    }
    save_inventory(&inv)?;
    Ok(installed_names)
}

pub fn remove(skill_name: &str) -> Result<bool> {
    validate_skill_name(skill_name)?;
    let mut inv = load_inventory()?;
    let in_inventory = inv.agent_skills.contains_key(skill_name);
    let dest = crate::paths::user_skills_dir()?.join(skill_name);
    let on_disk = dest.is_dir();
    if !in_inventory && !on_disk {
        return Ok(false);
    }
    inv.agent_skills.remove(skill_name);
    remove_from_user_dir(skill_name)?;
    save_inventory(&inv)?;
    Ok(true)
}

pub(crate) fn inventory_now() -> String {
    chrono_now_iso()
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

    fn with_agents_home<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
        let _guard = crate::paths::HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let skills = home.join("skills");
        fs::create_dir_all(skills.join("alpha")).unwrap();
        fs::write(skills.join("alpha").join("SKILL.md"), "a").unwrap();
        fs::create_dir_all(skills.join("beta")).unwrap();
        fs::write(skills.join("beta").join("SKILL.md"), "b").unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(home.join(".claude").join("keep"), "x").unwrap();
        // SAFETY: held under HOME_ENV_LOCK; restored before the guard drops.
        let prev = std::env::var_os("AGENTS_HOME");
        std::env::set_var("AGENTS_HOME", home);
        let out = f(home);
        match prev {
            Some(v) => std::env::set_var("AGENTS_HOME", v),
            None => std::env::remove_var("AGENTS_HOME"),
        }
        out
    }

    fn surviving_skills(home: &std::path::Path) -> Vec<String> {
        let skills = home.join("skills");
        if !skills.is_dir() {
            return Vec::new();
        }
        let mut names: Vec<String> = fs::read_dir(&skills)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir() && e.path().join("SKILL.md").exists())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn remove_from_user_dir_rejects_empty_dot_dotdot_and_traversal() {
        with_agents_home(|home| {
            for bad in ["", ".", "..", "../.claude"] {
                let err = super::remove_from_user_dir(bad).unwrap_err().to_string();
                assert!(
                    err.contains("invalid Agent Skill name") || err.contains("not a direct child"),
                    "unexpected error for {bad:?}: {err}"
                );
                assert_eq!(surviving_skills(home), ["alpha", "beta"]);
                assert!(
                    home.join(".claude").join("keep").exists(),
                    ".claude neighbour deleted for {bad:?}"
                );
            }
        });
    }

    #[test]
    fn remove_resolves_presence_before_deleting_and_leaves_neighbours() {
        with_agents_home(|home| {
            for bad in ["", ".", "..", "../.claude"] {
                assert!(
                    super::remove(bad).is_err(),
                    "remove({bad:?}) must error before any delete"
                );
            }
            assert!(!super::remove("missing-skill").unwrap());
            assert_eq!(surviving_skills(home), ["alpha", "beta"]);
            assert!(home.join(".claude").join("keep").exists());

            assert!(super::remove("alpha").unwrap());
            assert_eq!(surviving_skills(home), ["beta"]);
            assert!(!super::remove("alpha").unwrap());
        });
    }

    #[test]
    fn skills_at_path_treats_skill_dir_as_one_and_parent_as_children() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("packages/foo/skills");
        fs::create_dir_all(parent.join("wiki-manager")).unwrap();
        fs::write(parent.join("wiki-manager").join("SKILL.md"), "wm").unwrap();
        fs::create_dir_all(parent.join("wiki-query")).unwrap();
        fs::write(parent.join("wiki-query").join("SKILL.md"), "wq").unwrap();

        let parent_found = super::skills_at_path(&parent).unwrap();
        let names: Vec<_> = parent_found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["wiki-manager", "wiki-query"]);

        let one = super::skills_at_path(&parent.join("wiki-manager")).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].0, "wiki-manager");
    }

    #[test]
    fn copy_follows_in_clone_symlink_as_regular_file() {
        // Previously copy_dir_recursive dropped symlinks (is_file/is_dir false
        // when follow_links is false). Dest references/hub-resolution.md must
        // now be a regular file, not a symlink and not missing.
        let tmp = tempfile::tempdir().unwrap();
        let clone = tmp.path().join("clone");
        let refs = clone.join("claude-plugin/skills/wiki-manager/references");
        fs::create_dir_all(&refs).unwrap();
        fs::write(refs.join("hub-resolution.md"), "notes\n").unwrap();
        let skill = clone.join("packages/foo/skills/wiki-manager");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "openc\n").unwrap();
        std::os::unix::fs::symlink(
            "../../../../claude-plugin/skills/wiki-manager/references",
            skill.join("references"),
        )
        .unwrap();

        let dest = tmp.path().join("hub/wiki-manager");
        super::copy_dir_recursive(&skill, &dest, &clone).unwrap();
        let copied = dest.join("references/hub-resolution.md");
        assert!(copied.is_file(), "dereferenced contents must be present");
        assert!(
            !copied.symlink_metadata().unwrap().file_type().is_symlink(),
            "hub copy must be a regular file, not a symlink"
        );
        assert_eq!(fs::read_to_string(&copied).unwrap(), "notes\n");
    }

    #[test]
    fn copy_refuses_symlink_escape_and_leaves_no_partial_dest() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = tmp.path().join("clone");
        let skill = clone.join("skills/evil");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "x\n").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", skill.join("secret")).unwrap();

        let hub = tmp.path().join("hub");
        let err = super::install_to_root(&hub, "evil", &skill, &clone)
            .unwrap_err()
            .to_string();
        assert!(err.contains("escapes allowed root"), "{err}");
        assert!(
            !hub.join("evil").exists(),
            "failed copy must not leave a partial dest"
        );
    }

    #[test]
    fn sparse_copy_refuses_file_symlink_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", cache.join("SKILL.md")).unwrap();
        let hub = tmp.path().join("hub");
        let err = super::install_root_skill_sparse_to(&hub, "evil", &cache)
            .unwrap_err()
            .to_string();
        assert!(err.contains("escapes allowed root"), "{err}");
        assert!(
            !hub.join("evil").exists(),
            "failed sparse copy must not leave a partial dest"
        );
    }

    #[test]
    fn copy_refuses_in_clone_symlink_cycle() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = tmp.path().join("clone");
        let skill = clone.join("skills/loop");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "x\n").unwrap();
        std::os::unix::fs::symlink("..", skill.join("parent")).unwrap();
        let hub = tmp.path().join("hub");
        let err = super::install_to_root(&hub, "loop", &skill, &clone)
            .unwrap_err()
            .to_string();
        assert!(err.contains("cycle"), "{err}");
        assert!(
            !hub.join("loop").exists(),
            "cycle must not leave a partial dest"
        );
    }
}
