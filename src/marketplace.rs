//! Marketplace (tap) parsing.
//!
//! known_marketplaces.json schema (observed):
//! {
//!   "<name>": {
//!     "source": { "source": "github", "repo": "owner/repo" },
//!     "installLocation": "/Users/.../marketplaces/<name>",
//!     "lastUpdated": "...",
//!     "autoUpdate": true
//!   }
//! }

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MarketplaceManifest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PluginEntry {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// `source` can be a string OR an object (`{ source, url, ref, sha, ... }`) depending on marketplace.
    #[serde(default)]
    pub source: Option<serde_json::Value>,
}

pub fn load_known(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let bytes = std::fs::read(path)?;
    let v: Value = serde_json::from_slice(&bytes)?;
    match v {
        Value::Object(m) => Ok(m),
        _ => anyhow::bail!("{} is not a JSON object", path.display()),
    }
}

pub fn save_known(path: &Path, map: &Map<String, Value>) -> Result<()> {
    crate::settings::save(path, map)
}

/// Resolve a marketplace's source into a GitHub `owner/repo`, if its `known_marketplaces.json`
/// entry encodes one. Used to update non-git marketplaces via tarball.
pub fn github_owner_repo(known: &Map<String, Value>, name: &str) -> Option<String> {
    let entry = known.get(name)?;
    let src = entry.get("source")?;
    // Two shapes observed:
    //   { "source": "github", "repo": "owner/repo" }
    //   { "source": "git", "url": "https://github.com/owner/repo.git" }
    if let Some("github") = src.get("source").and_then(|v| v.as_str()) {
        return src.get("repo").and_then(|v| v.as_str()).map(str::to_string);
    }
    if let Some("git") = src.get("source").and_then(|v| v.as_str()) {
        let url = src.get("url").and_then(|v| v.as_str())?;
        return github_from_url(url);
    }
    None
}

fn github_from_url(url: &str) -> Option<String> {
    let stripped = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("git@github.com:"))?;
    let stripped = stripped.trim_end_matches(".git");
    Some(stripped.to_string())
}

/// Fetch the marketplace's GitHub archive tarball and atomically replace `dest`.
/// Tries `HEAD.tar.gz` (default branch). Uses the system temp dir for extraction
/// then renames into place.
pub fn update_via_tarball(name: &str, dest: &Path) -> Result<()> {
    let known = load_known(&crate::paths::known_marketplaces_json()?)?;
    let owner_repo = github_owner_repo(&known, name)
        .with_context(|| format!("no GitHub source recorded for marketplace '{}'", name))?;

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("zskills/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    // Try HEAD (default branch), then fall back to main and master explicitly.
    let candidates = [
        format!("https://github.com/{}/archive/HEAD.tar.gz", owner_repo),
        format!(
            "https://github.com/{}/archive/refs/heads/main.tar.gz",
            owner_repo
        ),
        format!(
            "https://github.com/{}/archive/refs/heads/master.tar.gz",
            owner_repo
        ),
    ];
    let mut bytes: Option<bytes::Bytes> = None;
    let mut last_err: Option<anyhow::Error> = None;
    for url in &candidates {
        match client.get(url).send().and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes() {
                Ok(b) => {
                    bytes = Some(b);
                    break;
                }
                Err(e) => last_err = Some(anyhow::Error::from(e)),
            },
            Err(e) => last_err = Some(anyhow::Error::from(e)),
        }
    }
    let bytes = bytes.ok_or_else(|| {
        anyhow::anyhow!(
            "could not fetch any tarball variant for {} ({})",
            owner_repo,
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )
    })?;

    // Extract into a sibling temp dir so the final rename stays on the same filesystem.
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("dest has no parent"))?;
    std::fs::create_dir_all(parent).ok();
    let staging = tempfile::tempdir_in(parent)
        .with_context(|| format!("creating staging dir under {}", parent.display()))?;

    let decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(staging.path())
        .with_context(|| "extracting tarball")?;

    // GitHub archives unpack as a single top-level dir like `<repo>-<sha>/`.
    let mut entries = std::fs::read_dir(staging.path())?;
    let only = entries
        .next()
        .ok_or_else(|| anyhow::anyhow!("tarball had no entries"))??;
    anyhow::ensure!(entries.next().is_none(), "tarball had unexpected layout");
    let extracted = only.path();

    // Atomic-ish swap: keep a backup we restore on failure.
    let backup = parent.join(format!(".{}-zskills-backup", name));
    if backup.exists() {
        std::fs::remove_dir_all(&backup).ok();
    }
    if dest.exists() {
        std::fs::rename(dest, &backup)
            .with_context(|| format!("moving existing {} aside", dest.display()))?;
    }
    if let Err(e) = std::fs::rename(&extracted, dest) {
        // Roll back.
        if backup.exists() {
            std::fs::rename(&backup, dest).ok();
        }
        return Err(e).context(format!(
            "moving extracted tree into place at {}",
            dest.display()
        ));
    }
    if backup.exists() {
        std::fs::remove_dir_all(&backup).ok();
    }
    Ok(())
}

pub fn load_manifest(path: &Path) -> Result<MarketplaceManifest> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading marketplace manifest {}", path.display()))?;
    let m: MarketplaceManifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as marketplace manifest", path.display()))?;
    Ok(m)
}

/// Resolve a possibly-unqualified spec ("foo" or "foo@bar") against known marketplaces.
/// Returns the qualified form "name@marketplace".
pub fn resolve_spec(spec: &str, known: &Map<String, Value>) -> Result<String> {
    if let Some((name, mp)) = spec.split_once('@') {
        return Ok(format!("{}@{}", name, mp));
    }
    let mut matches: Vec<String> = Vec::new();
    for mp_name in known.keys() {
        if let Ok(manifest) = load_manifest(&crate::paths::marketplace_manifest(mp_name)?) {
            if manifest.plugins.iter().any(|p| p.name == spec) {
                matches.push(format!("{}@{}", spec, mp_name));
            }
        }
    }
    match matches.len() {
        0 => anyhow::bail!("skill '{}' not found in any registered marketplace", spec),
        1 => Ok(matches.remove(0)),
        _ => Err(crate::error::Error::AmbiguousSkill(spec.to_string(), matches.join(", ")).into()),
    }
}
