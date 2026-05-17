//! `sync` — apply a declarative skills.toml manifest. The headline command.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

pub fn run(file: Option<PathBuf>, dry_run: bool, prune: bool, adopt: bool) -> Result<()> {
    // Warn loudly if a `./skills.toml` exists and the user didn't pass --file.
    if file.is_none() {
        if let Some(cwd_path) = crate::manifest::cwd_skills_toml() {
            eprintln!(
                "{} ignoring {} — pass {} to use it",
                "!".yellow(),
                cwd_path.display().to_string().dimmed(),
                "--file <path>".bold()
            );
        }
    }
    let path = file
        .or_else(crate::manifest::discover)
        .ok_or_else(|| anyhow::anyhow!("no skills.toml found at ~/.config/zskills/skills.toml"))?;
    println!("Manifest: {}", path.display().to_string().dimmed());

    let manifest = crate::manifest::load(&path)?;

    // -------- 1) Plugin reconciliation --------
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;

    let mut desired_plugins: BTreeSet<String> = BTreeSet::new();
    for entry in &manifest.skills {
        let qualified = match entry.qualified() {
            Some(q) => q,
            None => match crate::marketplace::resolve_spec(&entry.name, &known) {
                Ok(q) => q,
                Err(e) => {
                    eprintln!("{} {}: {}", "✗".red(), entry.name, e);
                    continue;
                }
            },
        };
        desired_plugins.insert(qualified);
    }

    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    let current_plugins: BTreeSet<String> = crate::settings::enabled_plugins(&settings)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| {
                    if v.as_bool().unwrap_or(false) {
                        Some(k.clone())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let plugins_to_enable: Vec<_> = desired_plugins.difference(&current_plugins).collect();
    let plugins_to_disable: Vec<_> = current_plugins.difference(&desired_plugins).collect();

    // -------- 2) Agent Skills reconciliation --------
    // The manifest carries (source, optional name). We need to compare against the inventory,
    // which carries (skill_name -> source). Build a desired-names set, but we also need to
    // remember the source for each so we can install.
    let mut desired_named: BTreeSet<String> = BTreeSet::new();
    let mut deferred_sources: Vec<&crate::manifest::AgentSkillEntry> = Vec::new();
    for entry in &manifest.agent_skills {
        match (&entry.name, &entry.source) {
            (Some(n), _) => {
                desired_named.insert(n.clone());
            }
            (None, Some(_)) => {
                // Source without an explicit name — every skill in `skills/` of that repo.
                deferred_sources.push(entry);
            }
            (None, None) => {
                // Invalid: report below at apply time
            }
        }
    }

    let inv = crate::agent_skill::load_inventory()?;
    let current_managed: BTreeSet<String> = inv.agent_skills.keys().cloned().collect();
    let on_disk: BTreeSet<String> = crate::agent_skill::installed_on_disk()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let agent_to_install_named: Vec<_> = desired_named
        .iter()
        .filter(|n| !on_disk.contains(*n))
        .cloned()
        .collect();
    let _agent_to_refresh_named: Vec<String> = desired_named
        .iter()
        .filter(|n| on_disk.contains(*n))
        .cloned()
        .collect();

    // For source-only entries: only show "install" if at least one of the skills the
    // repo would provide isn't yet on disk OR tagged with this source. Otherwise we'd
    // re-install every sync, which is wasteful and noisy.
    let deferred_sources_to_install: Vec<&crate::manifest::AgentSkillEntry> = deferred_sources
        .iter()
        .filter(|e| {
            let Some(src) = &e.source else { return false };
            // If we've already inventoried anything from this source AND those entries
            // are all on disk, treat as "already present".
            let inventoried_from_source: Vec<&String> = inv
                .agent_skills
                .iter()
                .filter(|(_, entry)| entry.source == *src)
                .map(|(name, _)| name)
                .collect();
            if inventoried_from_source.is_empty() {
                return true;
            }
            inventoried_from_source
                .iter()
                .any(|n| !on_disk.contains(n.as_str()))
        })
        .copied()
        .collect();
    // Don't propose removing a skill that's owned by any manifest entry — either:
    //   (a) it came from a source-only [[agent_skills]] entry (we'll re-resolve), or
    //   (b) its inventory source is "npm:<pkg>" matching an [[agent_skills]] npm= entry, or
    //   (c) its name matches a `claims` glob on any entry.
    let agent_to_remove: Vec<_> = current_managed
        .iter()
        .filter(|n| !desired_named.contains(*n))
        .filter(|n| {
            let inv_src = inv.agent_skills.get(*n).map(|e| e.source.clone());
            let owned_by_manifest = manifest.agent_skills.iter().any(|e| {
                // (a) source-only entry whose source matches the inventory tag
                if e.source.is_some() && e.name.is_none() && e.source == inv_src {
                    return true;
                }
                // (b) npm entry whose tag matches
                if let Some(pkg) = &e.npm {
                    if inv_src.as_deref() == Some(&format!("npm:{}", pkg)) {
                        return true;
                    }
                }
                // (c) claims glob match on any entry
                e.claims
                    .iter()
                    .any(|pat| crate::agent_skill::glob_match(pat, n))
            });
            !owned_by_manifest
        })
        .cloned()
        .collect();

    // -------- 2.5) MCP reconciliation --------
    // Validate every manifest entry up-front; bail on any error so we don't half-apply.
    for m in &manifest.mcps {
        m.validate()?;
    }
    let desired_mcp_keys: BTreeSet<(crate::mcp::Scope, String)> = manifest
        .mcps
        .iter()
        .map(|m| {
            let scope = match m.scope_kind().unwrap() {
                "user" => crate::mcp::Scope::User,
                "project" => crate::mcp::Scope::Project,
                "local" => crate::mcp::Scope::Local,
                _ => unreachable!(),
            };
            (scope, m.name.clone())
        })
        .collect();
    // Current state: every writable, manually-added MCP. Skip managed (read-only)
    // and skip plugin-injected (owned by their plugin, not by zskills's manifest).
    let current_mcps = crate::mcp::load_all().unwrap_or_default();
    let current_mcp_keys: BTreeSet<(crate::mcp::Scope, String)> = current_mcps
        .iter()
        .filter(|m| m.scope != crate::mcp::Scope::Managed)
        .filter(|m| matches!(m.source, crate::mcp::Source::Manual))
        .map(|m| (m.scope.clone(), m.name.clone()))
        .collect();

    let mcps_to_install: Vec<_> = desired_mcp_keys.difference(&current_mcp_keys).collect();
    // Sync always rewrites the manifest's entries (overwrite-on-overlap) so the
    // file is the source of truth; explicit "update" tracking is unnecessary.
    let mcps_to_update: Vec<_> = desired_mcp_keys.intersection(&current_mcp_keys).collect();
    let mcps_to_remove: Vec<_> = current_mcp_keys.difference(&desired_mcp_keys).collect();

    // -------- 3) Print plan --------
    println!("\n{}", "Plan".bold());
    let nothing = plugins_to_enable.is_empty()
        && plugins_to_disable.is_empty()
        && agent_to_install_named.is_empty()
        && agent_to_remove.is_empty()
        && deferred_sources_to_install.is_empty()
        && mcps_to_install.is_empty()
        && mcps_to_update.is_empty()
        && mcps_to_remove.is_empty();
    if nothing {
        println!("  (no changes — manifest matches current state)");
        return Ok(());
    }

    for k in &plugins_to_enable {
        println!("  {} enable  plugin  {}", "+".green(), k);
    }
    for k in &plugins_to_disable {
        if adopt {
            println!(
                "  {} adopt   plugin  {} {}",
                "+".cyan(),
                k,
                "(enabled but not in manifest — adding)".dimmed()
            );
        } else {
            println!(
                "  {} disable plugin  {} {}",
                "-".yellow(),
                k,
                "(in settings but not in manifest)".dimmed()
            );
        }
    }
    for n in &agent_to_install_named {
        println!("  {} install agent   {}", "+".green(), n);
    }
    for entry in &deferred_sources_to_install {
        if let Some(s) = &entry.source {
            println!(
                "  {} install agent   {} {}",
                "+".green(),
                s,
                "(all skills in repo)".dimmed()
            );
        }
    }
    for n in &agent_to_remove {
        if adopt {
            println!(
                "  {} adopt   agent   {} {}",
                "+".cyan(),
                n,
                "(in inventory but not in manifest — adding)".dimmed()
            );
        } else if prune {
            println!(
                "  {} remove  agent   {} {}",
                "-".red(),
                n,
                "(installed but not in manifest — bytes will be DELETED)".dimmed()
            );
        } else {
            println!(
                "  {} skip    agent   {} {}",
                "·".dimmed(),
                n,
                "(in inventory but not in manifest — pass --prune to delete, or --adopt to add to manifest)".dimmed()
            );
        }
    }
    for (scope, name) in &mcps_to_install {
        println!(
            "  {} install mcp     {} {}",
            "+".green(),
            name,
            format!("({})", scope.label()).dimmed()
        );
    }
    for (scope, name) in &mcps_to_update {
        println!(
            "  {} update  mcp     {} {}",
            "~".cyan(),
            name,
            format!("({}) — manifest wins on conflict", scope.label()).dimmed()
        );
    }
    for (scope, name) in &mcps_to_remove {
        if adopt {
            println!(
                "  {} adopt   mcp     {} {}",
                "+".cyan(),
                name,
                format!("({}) — adding to manifest", scope.label()).dimmed()
            );
        } else if prune {
            println!(
                "  {} remove  mcp     {} {}",
                "-".red(),
                name,
                format!("({}) — not in manifest, will be deleted", scope.label()).dimmed()
            );
        } else {
            println!(
                "  {} skip    mcp     {} {}",
                "·".dimmed(),
                name,
                format!(
                    "({}) — in {} but not in manifest, pass --prune to delete, or --adopt to add to manifest",
                    scope.label(),
                    scope.label()
                )
                .dimmed()
            );
        }
    }

    if dry_run {
        println!("\n(dry-run; no changes written)");
        return Ok(());
    }

    // -------- 3.5) Adopt (optional) --------
    // When --adopt is passed, append every orphan to the manifest BEFORE the
    // reconciliation pass. After this the manifest contains the orphans, so
    // they're no longer "to remove" / "to disable" and the apply phase skips them.
    if adopt {
        let mut adopted = 0usize;
        for k in &plugins_to_disable {
            let (name, mp) = k
                .split_once('@')
                .map(|(n, m)| (n.to_string(), Some(m.to_string())))
                .unwrap_or_else(|| ((*k).clone(), None));
            let entry = crate::manifest::SkillEntry {
                name,
                marketplace: mp,
                version: None,
            };
            if crate::manifest::append_skill(&path, &entry)? {
                adopted += 1;
            }
        }
        for n in &agent_to_remove {
            let inv_entry = inv.agent_skills.get(n);
            let src = inv_entry.map(|e| e.source.as_str());
            let manifest_entry = match src {
                Some("local") | None => crate::manifest::AgentSkillEntry {
                    name: Some(n.clone()),
                    ..Default::default()
                },
                Some(s) if s.starts_with("npm:") => crate::manifest::AgentSkillEntry {
                    npm: Some(s.trim_start_matches("npm:").to_string()),
                    name: Some(n.clone()),
                    ..Default::default()
                },
                Some(s) => crate::manifest::AgentSkillEntry {
                    source: Some(s.to_string()),
                    name: Some(n.clone()),
                    ..Default::default()
                },
            };
            if crate::manifest::append_agent_skill(&path, &manifest_entry)? {
                adopted += 1;
            }
        }
        for (scope, name) in &mcps_to_remove {
            let raw = match crate::mcp::read_raw(scope, name) {
                Some(v) => v,
                None => {
                    eprintln!(
                        "{} mcp `{}` ({}): could not re-read config — skipping adoption",
                        "!".yellow(),
                        name,
                        scope.label()
                    );
                    continue;
                }
            };
            let mcp_entry = mcp_entry_from_raw(name, scope, &raw);
            if crate::manifest::append_mcp(&path, &mcp_entry)? {
                adopted += 1;
            }
        }
        println!(
            "\n{} adopted {} orphan(s) into {}",
            "✓".green(),
            adopted,
            path.display()
        );
        if adopted == 0 {
            return Ok(());
        }
        println!(
            "  {}",
            "re-run `zskills sync` to confirm the manifest now matches state".dimmed()
        );
        return Ok(());
    }

    // -------- 4) Apply --------
    let ep = crate::settings::enabled_plugins_mut(&mut settings);
    for k in &plugins_to_enable {
        ep.insert((*k).clone(), Value::Bool(true));
    }
    for k in &plugins_to_disable {
        ep.insert((*k).clone(), Value::Bool(false));
    }
    crate::settings::save(&settings_path, &settings)?;

    for entry in &manifest.agent_skills {
        if let Some(pkg) = entry.npm.as_deref() {
            match crate::agent_skill::install_npm(pkg, entry.install_cmd.as_deref(), &entry.claims)
            {
                Ok(names) => {
                    println!(
                        "  {} npm:{}  ({} skill{})",
                        "✓".green(),
                        pkg.bold(),
                        names.len(),
                        if names.len() == 1 { "" } else { "s" }
                    );
                }
                Err(e) => eprintln!("{} npm:{}: {}", "✗".red(), pkg, e),
            }
            continue;
        }
        match (entry.source.as_deref(), entry.name.as_deref()) {
            (Some(src), name) => {
                // Skip the (re-)install if the skill is already on disk + tagged
                // with this same source. `upgrade` is the deliberate refresh path.
                let inv_now = crate::agent_skill::load_inventory()?;
                let on_disk: std::collections::BTreeSet<String> =
                    crate::agent_skill::installed_on_disk()
                        .unwrap_or_default()
                        .into_iter()
                        .collect();
                let already_present = match name {
                    Some(n) => {
                        on_disk.contains(n)
                            && inv_now.agent_skills.get(n).is_some_and(|e| e.source == src)
                    }
                    None => false,
                };
                if already_present {
                    if let Some(n) = name {
                        println!(
                            "  {} {}  {}",
                            "·".dimmed(),
                            n,
                            format!("← {}  (already present)", src).dimmed()
                        );
                    }
                    continue;
                }
                match crate::agent_skill::install(src, name) {
                    Ok(names) => {
                        for n in &names {
                            println!("  installed agent skill {}", n.bold());
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {}: {}", "✗".red(), src, e);
                    }
                }
            }
            (None, Some(name)) if entry.npm.is_none() => {
                // Local-only entry: register in inventory if present on disk; don't fetch.
                let mut inv = crate::agent_skill::load_inventory()?;
                if !inv.agent_skills.contains_key(name) {
                    inv.agent_skills.insert(
                        name.to_string(),
                        crate::agent_skill::Entry {
                            source: "local".to_string(),
                            installed_at: format!(
                                "@{}",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0)
                            ),
                            head_sha: "local".to_string(),
                        },
                    );
                    crate::agent_skill::save_inventory(&inv)?;
                    println!("  tracked local agent skill {}", name.bold());
                }
            }
            (None, None) => {
                eprintln!(
                    "{} agent_skill entry needs either `source` or `name`",
                    "✗".red()
                );
            }
            (None, Some(_)) => {
                // npm path; already handled by the early `if let Some(pkg) = entry.npm` continue.
                // Reachable only if npm = Some(_) AND name = Some(_) — name is informational
                // for npm entries.
            }
        }
    }

    if prune {
        for n in &agent_to_remove {
            match crate::agent_skill::remove(n) {
                Ok(_) => println!("  removed agent skill {}", n.bold()),
                Err(e) => eprintln!("{} {}: {}", "✗".red(), n, e),
            }
        }
    }

    // -------- 5) Apply MCP changes --------
    for m in &manifest.mcps {
        // validate() already ran above, but scope_kind() may fail at runtime if a
        // future field is added; tolerate per-entry errors without aborting the rest.
        let scope = match m.scope_kind() {
            Ok("user") => crate::mcp::Scope::User,
            Ok("project") => crate::mcp::Scope::Project,
            Ok("local") => crate::mcp::Scope::Local,
            _ => {
                eprintln!("{} mcp `{}`: invalid scope", "✗".red(), m.name);
                continue;
            }
        };
        if let Err(e) = crate::mcp::upsert(&scope, &m.name, m.to_json_value()) {
            eprintln!("{} mcp `{}`: {}", "✗".red(), m.name, e);
        } else {
            println!("  applied mcp {} ({})", m.name.bold(), scope.label());
        }
    }
    if prune {
        for (scope, name) in &mcps_to_remove {
            if let Err(e) = crate::mcp::remove(scope, name) {
                eprintln!("{} mcp `{}`: {}", "✗".red(), name, e);
            } else {
                println!("  removed mcp {} ({})", name.bold(), scope.label());
            }
        }
    }

    println!("\n{} applied.", "✓".green());
    Ok(())
}

/// Convert a raw `mcpServers["<name>"]` JSON value (as it lives in
/// settings.json / .mcp.json / .claude.json) into a manifest `McpEntry`.
/// Preserves env / header *values* verbatim — these may be literal secrets,
/// `${VAR}` references, or both. The user can sanitise after adoption.
fn mcp_entry_from_raw(
    name: &str,
    scope: &crate::mcp::Scope,
    raw: &serde_json::Value,
) -> crate::manifest::McpEntry {
    let mut e = crate::manifest::McpEntry {
        name: name.to_string(),
        scope: Some(scope.label().to_string()),
        ..Default::default()
    };
    let Some(obj) = raw.as_object() else { return e };

    match obj.get("type").and_then(|v| v.as_str()) {
        Some("http") => {
            e.transport = Some("http".into());
            e.url = obj.get("url").and_then(|v| v.as_str()).map(str::to_string);
            if let Some(h) = obj.get("headers").and_then(|v| v.as_object()) {
                for (k, v) in h {
                    if let Some(s) = v.as_str() {
                        e.headers.insert(k.clone(), s.to_string());
                    }
                }
            }
        }
        Some("sse") => {
            e.transport = Some("sse".into());
            e.url = obj.get("url").and_then(|v| v.as_str()).map(str::to_string);
            if let Some(h) = obj.get("headers").and_then(|v| v.as_object()) {
                for (k, v) in h {
                    if let Some(s) = v.as_str() {
                        e.headers.insert(k.clone(), s.to_string());
                    }
                }
            }
        }
        _ => {
            // stdio (Claude's default when `type` is absent)
            e.command = obj
                .get("command")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let Some(args) = obj.get("args").and_then(|v| v.as_array()) {
                e.args = args
                    .iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect();
            }
            if let Some(env) = obj.get("env").and_then(|v| v.as_object()) {
                for (k, v) in env {
                    if let Some(s) = v.as_str() {
                        e.env.insert(k.clone(), s.to_string());
                    }
                }
            }
        }
    }
    e
}
