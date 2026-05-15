use anyhow::Result;
use owo_colors::OwoColorize;

pub fn run(fix: bool) -> Result<()> {
    let report = crate::reconcile::run()?;
    let mut issues = 0;

    issues += check_mcps();

    if !report.enabled_orphan.is_empty() {
        issues += report.enabled_orphan.len();
        println!(
            "{} {} plugins enabled but not installed:",
            "✗".red(),
            report.enabled_orphan.len()
        );
        for k in &report.enabled_orphan {
            println!("  - {}", k);
        }
    }

    if !report.installed_orphan.is_empty() {
        issues += report.installed_orphan.len();
        println!(
            "{} {} plugins installed from missing marketplaces:",
            "✗".red(),
            report.installed_orphan.len()
        );
        for k in &report.installed_orphan {
            println!("  - {}", k);
        }
    }

    // Agent skills: entries in inventory but missing on disk
    let inv = crate::agent_skill::load_inventory()?;
    let on_disk: std::collections::BTreeSet<String> = crate::agent_skill::installed_on_disk()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let agent_inventory_missing: Vec<String> = inv
        .agent_skills
        .keys()
        .filter(|k| !on_disk.contains(k.as_str()))
        .cloned()
        .collect();
    if !agent_inventory_missing.is_empty() {
        issues += agent_inventory_missing.len();
        println!(
            "{} {} agent skills tracked in inventory but missing on disk:",
            "✗".red(),
            agent_inventory_missing.len()
        );
        for k in &agent_inventory_missing {
            println!("  - {}", k);
        }
    }

    if issues == 0 {
        println!(
            "{} All good — disk, inventory, and settings are in sync.",
            "✓".green()
        );
        return Ok(());
    }

    if fix {
        // Plugins: remove orphan enabledPlugins entries.
        let settings_path = crate::paths::settings_json()?;
        let mut settings = crate::settings::load(&settings_path)?;
        let ep = crate::settings::enabled_plugins_mut(&mut settings);
        for k in &report.enabled_orphan {
            ep.remove(k);
            println!("  removed {} from enabledPlugins", k);
        }
        crate::settings::save(&settings_path, &settings)?;

        // Agent skills: drop inventory entries with no bytes.
        let mut inv = crate::agent_skill::load_inventory()?;
        for k in &agent_inventory_missing {
            inv.agent_skills.remove(k);
            println!("  removed {} from agent-skill inventory", k);
        }
        crate::agent_skill::save_inventory(&inv)?;

        println!("{} Fixed {} issue(s).", "✓".green(), issues);
    } else {
        println!("\nRun {} to clean up.", "zskills doctor --fix".bold());
    }

    Ok(())
}

/// Static MCP server checks. Returns the number of warnings emitted.
///
/// We don't try to spawn or talk to the servers themselves — that's a runtime
/// concern that belongs to Claude Code, and replicating it would risk divergent
/// diagnoses. What we *can* verify without spawning:
///
/// 1. **stdio** servers reference a `command` that resolves on `$PATH`.
/// 2. Every `${VAR}` referenced in `env` (stdio) or `headers` (http/sse) is
///    actually defined in the user's environment.
/// 3. SSE servers get a deprecation note (the spec marks `sse` as legacy).
///
/// `--fix` is a no-op for MCPs in M3: none of these failures are auto-fixable
/// (we won't install a missing binary or invent an env var). Surfacing them is
/// the value-add.
fn check_mcps() -> usize {
    let mcps = match crate::mcp::load_all() {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if mcps.is_empty() {
        return 0;
    }
    let mut issues = 0;
    let mut by_server: Vec<(String, String, Vec<String>)> = Vec::new(); // (name, scope, messages)

    for m in &mcps {
        let mut msgs: Vec<String> = Vec::new();
        if let crate::mcp::Transport::Stdio { command, .. } = &m.transport {
            if which::which(command).is_err() {
                msgs.push(format!("command not found on $PATH: {}", command));
            }
        }
        for var in m.transport.referenced_vars() {
            if std::env::var(var).is_err() {
                msgs.push(format!("env var `{}` is referenced but not set", var));
            }
        }
        if m.transport.kind() == "sse" {
            msgs.push("transport `sse` is deprecated; switch to `http`".to_string());
        }
        if !msgs.is_empty() {
            issues += msgs.len();
            by_server.push((m.name.clone(), m.scope.label().to_string(), msgs));
        }
    }

    if by_server.is_empty() {
        return 0;
    }
    println!("{} {} MCP issue(s):", "✗".red(), issues);
    for (name, scope, msgs) in &by_server {
        println!("  {} {}", format!("[{}]", scope).dimmed(), name.bold());
        for msg in msgs {
            println!("    - {}", msg);
        }
    }
    issues
}
