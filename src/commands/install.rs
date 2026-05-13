//! `install <name>[@marketplace]...` — adds to enabledPlugins for plugin marketplaces.
//!
//! Plugin path (always on): we don't fetch bytes ourselves — we flip enabledPlugins and
//! let Claude Code's startup install path materialize them on next launch.
//!
//! Skills.sh fallback (cargo feature `skills-sh`): when local resolution fails AND the
//! skills.sh remote index is registered AND `ZSKILLS_SKILLS_SH_API_KEY` is set, we shell
//! through `agent_skill::install` to clone the source repo and drop SKILL.md into
//! ~/.claude/skills/<name>/.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;

pub fn run(specs: Vec<String>) -> Result<()> {
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
