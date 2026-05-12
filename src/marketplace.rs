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
