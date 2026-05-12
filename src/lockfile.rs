//! skills.lock — pinned versions/commits resolved from a Manifest.
//!
//! Minimal stub for v1 — fleshed out when update/install commit-pin lands.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Lockfile {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<LockEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LockEntry {
    pub qualified_name: String,
    pub marketplace_sha: String,
    pub version: String,
}

pub fn load(path: &Path) -> anyhow::Result<Lockfile> {
    if !path.exists() {
        return Ok(Lockfile {
            version: 1,
            entries: vec![],
        });
    }
    let raw = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&raw)?)
}

pub fn save(path: &Path, lock: &Lockfile) -> anyhow::Result<()> {
    let raw = toml::to_string_pretty(lock)?;
    std::fs::write(path, raw)?;
    Ok(())
}
