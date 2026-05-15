//! `search <query>` — keyword search across registered marketplaces.
//!
//! Always-on: substring-matches `query` against `name + description` in each marketplace's
//! cached `marketplace.json`. No network calls.
//!
//! Optional (via the `skills-sh` cargo feature): also federates to the skills.sh remote
//! index when registered and `ZSKILLS_SKILLS_SH_API_KEY` is set. Off by default; the binary
//! has zero skills.sh code unless built with `--features skills-sh`.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct Hit {
    pub kind: HitKind,
    pub name: String,
    pub description: String,
    pub marketplace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installs: Option<u64>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HitKind {
    Plugin,
    /// Only constructed when the `skills-sh` cargo feature is enabled.
    #[allow(dead_code)]
    Skill,
}

pub fn run(query: String, limit: u32, as_json: bool, interactive: bool) -> Result<()> {
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    if known.is_empty() {
        println!(
            "{}",
            "No marketplaces registered. Run `zskills marketplace add-recommended` to seed the defaults."
                .yellow()
        );
        return Ok(());
    }

    let mut hits: Vec<Hit> = Vec::new();
    let query_lc = query.to_lowercase();

    for (mp_name, entry) in &known {
        if crate::commands::marketplace::is_remote_index(entry) {
            #[cfg(feature = "skills-sh")]
            dispatch_remote_index(mp_name, entry, &query, limit, &mut hits);
            #[cfg(not(feature = "skills-sh"))]
            {
                let _ = (mp_name, entry); // silence unused warnings when feature off
            }
            continue;
        }

        match local_search(mp_name, &query_lc, limit as usize) {
            Ok(mut local_hits) => hits.append(&mut local_hits),
            Err(e) => eprintln!(
                "{} {}: {}",
                "✗".red(),
                mp_name,
                format!("local search failed: {}", e).dimmed()
            ),
        }
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&hits)?);
        return Ok(());
    }

    if hits.is_empty() {
        println!("(no matches for {:?})", query);
        return Ok(());
    }

    for h in &hits {
        let tag = match h.kind {
            HitKind::Plugin => "[plugin]".green().to_string(),
            HitKind::Skill => "[skill] ".cyan().to_string(),
        };
        let installs = h
            .installs
            .filter(|n| *n > 0)
            .map(|n| format!(" ({}↓)", n))
            .unwrap_or_default();
        let source = h
            .source_repo
            .as_ref()
            .map(|s| format!(" — {}", s.dimmed()))
            .unwrap_or_default();
        println!(
            "  {} {}{}  {}{}",
            tag,
            h.name.bold(),
            installs.dimmed(),
            short(&h.description, 80).dimmed(),
            source
        );
        println!("           {}", format!("from {}", h.marketplace).dimmed());
    }
    println!("\n{}", format!("{} result(s)", hits.len()).dimmed());

    if interactive {
        install_from_hits(&hits)?;
    }
    Ok(())
}

fn install_from_hits(hits: &[Hit]) -> Result<()> {
    use crate::interactive::Item;

    let items: Vec<Item> = hits
        .iter()
        .map(|h| {
            Item::new(
                format!("{}@{}", h.name, h.marketplace),
                h.description.clone(),
            )
        })
        .collect();
    match crate::interactive::pick_one("Install", &items)? {
        None => println!("Aborted."),
        Some(idx) => {
            let h = &hits[idx];
            let spec = format!("{}@{}", h.name, h.marketplace);
            crate::commands::install::run(vec![spec], false)?;
        }
    }
    Ok(())
}

#[cfg(feature = "skills-sh")]
fn dispatch_remote_index(
    mp_name: &str,
    entry: &Value,
    query: &str,
    limit: u32,
    hits: &mut Vec<Hit>,
) {
    let url = entry
        .get("source")
        .and_then(|s| s.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !url.contains("skills.sh") {
        return;
    }
    if !crate::skills_sh::has_api_key() {
        eprintln!(
            "{} {} skipped: set ZSKILLS_SKILLS_SH_API_KEY to enable federated search.",
            "·".yellow(),
            mp_name.dimmed()
        );
        return;
    }
    match crate::skills_sh::search(query, limit) {
        Ok(results) => {
            for r in results.into_iter().take(limit as usize) {
                hits.push(Hit {
                    kind: HitKind::Skill,
                    name: r.slug,
                    description: r.name,
                    marketplace: mp_name.to_string(),
                    source_repo: Some(r.source),
                    installs: Some(r.installs),
                });
            }
        }
        Err(e) => {
            eprintln!(
                "{} {}: {}",
                "✗".red(),
                mp_name,
                format!("skills.sh search failed: {}", e).dimmed()
            );
        }
    }
}

fn local_search(mp_name: &str, query_lc: &str, limit: usize) -> Result<Vec<Hit>> {
    let manifest_path = crate::paths::marketplace_manifest(mp_name)?;
    if !manifest_path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(&manifest_path)?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    let Some(plugins) = manifest.get("plugins").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for plugin in plugins {
        if out.len() >= limit {
            break;
        }
        let name = plugin
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let desc = plugin
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let haystack = format!("{} {}", name, desc).to_lowercase();
        if haystack.contains(query_lc) {
            out.push(Hit {
                kind: HitKind::Plugin,
                name,
                description: desc,
                marketplace: mp_name.to_string(),
                source_repo: None,
                installs: None,
            });
        }
    }
    Ok(out)
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_keeps_short_strings() {
        assert_eq!(short("hello", 80), "hello");
    }

    #[test]
    fn short_truncates_long_strings() {
        let long = "x".repeat(120);
        let s = short(&long, 80);
        assert_eq!(s.chars().count(), 80);
        assert!(s.ends_with('…'));
    }
}
