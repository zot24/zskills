//! Aggregate MCP servers across Claude Code's known sources.
//!
//! Read-only in M1: parsing + display only. No writes.
//!
//! Data model is tool-agnostic on purpose — the `McpServer` struct has no
//! Claude-specific fields. When grok-cli / codex adapters arrive later,
//! only the *loaders* (which files to look in, where they live on disk)
//! change; this struct stays put.

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Managed,
    User,
    Project,
    Local,
}

impl Scope {
    pub fn label(&self) -> &'static str {
        match self {
            Scope::Managed => "managed",
            Scope::User => "user",
            Scope::Project => "project",
            Scope::Local => "local",
        }
    }
    /// Display order: most-authoritative first.
    pub fn precedence(&self) -> u8 {
        match self {
            Scope::Managed => 0,
            Scope::Local => 1,
            Scope::Project => 2,
            Scope::User => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Transport {
    Stdio {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        /// Env var *keys* only — values are never captured (often contain `${VAR}` refs
        /// or, in rare bad-practice cases, inline secrets).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env_keys: Vec<String>,
    },
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        header_keys: Vec<String>,
    },
    Sse {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        header_keys: Vec<String>,
    },
}

impl Transport {
    pub fn kind(&self) -> &'static str {
        match self {
            Transport::Stdio { .. } => "stdio",
            Transport::Http { .. } => "http",
            Transport::Sse { .. } => "sse",
        }
    }
    pub fn short(&self) -> String {
        match self {
            Transport::Stdio { command, args, .. } => {
                if args.is_empty() {
                    command.clone()
                } else {
                    format!("{} {}", command, args.join(" "))
                }
            }
            Transport::Http { url, .. } | Transport::Sse { url, .. } => url.clone(),
        }
    }
    /// Number of env vars (stdio) or headers (http/sse). Used for the `(N secret)` hint.
    pub fn sensitive_count(&self) -> usize {
        match self {
            Transport::Stdio { env_keys, .. } => env_keys.len(),
            Transport::Http { header_keys, .. } | Transport::Sse { header_keys, .. } => {
                header_keys.len()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Manual,
    FromPlugin { plugin: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServer {
    pub name: String,
    pub scope: Scope,
    pub transport: Transport,
    pub source: Source,
    pub source_file: PathBuf,
}

/// Load every MCP server visible to Claude Code.
///
/// Sources, by precedence (highest first):
/// 1. **Managed** — org/IT-deployed settings (read-only).
/// 2. **Local** — `<cwd>/.claude.local/settings.json` (gitignored, personal+creds).
/// 3. **Project** — `<cwd>/.mcp.json` (recommended) and `<cwd>/.claude/settings.json` (legacy).
/// 4. **User** — `~/.claude/settings.json`.
///
/// Entries are attributed to a plugin when their name matches one declared in
/// an enabled plugin's `plugin.json` or sibling `.mcp.json`.
pub fn load_all() -> Result<Vec<McpServer>> {
    let plugin_index = build_plugin_mcp_index().unwrap_or_default();
    let attribute = |name: &str| -> Source {
        plugin_index
            .get(name)
            .cloned()
            .map(|plugin| Source::FromPlugin { plugin })
            .unwrap_or(Source::Manual)
    };

    let mut out: Vec<McpServer> = Vec::new();

    if let Some(path) = managed_settings_path() {
        out.extend(load_settings_file(&path, Scope::Managed, &attribute));
    }

    if let Ok(cwd) = std::env::current_dir() {
        out.extend(load_settings_file(
            &cwd.join(".claude.local").join("settings.json"),
            Scope::Local,
            &attribute,
        ));
        out.extend(load_mcp_json(
            &cwd.join(".mcp.json"),
            Scope::Project,
            &attribute,
        ));
        out.extend(load_settings_file(
            &cwd.join(".claude").join("settings.json"),
            Scope::Project,
            &attribute,
        ));
    }

    // User scope lives in TWO files in the wild:
    //   ~/.claude.json          — where `claude mcp add --scope user` writes (primary)
    //   ~/.claude/settings.json — also valid per the spec; some users have entries here
    if let Ok(p) = crate::paths::claude_json() {
        out.extend(load_settings_file(&p, Scope::User, &attribute));
    }
    if let Ok(p) = crate::paths::settings_json() {
        out.extend(load_settings_file(&p, Scope::User, &attribute));
    }

    Ok(out)
}

/// Load servers from a file whose top-level shape is `{"mcpServers": {...}}`.
/// Used for every Claude settings.json variant.
fn load_settings_file(
    path: &Path,
    scope: Scope,
    attribute: &dyn Fn(&str) -> Source,
) -> Vec<McpServer> {
    let Some(val) = read_json(path) else {
        return vec![];
    };
    let Some(obj) = val.get("mcpServers").and_then(|v| v.as_object()) else {
        return vec![];
    };
    obj.iter()
        .filter_map(|(name, entry)| {
            parse_transport(entry).map(|transport| McpServer {
                name: name.clone(),
                scope: scope.clone(),
                transport,
                source: attribute(name),
                source_file: path.to_path_buf(),
            })
        })
        .collect()
}

/// Load servers from a `.mcp.json` file. Accepts BOTH real-world schemas:
/// the wrapped `{"mcpServers": {...}}` form documented in the spec, and the
/// flat `{ "<name>": {...} }` form that many plugins ship.
fn load_mcp_json(path: &Path, scope: Scope, attribute: &dyn Fn(&str) -> Source) -> Vec<McpServer> {
    let Some(val) = read_json(path) else {
        return vec![];
    };
    let map = val
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .or_else(|| val.as_object());
    let Some(obj) = map else {
        return vec![];
    };
    obj.iter()
        .filter_map(|(name, entry)| {
            parse_transport(entry).map(|transport| McpServer {
                name: name.clone(),
                scope: scope.clone(),
                transport,
                source: attribute(name),
                source_file: path.to_path_buf(),
            })
        })
        .collect()
}

fn read_json(path: &Path) -> Option<Value> {
    if !path.exists() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<Value>(&bytes).ok()
}

fn parse_transport(entry: &Value) -> Option<Transport> {
    let obj = entry.as_object()?;
    let kind = obj.get("type").and_then(|v| v.as_str());
    match kind {
        Some("http") => Some(Transport::Http {
            url: obj.get("url")?.as_str()?.to_string(),
            header_keys: key_list(obj.get("headers")),
        }),
        Some("sse") => Some(Transport::Sse {
            url: obj.get("url")?.as_str()?.to_string(),
            header_keys: key_list(obj.get("headers")),
        }),
        // No `type` → stdio (Claude's default).
        _ => Some(Transport::Stdio {
            command: obj.get("command")?.as_str()?.to_string(),
            args: obj
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            env_keys: key_list(obj.get("env")),
        }),
    }
}

fn key_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(|x| x.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// Walk enabled plugins' install paths, parse their `.claude-plugin/plugin.json`
/// (for an embedded `mcpServers` key) and any sibling `.mcp.json` file, and
/// build a name → plugin-name lookup for attribution.
fn build_plugin_mcp_index() -> Result<BTreeMap<String, String>> {
    let mut idx = BTreeMap::new();

    let settings_path = crate::paths::settings_json()?;
    if !settings_path.exists() {
        return Ok(idx);
    }
    let settings = crate::settings::load(&settings_path)?;
    let Some(ep) = crate::settings::enabled_plugins(&settings) else {
        return Ok(idx);
    };

    let inv_path = crate::paths::installed_plugins_json()?;
    let inv = crate::inventory::load(&inv_path)?;
    let plugins = crate::inventory::plugins(&inv);

    for qualified in ep.keys() {
        let Some((plugin_name, _marketplace)) = qualified.split_once('@') else {
            continue;
        };
        // Walk every recorded install path for this plugin.
        let install_paths: Vec<PathBuf> = plugins
            .and_then(|p| p.get(qualified))
            .and_then(|v| v.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| e.get("installPath").and_then(|p| p.as_str()))
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default();

        for root in install_paths {
            // Embedded mcpServers in plugin.json
            let plugin_json = root.join(".claude-plugin").join("plugin.json");
            if let Some(v) = read_json(&plugin_json) {
                if let Some(obj) = v.get("mcpServers").and_then(|x| x.as_object()) {
                    for name in obj.keys() {
                        idx.insert(name.clone(), plugin_name.to_string());
                    }
                }
            }
            // Sibling .mcp.json (either schema)
            let mcp_json = root.join(".mcp.json");
            if let Some(v) = read_json(&mcp_json) {
                let map = v
                    .get("mcpServers")
                    .and_then(|x| x.as_object())
                    .or_else(|| v.as_object());
                if let Some(obj) = map {
                    for name in obj.keys() {
                        idx.insert(name.clone(), plugin_name.to_string());
                    }
                }
            }
        }
    }
    Ok(idx)
}

/// Platform-specific path for org/IT-deployed managed settings.
/// Overridable via `ZSKILLS_MANAGED_SETTINGS` for testing.
fn managed_settings_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ZSKILLS_MANAGED_SETTINGS") {
        return Some(PathBuf::from(p));
    }
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from(
            "/Library/Application Support/ClaudeCode/managed-settings.json",
        ))
    }
    #[cfg(target_os = "linux")]
    {
        Some(PathBuf::from("/etc/claude-code/managed-settings.json"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_stdio_basic() {
        let v = json!({"command":"npx","args":["-y","@x/server"]});
        let t = parse_transport(&v).unwrap();
        assert_eq!(t.kind(), "stdio");
        assert_eq!(t.short(), "npx -y @x/server");
        assert_eq!(t.sensitive_count(), 0);
    }

    #[test]
    fn parse_stdio_counts_env_keys_only() {
        let v = json!({"command":"x","env":{"A":"${A}","B":"literal"}});
        assert_eq!(parse_transport(&v).unwrap().sensitive_count(), 2);
    }

    #[test]
    fn parse_http_with_headers() {
        let v = json!({"type":"http","url":"https://x.example","headers":{"Authorization":"Bearer ${X}"}});
        let t = parse_transport(&v).unwrap();
        assert_eq!(t.kind(), "http");
        assert_eq!(t.short(), "https://x.example");
        assert_eq!(t.sensitive_count(), 1);
    }

    #[test]
    fn parse_sse_flagged_distinctly() {
        let v = json!({"type":"sse","url":"https://s.example"});
        assert_eq!(parse_transport(&v).unwrap().kind(), "sse");
    }

    #[test]
    fn parse_skips_malformed() {
        let v = json!({"type":"http"}); // missing url
        assert!(parse_transport(&v).is_none());
    }
}
