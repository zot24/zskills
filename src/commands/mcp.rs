//! `zskills mcp add` / `zskills mcp remove`. Dual-write intent then state.

use anyhow::Result;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::manifest::McpEntry;
use crate::mcp::Scope;

#[allow(clippy::too_many_arguments)]
pub fn add(
    name: String,
    transport: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    env: Vec<(String, String)>,
    url: Option<String>,
    header: Vec<(String, String)>,
    scope: Option<String>,
    file: Option<PathBuf>,
    harness: Vec<crate::harness::Harness>,
) -> Result<()> {
    let entry = McpEntry {
        name: name.clone(),
        transport: transport.clone(),
        command,
        args,
        env: env.into_iter().collect::<BTreeMap<_, _>>(),
        url,
        headers: header.into_iter().collect::<BTreeMap<_, _>>(),
        scope: scope.clone(),
        mcp_harnesses: harness.iter().map(|h| h.as_str().to_string()).collect(),
    };
    entry.validate()?;
    let scope_s = entry.scope_kind()?;
    let mcp_scope = parse_scope(scope_s)?;

    let manifest_path = file.or_else(crate::manifest::discover);
    if let Some(path) = manifest_path.as_ref() {
        let outcome = crate::manifest::upsert_mcp(path, &entry)?;
        println!(
            "{} {outcome} [[mcps]] {} ({}) in {}",
            "✓".green(),
            name.bold(),
            scope_s,
            path.display().to_string().dimmed()
        );
    } else {
        eprintln!(
            "{} no skills.toml found — wrote runtime only; next sync cannot reproduce this",
            "!".yellow()
        );
    }

    let (_, mcp_defaults) = crate::harness::load_defaults();
    let hs = crate::harness::resolve(
        &harness,
        &mcp_defaults,
        &[],
        vec![crate::harness::Harness::Claude],
    )?;
    if hs.contains(&crate::harness::Harness::Claude) {
        crate::mcp::upsert(&mcp_scope, &name, entry.to_json_value())?;
        println!("{} applied mcp {} ({})", "✓".green(), name.bold(), scope_s);
    }
    crate::harness::print_mcp_skips(&hs);
    Ok(())
}

pub fn remove(name: String, scope: Option<String>, file: Option<PathBuf>) -> Result<()> {
    let scope_s = scope.as_deref().unwrap_or("user");
    let mcp_scope = parse_scope(scope_s)?;

    let all = crate::mcp::load_all().unwrap_or_default();
    let other_scopes: Vec<&str> = all
        .iter()
        .filter(|m| m.name == name && m.scope.label() != scope_s)
        .map(|m| m.scope.label())
        .collect();
    let at_requested: Vec<_> = all
        .iter()
        .filter(|m| m.name == name && m.scope.label() == scope_s)
        .collect();

    if at_requested.is_empty() {
        if other_scopes.is_empty() {
            anyhow::bail!("MCP '{name}' is not configured at scope {scope_s}");
        }
        anyhow::bail!(
            "MCP '{name}' is not configured at scope {scope_s} (found at: {}); pass --scope",
            other_scopes.join("|")
        );
    }
    if at_requested.len() > 1 {
        // Two files at one scope: still delete both via mcp::remove.
    }
    let also_elsewhere: Vec<&str> = all
        .iter()
        .filter(|m| m.name == name && m.scope.label() != scope_s)
        .map(|m| m.scope.label())
        .collect();
    if !also_elsewhere.is_empty() && scope.is_none() {
        let mut scopes: Vec<&str> = vec![scope_s];
        scopes.extend(also_elsewhere);
        anyhow::bail!(
            "MCP '{name}' exists at scopes: {}; pass --scope",
            scopes.join(", ")
        );
    }

    let manifest_path = file.or_else(crate::manifest::discover);
    if let Some(path) = manifest_path.as_ref() {
        crate::manifest::drop_mcp(path, &name, scope_s)?;
    } else {
        eprintln!(
            "{} no skills.toml found — runtime only; intent was not updated",
            "!".yellow()
        );
    }

    let deleted = crate::mcp::remove(&mcp_scope, &name)?;
    if !deleted {
        anyhow::bail!("MCP '{name}' is not configured at scope {scope_s}");
    }
    println!("{} removed mcp {} ({})", "-".yellow(), name.bold(), scope_s);
    Ok(())
}

fn parse_scope(s: &str) -> Result<Scope> {
    match s {
        "user" => Ok(Scope::User),
        "project" => Ok(Scope::Project),
        "local" => Ok(Scope::Local),
        "managed" => anyhow::bail!("cannot write to managed scope — deployed by IT, not zskills"),
        other => anyhow::bail!("unknown scope {other:?} (must be user, project, or local)"),
    }
}

pub fn parse_kv(s: &str) -> Result<(String, String), String> {
    let Some((k, v)) = s.split_once('=') else {
        return Err(format!("expected KEY=VALUE, got {s:?}"));
    };
    if k.is_empty() {
        return Err("empty KEY in KEY=VALUE".into());
    }
    Ok((k.to_string(), v.to_string()))
}
