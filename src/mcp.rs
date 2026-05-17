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

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
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
        /// Env var *keys* only — values are never stored, only briefly inspected
        /// for `${VAR}` references which land in `env_refs`. Both `key` and `var-name`
        /// are safe to surface; raw values are not, so they get dropped.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env_keys: Vec<String>,
        /// Names of `${VAR}` references appearing anywhere in the env values.
        /// Used by `doctor` to verify each referenced env var is actually set.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env_refs: Vec<String>,
    },
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        header_keys: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        header_refs: Vec<String>,
    },
    Sse {
        url: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        header_keys: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        header_refs: Vec<String>,
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

    /// Names of all `${VAR}` references in this server's values. Doctor checks
    /// each against the process environment.
    pub fn referenced_vars(&self) -> &[String] {
        match self {
            Transport::Stdio { env_refs, .. } => env_refs,
            Transport::Http { header_refs, .. } | Transport::Sse { header_refs, .. } => header_refs,
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
        Some("http") => {
            let (header_keys, header_refs) = keys_and_refs(obj.get("headers"));
            Some(Transport::Http {
                url: obj.get("url")?.as_str()?.to_string(),
                header_keys,
                header_refs,
            })
        }
        Some("sse") => {
            let (header_keys, header_refs) = keys_and_refs(obj.get("headers"));
            Some(Transport::Sse {
                url: obj.get("url")?.as_str()?.to_string(),
                header_keys,
                header_refs,
            })
        }
        // No `type` → stdio (Claude's default).
        _ => {
            let (env_keys, mut env_refs) = keys_and_refs(obj.get("env"));
            let args: Vec<String> = obj
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            // Real-world configs (e.g. the mcp-remote proxy pattern) embed
            // `${VAR}` references inside args, not just env values. Surface
            // those too so doctor can check whether they're set.
            for a in &args {
                for r in extract_var_refs(a) {
                    if !env_refs.contains(&r) {
                        env_refs.push(r);
                    }
                }
            }
            Some(Transport::Stdio {
                command: obj.get("command")?.as_str()?.to_string(),
                args,
                env_keys,
                env_refs,
            })
        }
    }
}

/// For an env/headers object, return (keys, ${VAR} refs found in values).
/// Values themselves are dropped — only the `${VAR}` variable *names* are kept.
fn keys_and_refs(v: Option<&Value>) -> (Vec<String>, Vec<String>) {
    let Some(obj) = v.and_then(|x| x.as_object()) else {
        return (vec![], vec![]);
    };
    let mut keys: Vec<String> = Vec::new();
    let mut refs: Vec<String> = Vec::new();
    for (k, val) in obj {
        keys.push(k.clone());
        if let Some(s) = val.as_str() {
            for r in extract_var_refs(s) {
                if !refs.contains(&r) {
                    refs.push(r);
                }
            }
        }
    }
    (keys, refs)
}

/// Pull every `${VAR}` reference out of a string, returning just the variable names.
/// Accepts identifiers matching `[A-Za-z_][A-Za-z0-9_]*` inside the braces.
fn extract_var_refs(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'{' {
            let start = i + 2;
            if let Some(end_off) = bytes[start..].iter().position(|&b| b == b'}') {
                let name = &s[start..start + end_off];
                let valid = !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !name.chars().next().unwrap().is_ascii_digit();
                if valid {
                    out.push(name.to_string());
                }
                i = start + end_off + 1;
                continue;
            }
        }
        i += 1;
    }
    out
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

/// Resolve the on-disk file zskills writes to for a given scope.
/// Returns the path AND a flag telling callers whether to use the wrapped
/// `{"mcpServers": {...}}` JSON shape (true) or a flat top-level map (false).
///
/// - `user`    → `~/.claude.json` (wrapped — matches what `claude mcp` writes).
/// - `project` → `<cwd>/.mcp.json` (wrapped *if* the file already exists with
///   the wrapper; otherwise flat — preserves what's there. New files get wrapped
///   per the spec).
/// - `local`   → `<cwd>/.claude.local/settings.json` (wrapped — it's a
///   settings.json variant).
///
/// `managed` is intentionally not supported: that scope is read-only.
pub fn write_target(scope: &Scope) -> Result<(PathBuf, bool)> {
    match scope {
        Scope::User => Ok((crate::paths::claude_json()?, true)),
        Scope::Local => {
            let cwd = std::env::current_dir()?;
            Ok((cwd.join(".claude.local").join("settings.json"), true))
        }
        Scope::Project => {
            let cwd = std::env::current_dir()?;
            let path = cwd.join(".mcp.json");
            let wrapped = if !path.exists() {
                true // default to spec-compliant for new files
            } else {
                match read_json(&path) {
                    Some(v) => v.get("mcpServers").is_some(),
                    None => true,
                }
            };
            Ok((path, wrapped))
        }
        Scope::Managed => {
            anyhow::bail!("cannot write to managed scope — deployed by IT, not zskills")
        }
    }
}

/// Read back the raw JSON entry for `name` at `scope` from whichever file
/// currently declares it. Used by `sync --adopt` to copy a server's full
/// config (including env/header values) into skills.toml without re-asking
/// the user for transport details.
pub fn read_raw(scope: &Scope, name: &str) -> Option<Value> {
    let candidates: Vec<PathBuf> = match scope {
        Scope::User => {
            let mut v = vec![];
            if let Ok(p) = crate::paths::claude_json() {
                v.push(p);
            }
            if let Ok(p) = crate::paths::settings_json() {
                v.push(p);
            }
            v
        }
        Scope::Project => std::env::current_dir()
            .ok()
            .map(|cwd| {
                vec![
                    cwd.join(".mcp.json"),
                    cwd.join(".claude").join("settings.json"),
                ]
            })
            .unwrap_or_default(),
        Scope::Local => std::env::current_dir()
            .ok()
            .map(|cwd| vec![cwd.join(".claude.local").join("settings.json")])
            .unwrap_or_default(),
        Scope::Managed => managed_settings_path().into_iter().collect(),
    };
    for path in candidates {
        let Some(val) = read_json(&path) else { continue };
        let map = val
            .get("mcpServers")
            .and_then(|v| v.as_object())
            .or_else(|| val.as_object());
        if let Some(obj) = map {
            if let Some(entry) = obj.get(name) {
                return Some(entry.clone());
            }
        }
    }
    None
}

/// Set or replace one MCP server entry in the target file for `scope`. Atomic.
/// `name` is the server name; `entry` is the JSON value to store under it.
pub fn upsert(scope: &Scope, name: &str, entry: serde_json::Value) -> Result<()> {
    let (path, wrapped) = write_target(scope)?;
    write_with_mutation(&path, wrapped, |servers| {
        servers.insert(name.to_string(), entry);
    })
}

/// Remove one MCP server entry from the target file for `scope`. Atomic.
/// No-op if the entry isn't present.
pub fn remove(scope: &Scope, name: &str) -> Result<()> {
    let (path, wrapped) = write_target(scope)?;
    if !path.exists() {
        return Ok(());
    }
    write_with_mutation(&path, wrapped, |servers| {
        servers.shift_remove(name);
    })
}

/// Read the target file (creating an empty shell if missing), apply `mutate` to
/// the mcpServers map, and write back atomically. Preserves all other top-level
/// keys (hooks, permissions, env, etc.) and the existing wrapped-vs-flat shape.
fn write_with_mutation(
    path: &Path,
    wrapped: bool,
    mutate: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> Result<()> {
    use serde_json::{Map, Value};
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut top: Map<String, Value> = if path.exists() {
        let bytes = std::fs::read(path)?;
        match serde_json::from_slice::<Value>(&bytes)? {
            Value::Object(m) => m,
            _ => anyhow::bail!("{} is not a JSON object", path.display()),
        }
    } else {
        Map::new()
    };

    if wrapped {
        let servers = top
            .entry("mcpServers")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("mcpServers must be a JSON object"))?;
        mutate(servers);
    } else {
        mutate(&mut top);
    }

    crate::settings::save(path, &top)
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
        let t = parse_transport(&v).unwrap();
        assert_eq!(t.sensitive_count(), 2);
        // Only "A" references a ${VAR}; "B" is a literal and contributes no refs.
        assert_eq!(t.referenced_vars(), &["A".to_string()]);
    }

    #[test]
    fn parse_http_with_headers() {
        let v = json!({"type":"http","url":"https://x.example","headers":{"Authorization":"Bearer ${X}"}});
        let t = parse_transport(&v).unwrap();
        assert_eq!(t.kind(), "http");
        assert_eq!(t.short(), "https://x.example");
        assert_eq!(t.sensitive_count(), 1);
        assert_eq!(t.referenced_vars(), &["X".to_string()]);
    }

    #[test]
    fn extract_var_refs_handles_embedded_and_multiple() {
        assert_eq!(extract_var_refs("${A}"), vec!["A"]);
        assert_eq!(extract_var_refs("Bearer ${TOKEN}"), vec!["TOKEN"]);
        assert_eq!(extract_var_refs("${A}-${B}"), vec!["A", "B"]);
        assert_eq!(extract_var_refs("literal"), Vec::<String>::new());
        // Bad shapes don't panic and don't produce false matches.
        assert_eq!(extract_var_refs("${}"), Vec::<String>::new());
        assert_eq!(extract_var_refs("${1BAD}"), Vec::<String>::new());
        assert_eq!(extract_var_refs("$NOTBRACED"), Vec::<String>::new());
    }

    #[test]
    fn refs_are_deduped() {
        let v = json!({"command":"x","env":{"A":"${TOK}","B":"x ${TOK} y"}});
        assert_eq!(
            parse_transport(&v).unwrap().referenced_vars(),
            &["TOK".to_string()]
        );
    }

    #[test]
    fn parse_stdio_extracts_refs_from_args_too() {
        // mcp-remote proxy pattern: `${VAR}` is referenced inside args,
        // not in the env block. Doctor needs to see those.
        let v = json!({
            "command": "npx",
            "args": [
                "mcp-remote",
                "https://example.com",
                "--header", "Authorization:${AUTH_HEADER}",
                "--header", "X-User-Name:${USER_NAME}"
            ],
            "env": {}
        });
        let refs = parse_transport(&v).unwrap().referenced_vars().to_vec();
        assert!(refs.contains(&"AUTH_HEADER".to_string()));
        assert!(refs.contains(&"USER_NAME".to_string()));
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
