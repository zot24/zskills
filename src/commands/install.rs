//! `install <spec>...` — three accepted spec shapes:
//!
//! 1. `<name>` or `<name>@<marketplace>` — plugin path. Resolves against registered
//!    marketplace caches, flips `enabledPlugins` in settings.json. Claude Code fetches
//!    bytes on next launch.
//! 2. `<owner>/<repo>` or `git@…` / `https://…/<repo>.git` URL — **repo path** (v0.8+).
//!    Clones the repo, surveys it via `repo_scanner`, and installs Agent Skills found
//!    under `skills/<name>/SKILL.md`. Marketplaces are detected and redirected; MCPs
//!    are mentioned but not auto-installed in this mode.
//! 3. `skills.sh` remote index (cargo feature `skills-sh`): when local resolution fails
//!    AND the index is registered AND `ZSKILLS_SKILLS_SH_API_KEY` is set, falls through
//!    to clone the source repo and drop SKILL.md into `~/.agents/skills/<name>/`.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;

pub fn run(specs: Vec<String>, interactive: bool, all: bool) -> Result<()> {
    if interactive && specs.is_empty() {
        return run_interactive_browse_marketplaces();
    }

    if specs.is_empty() {
        anyhow::bail!("specify at least one skill name, or use -i/--interactive to browse");
    }

    // Partition specs by shape. Repo specs (owner/repo, git URLs) don't need a
    // registered marketplace — they clone directly. Marketplace specs do.
    let (repo_specs, plugin_specs): (Vec<String>, Vec<String>) =
        specs.into_iter().partition(|s| is_repo_spec(s));

    for spec in &repo_specs {
        if let Err(e) = install_from_repo(spec, interactive, all) {
            eprintln!("{} {}: {}", "✗".red(), spec, e);
        }
    }

    if !plugin_specs.is_empty() {
        install_plugin_specs(plugin_specs)?;
    }

    Ok(())
}

/// True for specs that should clone a git repo directly: `owner/repo` or full
/// git URLs. Excludes:
/// - `name` (no slash) — unqualified plugin name, marketplace path.
/// - `name@marketplace` — qualified plugin name, marketplace path.
/// - `./local-path` and `/abs-path` — local paths (not supported as install sources).
pub(crate) fn is_repo_spec(spec: &str) -> bool {
    if spec.contains('@') && !spec.starts_with("git@") {
        return false;
    }
    if spec.starts_with('.') || spec.starts_with('/') {
        return false;
    }
    spec.contains("://") || spec.starts_with("git@") || spec.contains('/')
}

fn install_from_repo(spec: &str, interactive: bool, all: bool) -> Result<()> {
    println!("{} {}", "Surveying".dimmed(), spec.to_string().bold());
    let cache = crate::agent_skill::ensure_cache(spec)?;
    let survey = crate::repo_scanner::survey(&cache)?;

    if survey.marketplace {
        println!(
            "{}",
            "This repo is a plugin marketplace. To register and install plugins from it:".yellow()
        );
        println!("  zskills marketplace add {}", spec.bold());
        println!(
            "  zskills install <plugin>@<marketplace>   {}",
            "(or `zskills install -i` to browse)".dimmed()
        );
        return Ok(());
    }

    if survey.plugin {
        eprintln!(
            "{} {}",
            "!".yellow(),
            "this repo declares a single plugin (no marketplace.json); use `zskills marketplace add` to register it".dimmed()
        );
    }
    if survey.mcp_count > 0 {
        eprintln!(
            "{} this repo also declares {} MCP server(s); add `[[mcps]]` entries to your skills.toml to manage them",
            "·".dimmed(),
            survey.mcp_count
        );
    }

    const AUTO_INSTALL_MAX: usize = 5;
    let n = survey.agent_skills.len();
    let chosen: Vec<String> = match (n, interactive, all) {
        (0, _, _) => anyhow::bail!(
            "no Agent Skills found in {} (expected skills/<name>/SKILL.md)",
            spec
        ),
        (1, _, _) => vec![survey.agent_skills[0].name.clone()],
        (_, true, _) => pick_skills(spec, &survey.agent_skills)?,
        (_, false, true) => survey.agent_skills.iter().map(|s| s.name.clone()).collect(),
        (count, false, false) if count <= AUTO_INSTALL_MAX => {
            survey.agent_skills.iter().map(|s| s.name.clone()).collect()
        }
        (count, false, false) => {
            print_large_collection_summary(spec, count, &survey.agent_skills);
            return Ok(());
        }
    };

    if chosen.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    for name in &chosen {
        match crate::agent_skill::install(spec, Some(name)) {
            Ok(installed) => {
                for n in installed {
                    println!(
                        "{} {} {}",
                        "+".green(),
                        n,
                        format!("[from {}]", spec).dimmed()
                    );
                }
            }
            Err(e) => eprintln!("{} {}: {}", "✗".red(), name, e),
        }
    }
    Ok(())
}

fn pick_skills(spec: &str, skills: &[crate::repo_scanner::SkillSummary]) -> Result<Vec<String>> {
    let items: Vec<crate::interactive::Item> = skills
        .iter()
        .map(|s| {
            crate::interactive::Item::new(s.name.clone(), s.description.clone().unwrap_or_default())
        })
        .collect();
    let idxs =
        crate::interactive::pick_many(&format!("Skills in {} (space to select)", spec), &items)?;
    Ok(idxs.iter().map(|&i| skills[i].name.clone()).collect())
}

fn print_large_collection_summary(
    spec: &str,
    count: usize,
    skills: &[crate::repo_scanner::SkillSummary],
) {
    println!(
        "{} contains {} {} — zskills won't install all of them by default.",
        spec.bold(),
        count.to_string().bold(),
        "Agent Skills".dimmed()
    );
    println!("\n{}", "Options:".bold());
    println!(
        "  {}   {}",
        format!("zskills install {} -i", spec).bold(),
        "interactive picker".dimmed()
    );
    println!(
        "  {}   {}",
        format!("zskills install {} --all", spec).bold(),
        format!("install all {} skills", count).dimmed()
    );
    let preview: Vec<&str> = skills.iter().take(5).map(|s| s.name.as_str()).collect();
    println!(
        "\n{} {}{}",
        format!("Sample (5 of {}):", count).dimmed(),
        preview.join(", ").dimmed(),
        if count > 5 {
            ", …".dimmed().to_string()
        } else {
            String::new()
        }
    );
}

fn install_plugin_specs(specs: Vec<String>) -> Result<()> {
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    if known.is_empty() {
        println!(
            "{}",
            "No marketplaces registered. Run `zskills marketplace add-recommended` first.".yellow()
        );
        return Ok(());
    }

    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    let mut settings_dirty = false;
    let mut count = 0;

    for spec in &specs {
        match crate::marketplace::resolve_spec(spec, &known) {
            Ok(qualified) => {
                let ep = crate::settings::enabled_plugins_mut(&mut settings);
                ep.insert(qualified.clone(), Value::Bool(true));
                settings_dirty = true;
                println!("{} {}", "+".green(), qualified);
                count += 1;
            }
            Err(plugin_err) => match try_install_from_remote(spec, &known) {
                Ok(true) => count += 1,
                Ok(false) => {
                    eprintln!("{} {}: {}", "✗".red(), spec, plugin_err);
                }
                Err(remote_err) => {
                    eprintln!(
                        "{} {}: {} (remote index also failed: {})",
                        "✗".red(),
                        spec,
                        plugin_err,
                        remote_err
                    );
                }
            },
        }
    }

    if settings_dirty {
        crate::settings::save(&settings_path, &settings)?;
        println!(
            "\nWrote {} plugin entry/entries to {}.\nRestart Claude Code (or run `/plugin marketplace update` and `/plugin install ...`) to fetch the bytes.",
            count,
            settings_path.display()
        );
    } else if count > 0 {
        println!(
            "\nInstalled {} agent skill(s) into {}.",
            count,
            crate::paths::user_skills_dir()?.display()
        );
    }
    Ok(())
}

fn run_interactive_browse_marketplaces() -> Result<()> {
    use crate::interactive::Item;

    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    if known.is_empty() {
        println!(
            "{}",
            "No marketplaces registered. Run `zskills marketplace add-recommended` first.".yellow()
        );
        return Ok(());
    }

    let mut qualified_names: Vec<String> = Vec::new();
    let mut items: Vec<Item> = Vec::new();

    for (mp_name, entry) in &known {
        if crate::commands::marketplace::is_remote_index(entry) {
            continue;
        }
        let manifest_path = match crate::paths::marketplace_manifest(mp_name) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if let Ok(manifest) = crate::marketplace::load_manifest(&manifest_path) {
            for plugin in manifest.plugins {
                let qualified = format!("{}@{}", plugin.name, mp_name);
                let desc = plugin.description.unwrap_or_default();
                items.push(Item::new(qualified.clone(), desc));
                qualified_names.push(qualified);
            }
        }
    }

    if qualified_names.is_empty() {
        println!(
            "{}",
            "No plugins found. Run `zskills marketplace update` to refresh caches.".yellow()
        );
        return Ok(());
    }

    match crate::interactive::pick_one("Install plugin", &items)? {
        None => println!("Aborted."),
        Some(idx) => run(vec![qualified_names[idx].clone()], false, false)?,
    }
    Ok(())
}

/// Returns Ok(true) if the spec was installed via a remote index, Ok(false) if no remote
/// driver matched (and the caller should surface the plugin error), or Err if a driver
/// was selected but failed.
#[cfg(feature = "skills-sh")]
fn try_install_from_remote(spec: &str, known: &serde_json::Map<String, Value>) -> Result<bool> {
    let has_skills_sh = known.values().any(|entry| {
        crate::commands::marketplace::is_remote_index(entry)
            && entry
                .get("source")
                .and_then(|s| s.get("url"))
                .and_then(|v| v.as_str())
                .is_some_and(|u| u.contains("skills.sh"))
    });
    if !has_skills_sh || !crate::skills_sh::has_api_key() {
        return Ok(false);
    }
    let slug = spec.split_once('@').map(|(n, _)| n).unwrap_or(spec);
    let results = crate::skills_sh::search(slug, 25)?;
    let Some(hit) = results.iter().find(|r| r.slug == slug) else {
        return Ok(false);
    };
    let installed = crate::agent_skill::install(&hit.source, Some(&hit.slug))?;
    for name in &installed {
        println!(
            "{} {} {}",
            "+".green(),
            name,
            format!("[skill via skills.sh — {}]", hit.source).dimmed()
        );
    }
    Ok(!installed.is_empty())
}

#[cfg(not(feature = "skills-sh"))]
fn try_install_from_remote(_spec: &str, _known: &serde_json::Map<String, Value>) -> Result<bool> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_repo_spec_detects_owner_repo() {
        assert!(is_repo_spec("zot24/zskills"));
        assert!(is_repo_spec("anthropics/claude-plugins-official"));
    }

    #[test]
    fn is_repo_spec_detects_git_url() {
        assert!(is_repo_spec("https://github.com/foo/bar.git"));
        assert!(is_repo_spec("git@github.com:foo/bar.git"));
        assert!(is_repo_spec("file:///tmp/foo"));
    }

    #[test]
    fn is_repo_spec_rejects_unqualified_name() {
        assert!(!is_repo_spec("firecrawl"));
        assert!(!is_repo_spec("get-shit-done-cc"));
    }

    #[test]
    fn is_repo_spec_rejects_qualified_name() {
        assert!(!is_repo_spec("firecrawl@zot24-skills"));
    }

    #[test]
    fn is_repo_spec_rejects_local_paths() {
        assert!(!is_repo_spec("./local-repo"));
        assert!(!is_repo_spec("/abs/local/repo"));
    }
}
