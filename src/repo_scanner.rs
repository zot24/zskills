//! Walk a cloned repo tree and report what's installable.
//!
//! Used by `zskills install <owner/repo>` to figure out whether the repo
//! contains Agent Skills (the thing we install today), a plugin marketplace
//! (redirect the user to `zskills marketplace add`), or MCP servers (out of
//! scope for v1 — surface a hint and move on).
//!
//! Read-only; never mutates the cache.

use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct SkillSummary {
    pub name: String,
    /// Pulled from the `description:` field of the SKILL.md YAML frontmatter
    /// when available. Best-effort — malformed or missing frontmatter is fine.
    pub description: Option<String>,
    /// Absolute path to the directory under the cache containing `SKILL.md`.
    #[allow(dead_code)]
    pub source_dir: PathBuf,
}

#[derive(Debug, Default)]
pub struct RepoSurvey {
    pub agent_skills: Vec<SkillSummary>,
    /// `.claude-plugin/marketplace.json` at the repo root.
    pub marketplace: bool,
    /// `.claude-plugin/plugin.json` at the repo root (only meaningful when
    /// `marketplace` is false — marketplaces always have plugins).
    pub plugin: bool,
    /// Total MCP server entries discovered across:
    /// - `<repo>/.mcp.json` (both wrapped and flat schemas)
    /// - `<repo>/.claude-plugin/plugin.json` → `mcpServers`
    pub mcp_count: usize,
}

pub fn survey(cache: &Path) -> Result<RepoSurvey> {
    let mut out = RepoSurvey::default();

    // Agent Skills — reuse the existing skills_in_cache logic and enrich.
    for (name, dir) in crate::agent_skill::skills_in_cache(cache) {
        let description = extract_description(&dir.join("SKILL.md"));
        out.agent_skills.push(SkillSummary {
            name,
            description,
            source_dir: dir,
        });
    }

    // Marketplace / plugin detection.
    let mp_path = cache.join(".claude-plugin").join("marketplace.json");
    out.marketplace = mp_path.exists();
    let plugin_path = cache.join(".claude-plugin").join("plugin.json");
    out.plugin = plugin_path.exists() && !out.marketplace;

    // MCP count: combine root .mcp.json and plugin.json's mcpServers.
    out.mcp_count = count_mcp_entries(&cache.join(".mcp.json"))
        + count_mcp_entries_from_plugin_json(&plugin_path);

    Ok(out)
}

/// Read the SKILL.md YAML frontmatter and extract the `description:` line.
/// We don't pull a full YAML parser in just for this — the format is a
/// `---`-delimited block at the top with `key: value` lines.
fn extract_description(skill_md: &Path) -> Option<String> {
    let content = std::fs::read_to_string(skill_md).ok()?;
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut buf = String::new();
    let mut in_description = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(rest) = line.strip_prefix("description:") {
            buf.push_str(rest.trim());
            in_description = true;
            continue;
        }
        if in_description {
            // YAML continuation: indented lines are part of the value.
            if line.starts_with(' ') || line.starts_with('\t') {
                buf.push(' ');
                buf.push_str(line.trim());
            } else {
                break;
            }
        }
    }
    let trimmed = buf.trim().trim_matches('"').trim_matches('\'').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Parse a `.mcp.json` (accepting both wrapped `{"mcpServers": {...}}` and
/// flat `{"<name>": {...}}` shapes) and return the entry count.
fn count_mcp_entries(path: &Path) -> usize {
    let Ok(bytes) = std::fs::read(path) else {
        return 0;
    };
    let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return 0;
    };
    let map = val
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .or_else(|| val.as_object());
    map.map(|m| m.len()).unwrap_or(0)
}

/// Parse a plugin.json and return the `mcpServers` entry count.
fn count_mcp_entries_from_plugin_json(path: &Path) -> usize {
    let Ok(bytes) = std::fs::read(path) else {
        return 0;
    };
    let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return 0;
    };
    val.get("mcpServers")
        .and_then(|v| v.as_object())
        .map(|m| m.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn skill_md(description: Option<&str>) -> String {
        let desc = description.unwrap_or("");
        format!("---\nname: foo\ndescription: {}\n---\n# Foo\n", desc)
    }

    #[test]
    fn extracts_simple_description() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("SKILL.md");
        fs::write(&path, skill_md(Some("Helps with foo things"))).unwrap();
        assert_eq!(
            extract_description(&path).as_deref(),
            Some("Helps with foo things")
        );
    }

    #[test]
    fn handles_missing_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("SKILL.md");
        fs::write(&path, "# Just a title, no YAML\n").unwrap();
        assert!(extract_description(&path).is_none());
    }

    #[test]
    fn handles_missing_description_key() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("SKILL.md");
        fs::write(&path, "---\nname: x\n---\nbody\n").unwrap();
        assert!(extract_description(&path).is_none());
    }

    #[test]
    fn counts_mcp_entries_wrapped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".mcp.json");
        fs::write(&path, r#"{"mcpServers":{"a":{},"b":{}}}"#).unwrap();
        assert_eq!(count_mcp_entries(&path), 2);
    }

    #[test]
    fn counts_mcp_entries_flat() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".mcp.json");
        fs::write(&path, r#"{"linear":{"type":"http","url":"x"}}"#).unwrap();
        assert_eq!(count_mcp_entries(&path), 1);
    }

    #[test]
    fn survey_discovers_skill_and_mcp() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills").join("zskills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), skill_md(Some("Manages skills"))).unwrap();
        fs::write(
            tmp.path().join(".mcp.json"),
            r#"{"mcpServers":{"linear":{}}}"#,
        )
        .unwrap();
        let s = survey(tmp.path()).unwrap();
        assert_eq!(s.agent_skills.len(), 1);
        assert_eq!(s.agent_skills[0].name, "zskills");
        assert_eq!(
            s.agent_skills[0].description.as_deref(),
            Some("Manages skills")
        );
        assert_eq!(s.mcp_count, 1);
        assert!(!s.marketplace);
    }

    #[test]
    fn survey_discovers_nested_category_skills() {
        // skills/<category>/<name>/SKILL.md — the layout larger collections use.
        let tmp = tempfile::tempdir().unwrap();
        for (cat, name) in [
            ("engineering", "research"),
            ("engineering", "prototype"),
            ("productivity", "grilling"),
        ] {
            let d = tmp.path().join("skills").join(cat).join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("SKILL.md"), skill_md(Some("desc"))).unwrap();
        }
        let s = survey(tmp.path()).unwrap();
        let names: Vec<&str> = s.agent_skills.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["grilling", "prototype", "research"]);
    }

    #[test]
    fn survey_mixes_flat_and_nested_layouts() {
        let tmp = tempfile::tempdir().unwrap();
        let flat = tmp.path().join("skills").join("zskills");
        fs::create_dir_all(&flat).unwrap();
        fs::write(flat.join("SKILL.md"), skill_md(None)).unwrap();
        let nested = tmp.path().join("skills").join("engineering").join("tdd");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("SKILL.md"), skill_md(None)).unwrap();
        let s = survey(tmp.path()).unwrap();
        let names: Vec<&str> = s.agent_skills.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["tdd", "zskills"]);
    }

    #[test]
    fn survey_does_not_descend_into_a_skill_with_subdirs() {
        // A skill that owns helper subdirectories must not be read as a category.
        let tmp = tempfile::tempdir().unwrap();
        let skill = tmp.path().join("skills").join("wayfinder");
        fs::create_dir_all(skill.join("agents")).unwrap();
        fs::write(skill.join("SKILL.md"), skill_md(None)).unwrap();
        fs::write(skill.join("agents").join("SKILL.md"), skill_md(None)).unwrap();
        let s = survey(tmp.path()).unwrap();
        let names: Vec<&str> = s.agent_skills.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["wayfinder"]);
    }

    #[test]
    fn survey_dedupes_same_skill_name_across_categories() {
        let tmp = tempfile::tempdir().unwrap();
        for cat in ["engineering", "deprecated"] {
            let d = tmp.path().join("skills").join(cat).join("grilling");
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("SKILL.md"), skill_md(None)).unwrap();
        }
        let s = survey(tmp.path()).unwrap();
        assert_eq!(s.agent_skills.len(), 1);
        assert_eq!(s.agent_skills[0].name, "grilling");
    }

    #[test]
    fn survey_finds_skills_in_a_marketplace_repo() {
        // Both flags true at once: the redirect must not imply "no Agent Skills".
        let tmp = tempfile::tempdir().unwrap();
        let mp_dir = tmp.path().join(".claude-plugin");
        fs::create_dir_all(&mp_dir).unwrap();
        fs::write(mp_dir.join("marketplace.json"), "{}").unwrap();
        let d = tmp
            .path()
            .join("skills")
            .join("engineering")
            .join("research");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("SKILL.md"), skill_md(None)).unwrap();
        let s = survey(tmp.path()).unwrap();
        assert!(s.marketplace);
        assert_eq!(s.agent_skills.len(), 1);
        assert_eq!(s.agent_skills[0].name, "research");
    }

    #[test]
    fn survey_detects_marketplace() {
        let tmp = tempfile::tempdir().unwrap();
        let mp_dir = tmp.path().join(".claude-plugin");
        fs::create_dir_all(&mp_dir).unwrap();
        fs::write(mp_dir.join("marketplace.json"), "{}").unwrap();
        let s = survey(tmp.path()).unwrap();
        assert!(s.marketplace);
        assert!(!s.plugin); // marketplace shadows the plugin flag
    }
    #[test]
    fn survey_finds_skills_under_dot_agents_skills() {
        // The cross-client layout from the Agent Skills spec, and what
        // `warpdotdev/common-skills` uses. Before this was walked, the survey came back
        // empty and `--skill <name>` reported "not found (available: )".
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp
            .path()
            .join(".agents")
            .join("skills")
            .join("skill-doctor");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("SKILL.md"), skill_md(Some("Grade your skills"))).unwrap();

        let s = survey(tmp.path()).unwrap();
        assert_eq!(s.agent_skills.len(), 1);
        assert_eq!(s.agent_skills[0].name, "skill-doctor");
        assert_eq!(
            s.agent_skills[0].description.as_deref(),
            Some("Grade your skills")
        );
    }

    #[test]
    fn survey_finds_both_roots_and_prefers_dot_agents_on_a_name_clash() {
        let tmp = tempfile::tempdir().unwrap();
        for (root, name) in [
            (".agents/skills", "shared"),
            (".agents/skills", "only-agents"),
            ("skills", "shared"),
            ("skills", "only-skills"),
        ] {
            let d = tmp.path().join(root).join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("SKILL.md"), skill_md(Some(root))).unwrap();
        }

        let s = survey(tmp.path()).unwrap();
        let names: Vec<&str> = s.agent_skills.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["only-agents", "only-skills", "shared"]);
        // One `shared`, and it is the .agents/skills copy.
        let shared = s.agent_skills.iter().find(|k| k.name == "shared").unwrap();
        assert_eq!(shared.description.as_deref(), Some(".agents/skills"));
    }

    #[test]
    fn survey_finds_nested_categories_under_dot_agents_skills() {
        let tmp = tempfile::tempdir().unwrap();
        for (cat, name) in [("web", "alpha"), ("infra", "beta")] {
            let d = tmp
                .path()
                .join(".agents")
                .join("skills")
                .join(cat)
                .join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("SKILL.md"), skill_md(None)).unwrap();
        }
        let s = survey(tmp.path()).unwrap();
        let names: Vec<&str> = s.agent_skills.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn a_root_skill_md_is_ignored_when_dot_agents_skills_has_entries() {
        // The root-SKILL.md fallback is for repos with no skill root at all.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("SKILL.md"), skill_md(None)).unwrap();
        let d = tmp.path().join(".agents").join("skills").join("real");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("SKILL.md"), skill_md(None)).unwrap();

        let s = survey(tmp.path()).unwrap();
        let names: Vec<&str> = s.agent_skills.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["real"]);
    }
}
