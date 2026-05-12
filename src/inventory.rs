//! ~/.claude/plugins/installed_plugins.json — Claude Code's inventory.
//!
//! Schema (observed):
//! {
//!   "version": 2,
//!   "plugins": {
//!     "<name>@<marketplace>": [
//!       { "scope": "user"|"local", "installPath": "...", "version": "...", ...},
//!       ...
//!     ]
//!   }
//! }

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::path::Path;

pub fn load(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        let mut m = Map::new();
        m.insert("version".to_string(), Value::from(2));
        m.insert("plugins".to_string(), Value::Object(Map::new()));
        return Ok(m);
    }
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let v: Value = serde_json::from_slice(&bytes)?;
    match v {
        Value::Object(m) => Ok(m),
        _ => anyhow::bail!("{} is not a JSON object", path.display()),
    }
}

pub fn save(path: &Path, map: &Map<String, Value>) -> Result<()> {
    crate::settings::save(path, map)
}

pub fn plugins<'a>(m: &'a Map<String, Value>) -> Option<&'a Map<String, Value>> {
    m.get("plugins").and_then(|v| v.as_object())
}

pub fn plugins_mut<'a>(m: &'a mut Map<String, Value>) -> &'a mut Map<String, Value> {
    m.entry("plugins")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("plugins must be an object")
}

/// Iterate over (qualified_name, scope) tuples across installations.
pub fn installed_entries<'a>(m: &'a Map<String, Value>) -> Vec<(&'a str, &'a str)> {
    let mut out = Vec::new();
    if let Some(p) = plugins(m) {
        for (k, v) in p.iter() {
            if let Some(arr) = v.as_array() {
                for entry in arr {
                    let scope = entry.get("scope").and_then(|s| s.as_str()).unwrap_or("?");
                    out.push((k.as_str(), scope));
                }
            }
        }
    }
    out
}
