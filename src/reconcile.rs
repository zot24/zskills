//! Compute the reconciliation between three sources of truth:
//! - bytes on disk under ~/.claude/plugins/cache/<marketplace>/<name>/<version>/
//! - inventory in ~/.claude/plugins/installed_plugins.json
//! - enabled flags in ~/.claude/settings.json -> enabledPlugins
//!
//! Plus marketplace tap status (registered vs missing from disk).

use anyhow::Result;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct Report {
    /// installed AND enabled
    pub active: Vec<String>,
    /// installed but disabled (or absent from enabledPlugins)
    pub installed_disabled: Vec<String>,
    /// in enabledPlugins but not in inventory — broken reference
    pub enabled_orphan: Vec<String>,
    /// in inventory but whose marketplace tap no longer exists
    pub installed_orphan: Vec<String>,
}

pub fn run() -> Result<Report> {
    let settings = crate::settings::load(&crate::paths::settings_json()?)?;
    let inventory = crate::inventory::load(&crate::paths::installed_plugins_json()?)?;
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;

    let enabled: BTreeMap<String, bool> = crate::settings::enabled_plugins(&settings)
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_bool().unwrap_or(false)))
                .collect()
        })
        .unwrap_or_default();

    let inv: BTreeMap<String, ()> = crate::inventory::plugins(&inventory)
        .map(|m| m.keys().map(|k| (k.clone(), ())).collect())
        .unwrap_or_default();

    let mut r = Report::default();

    for (k, on) in &enabled {
        if *on {
            if inv.contains_key(k) {
                r.active.push(k.clone());
            } else {
                r.enabled_orphan.push(k.clone());
            }
        }
    }
    for k in inv.keys() {
        let is_enabled = enabled.get(k).copied().unwrap_or(false);
        if !is_enabled {
            r.installed_disabled.push(k.clone());
        }
        // marketplace tap orphan?
        if let Some((_, mp)) = k.rsplit_once('@') {
            if !known.contains_key(mp) {
                r.installed_orphan.push(k.clone());
            }
        }
    }

    r.active.sort();
    r.installed_disabled.sort();
    r.enabled_orphan.sort();
    r.installed_orphan.sort();
    Ok(r)
}

/// Helper: given an inventory map and a qualified key, return its entries array (mut).
#[allow(dead_code)]
pub fn entry_array_mut<'a>(
    inv: &'a mut Map<String, Value>,
    qualified: &str,
) -> Option<&'a mut Vec<Value>> {
    crate::inventory::plugins_mut(inv)
        .get_mut(qualified)
        .and_then(|v| v.as_array_mut())
}
