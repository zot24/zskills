//! Atomic round-trip of ~/.claude/settings.json.
//!
//! We MUST preserve unknown fields — Claude Code writes hooks, permissions, env, etc.
//! and we only touch `extraKnownMarketplaces` and `enabledPlugins`.

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::io::Write;
use std::path::Path;

/// Load settings.json as a generic Map; returns empty if file missing.
pub fn load(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let v: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as JSON", path.display()))?;
    match v {
        Value::Object(m) => Ok(m),
        _ => anyhow::bail!("{} is not a JSON object", path.display()),
    }
}

/// Write settings.json atomically (tempfile + rename).
pub fn save(path: &Path, map: &Map<String, Value>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("creating tempfile in {}", dir.display()))?;
    let pretty = serde_json::to_string_pretty(map)?;
    tmp.write_all(pretty.as_bytes())?;
    tmp.write_all(b"\n")?;
    tmp.flush()?;
    tmp.persist(path)
        .with_context(|| format!("persisting tempfile to {}", path.display()))?;
    Ok(())
}

/// Get the enabledPlugins map (creates if absent).
pub fn enabled_plugins<'a>(m: &'a Map<String, Value>) -> Option<&'a Map<String, Value>> {
    m.get("enabledPlugins").and_then(|v| v.as_object())
}

pub fn enabled_plugins_mut<'a>(m: &'a mut Map<String, Value>) -> &'a mut Map<String, Value> {
    m.entry("enabledPlugins")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("enabledPlugins must be an object")
}

/// Get the extraKnownMarketplaces map (creates if absent).
pub fn extra_marketplaces_mut<'a>(m: &'a mut Map<String, Value>) -> &'a mut Map<String, Value> {
    m.entry("extraKnownMarketplaces")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("extraKnownMarketplaces must be an object")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_unknown_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let raw = r#"{
            "permissions": { "defaultMode": "auto" },
            "hooks": { "SessionStart": [] },
            "enabledPlugins": { "foo@bar": true }
        }"#;
        std::fs::write(&path, raw).unwrap();
        let mut m = load(&path).unwrap();
        enabled_plugins_mut(&mut m).insert("baz@qux".to_string(), Value::Bool(true));
        save(&path, &m).unwrap();
        let m2 = load(&path).unwrap();
        assert!(m2.contains_key("permissions"));
        assert!(m2.contains_key("hooks"));
        let ep = enabled_plugins(&m2).unwrap();
        assert!(ep.get("foo@bar").is_some());
        assert!(ep.get("baz@qux").is_some());
    }
}
