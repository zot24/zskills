use anyhow::Result;
use owo_colors::OwoColorize;

pub fn run(fix: bool) -> Result<()> {
    let report = crate::reconcile::run()?;
    // Two counters, deliberately. `issues` is everything worth telling the user
    // about; `fixable` is the subset `--fix` has code for. Comparing repairs against
    // the first is how `--fix` ends up reporting failure forever over a finding it
    // was never going to touch — one deprecated MCP server was enough.
    let mut issues = 0;
    let mut fixable = 0;

    issues += check_mcps();
    issues += check_mcp_intent();
    issues += check_stale_zskills_skill_md();

    // A marketplace entry with no `lastUpdated` string makes Claude Code reject the
    // *whole* known_marketplaces.json, which breaks every `claude plugin install`.
    // Reporting "All good" while that is true is the single most expensive lie doctor
    // can tell, so it is checked first.
    let known_path = crate::paths::known_marketplaces_json()?;
    let known = crate::marketplace::load_known(&known_path)?;
    let stale_taps: Vec<String> = known
        .iter()
        .filter(|(_, entry)| crate::marketplace::missing_last_updated(entry))
        .map(|(name, _)| name.clone())
        .collect();
    if !stale_taps.is_empty() {
        issues += stale_taps.len();
        fixable += stale_taps.len();
        println!(
            "{} {} marketplace(s) missing a `lastUpdated` string in {}:",
            "✗".red(),
            stale_taps.len(),
            known_path.display()
        );
        for name in &stale_taps {
            println!("  - {}", name);
        }
        println!(
            "  {}",
            "Claude Code rejects the whole file over this — every `claude plugin install` fails with \"Marketplace configuration file is corrupted\"".dimmed()
        );
    }

    // "Enabled but not installed" splits three ways, and the three want different fixes:
    // install it, drop it, or — when we genuinely cannot tell — touch nothing.
    use crate::marketplace::Offer;
    let mut fetchable: Vec<String> = Vec::new();
    let mut dangling: Vec<String> = Vec::new();
    let mut unverifiable: Vec<String> = Vec::new();
    for k in &report.enabled_orphan {
        match crate::marketplace::plugin_offer(&known, k) {
            Offer::Yes => fetchable.push(k.clone()),
            Offer::No => dangling.push(k.clone()),
            Offer::Unknown => unverifiable.push(k.clone()),
        }
    }

    if !fetchable.is_empty() {
        issues += fetchable.len();
        fixable += fetchable.len();
        println!(
            "{} {} plugin(s) enabled but not installed (bytes never fetched):",
            "✗".red(),
            fetchable.len()
        );
        for k in &fetchable {
            println!("  - {}", k);
        }
        println!(
            "  {}",
            "these are real plugins in a registered marketplace — `--fix` installs them".dimmed()
        );
    }

    if !dangling.is_empty() {
        issues += dangling.len();
        fixable += dangling.len();
        println!(
            "{} {} plugin(s) enabled but offered by no registered marketplace:",
            "✗".red(),
            dangling.len()
        );
        for k in &dangling {
            println!("  - {}", k);
        }
        println!(
            "  {}",
            "no tap carries these — `--fix` drops the dangling enable".dimmed()
        );
    }

    if !unverifiable.is_empty() {
        issues += unverifiable.len();
        println!(
            "{} {} plugin(s) enabled but unverifiable — the marketplace manifest could not be read:",
            "✗".red(),
            unverifiable.len()
        );
        for k in &unverifiable {
            println!("  - {}", k);
        }
        println!(
            "  {}",
            "`--fix` will not touch these: an unreadable manifest is not evidence the plugin is bogus. Run `zskills marketplace update` first.".dimmed()
        );
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
        fixable += agent_inventory_missing.len();
        println!(
            "{} {} agent skills tracked in inventory but missing on disk:",
            "✗".red(),
            agent_inventory_missing.len()
        );
        for k in &agent_inventory_missing {
            println!("  - {}", k);
        }
    }

    // Agent skills: legacy full-repo installs. Before sparse installs
    // (v0.8), a root-level skill was a verbatim copy of the whole clone —
    // reliably detectable by the `.git/` directory it dragged along.
    let full_repo_installs = find_full_repo_installs(&inv)?;
    if !full_repo_installs.is_empty() {
        issues += full_repo_installs.len();
        fixable += full_repo_installs.len();
        println!(
            "{} {} agent skill(s) are full-repo installs (whole source tree, not just the skill):",
            "✗".red(),
            full_repo_installs.len()
        );
        for (name, source) in &full_repo_installs {
            println!("  - {} {}", name, format!("[from {}]", source).dimmed());
        }
        println!(
            "  {}",
            "run `zskills skill upgrade <name>` or `zskills doctor --fix` to re-install slim"
                .dimmed()
        );
    }

    if issues == 0 {
        println!(
            "{} All good — disk, inventory, and settings are in sync.",
            "✓".green()
        );
        return Ok(());
    }

    if fix {
        // Count what we actually repaired, not what we noticed. A fix summary that
        // reports the issue count is the same class of lie as "All good".
        let mut fixed = 0usize;

        // Marketplaces: write the timestamp. Never drop the tap — the user asked for it,
        // and a missing field is our bug to repair, not their registration to revoke.
        if !stale_taps.is_empty() {
            let mut known = crate::marketplace::load_known(&known_path)?;
            for name in &stale_taps {
                if crate::marketplace::stamp_last_updated(&mut known, name) {
                    println!("  stamped lastUpdated on marketplace {}", name);
                    fixed += 1;
                }
            }
            crate::marketplace::save_known(&known_path, &known)?;
        }

        // Plugins that a marketplace really offers: finish the install. Removing the
        // enable here would silently undo whatever the user (or `zskills install`) just
        // asked for — the exact failure this check exists to prevent.
        if !fetchable.is_empty() {
            fixed += crate::commands::install::materialize_plugins(&fetchable)?;
        }

        // Plugins nothing offers: the enable is a dangling reference. Drop it.
        // Load *after* materialize_plugins so we build on whatever `claude` just
        // wrote, and skip the write entirely when there is nothing to remove.
        if !dangling.is_empty() {
            let settings_path = crate::paths::settings_json()?;
            let mut settings = crate::settings::load(&settings_path)?;
            let ep = crate::settings::enabled_plugins_mut(&mut settings);
            for k in &dangling {
                ep.remove(k);
                println!("  removed {} from enabledPlugins", k);
                fixed += 1;
            }
            crate::settings::save(&settings_path, &settings)?;
        }

        // Agent skills: drop inventory entries with no bytes.
        let mut inv = crate::agent_skill::load_inventory()?;
        for k in &agent_inventory_missing {
            inv.agent_skills.remove(k);
            println!("  removed {} from Agent Skill inventory", k);
            fixed += 1;
        }
        crate::agent_skill::save_inventory(&inv)?;

        // Full-repo installs: re-run the install from the recorded source,
        // which re-materializes sparsely (delete + slim copy).
        for (name, source) in &full_repo_installs {
            match crate::agent_skill::install(source, Some(name)) {
                Ok(_) => {
                    println!("  re-installed {} slim from {}", name, source);
                    fixed += 1;
                }
                Err(e) => println!("  {} could not re-install {}: {}", "✗".red(), name, e),
            }
        }

        let informational = issues.saturating_sub(fixable);
        if fixed >= fixable {
            println!("{} Fixed {} issue(s).", "✓".green(), fixed);
        } else {
            println!(
                "{} Fixed {} of {} fixable issue(s); {} still open — re-run {} for the current state.",
                "!".yellow(),
                fixed,
                fixable,
                fixable - fixed,
                "zskills doctor".bold()
            );
        }
        if informational > 0 {
            println!(
                "  {}",
                format!(
                    "{} further finding(s) need a human — `--fix` has no repair for them.",
                    informational
                )
                .dimmed()
            );
        }
    } else if fixable > 0 {
        println!("\nRun {} to clean up.", "zskills doctor --fix".bold());
    } else {
        println!(
            "\n{}",
            "Nothing here is auto-fixable; see the notes above.".dimmed()
        );
    }

    Ok(())
}

/// Managed agent skills whose installed dir contains a `.git/` directory —
/// the signature of a pre-sparse full-repo install. Returns `(name, source)`
/// pairs, only for entries with a git-fetchable source (npm installs and
/// local-only skills are never full-repo copies).
fn find_full_repo_installs(inv: &crate::agent_skill::Inventory) -> Result<Vec<(String, String)>> {
    let skills_dir = crate::paths::user_skills_dir()?;
    let mut out = Vec::new();
    for (name, entry) in &inv.agent_skills {
        if entry.source.starts_with("npm:") {
            continue;
        }
        if skills_dir.join(name).join(".git").exists() {
            out.push((name.clone(), entry.source.clone()));
        }
    }
    Ok(out)
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

/// Compare [[mcps]] intent with runtime Manual keys. `--fix` stays a no-op.
fn check_mcp_intent() -> usize {
    let Some(path) = crate::manifest::discover() else {
        return 0;
    };
    let Ok(manifest) = crate::manifest::load(&path) else {
        return 0;
    };
    let runtime = crate::mcp::load_all().unwrap_or_default();
    let mut issues = 0;
    for entry in &manifest.mcps {
        let Ok(scope) = entry.scope_kind() else {
            continue;
        };
        let present = runtime.iter().any(|m| {
            m.name == entry.name
                && m.scope.label() == scope
                && matches!(m.source, crate::mcp::Source::Manual)
        });
        if !present {
            issues += 1;
            println!(
                "{} [[mcps]] `{}` ({}) is in the manifest but not in the runtime map",
                "✗".red(),
                entry.name,
                scope
            );
        }
    }
    for m in runtime
        .iter()
        .filter(|m| matches!(m.source, crate::mcp::Source::Manual))
    {
        let in_manifest = manifest
            .mcps
            .iter()
            .any(|e| e.name == m.name && e.scope_kind().ok() == Some(m.scope.label()));
        if !in_manifest {
            issues += 1;
            println!(
                "{} MCP `{}` ({}) is in the runtime map but not in [[mcps]]",
                "✗".red(),
                m.name,
                m.scope.label()
            );
        }
    }
    issues
}

/// Warn if a SKILL.md named zskills still teaches bare verbs.
fn check_stale_zskills_skill_md() -> usize {
    let mut issues = 0;
    let candidates = [
        crate::paths::user_skills_dir()
            .ok()
            .map(|p| p.join("zskills").join("SKILL.md")),
        crate::paths::claude_home()
            .ok()
            .map(|p| p.join("skills").join("zskills").join("SKILL.md")),
    ];
    for path in candidates.into_iter().flatten() {
        if !path.exists() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let stale = [
            "zskills install ",
            "zskills remove ",
            "zskills purge ",
            "zskills enable ",
            "zskills disable ",
        ];
        if stale.iter().any(|s| text.contains(s)) {
            issues += 1;
            println!(
                "{} {} still documents bare verbs removed in 1.0 — recopy skills/zskills/SKILL.md from this version",
                "!".yellow(),
                path.display()
            );
        }
    }
    issues
}
