//! `search <query>` — keyword search across registered marketplaces.
//!
//! Reads each marketplace's cached `marketplace.json` and substring-matches `query`
//! against `name + description`. No network calls; purely local.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct Hit {
    pub name: String,
    pub description: String,
    pub marketplace: String,
}

pub fn run(query: String, limit: u32, as_json: bool) -> Result<()> {
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

    for mp_name in known.keys() {
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
        println!(
            "  {} {}  {}",
            "[plugin]".green(),
            h.name.bold(),
            short(&h.description, 80).dimmed(),
        );
        println!("           {}", format!("from {}", h.marketplace).dimmed());
    }
    println!("\n{}", format!("{} result(s)", hits.len()).dimmed());
    Ok(())
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
                name,
                description: desc,
                marketplace: mp_name.to_string(),
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
