use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::json;
use std::collections::BTreeMap;

pub fn run(json_out: bool, verbose: bool, paths: bool) -> Result<()> {
    let report = crate::reconcile::run()?;
    let inv = crate::agent_skill::load_inventory()?;
    let on_disk = crate::agent_skill::installed_on_disk().unwrap_or_default();
    let mcps = crate::mcp::load_all().unwrap_or_default();
    // Load plugin inventory for installPath lookups when --paths is on.
    let plugin_inv: serde_json::Value = if paths {
        let p = crate::paths::installed_plugins_json()?;
        if p.exists() {
            serde_json::from_slice(&std::fs::read(&p)?).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        }
    } else {
        serde_json::json!({})
    };
    let user_skills = crate::paths::user_skills_dir().ok();

    let managed_names: Vec<&String> = inv.agent_skills.keys().collect();
    let untracked: Vec<String> = on_disk
        .iter()
        .filter(|n| !inv.agent_skills.contains_key(n.as_str()))
        .cloned()
        .collect();

    // Group managed agent skills by their `source` field.
    let mut by_source: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in &managed_names {
        let src = inv
            .agent_skills
            .get(name.as_str())
            .map(|e| e.source.as_str())
            .unwrap_or("?")
            .to_string();
        by_source.entry(src).or_default().push((*name).clone());
    }
    for v in by_source.values_mut() {
        v.sort();
    }

    if json_out {
        let groups: Vec<_> = by_source
            .iter()
            .map(|(src, names)| {
                json!({
                    "source": src,
                    "skills": names,
                    "count": names.len(),
                })
            })
            .collect();
        let out = json!({
            "plugins": {
                "active": report.active,
                "installed_disabled": report.installed_disabled,
                "enabled_orphan": report.enabled_orphan,
                "installed_orphan": report.installed_orphan,
            },
            "agent_skills": {
                "managed": groups,
                "untracked": untracked,
            },
            "mcp_servers": mcps,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!(
        "{}",
        "Plugins — active (enabled + installed)".bold().green()
    );
    if report.active.is_empty() {
        println!("  (none)");
    } else {
        for k in &report.active {
            print_plugin_line("✓", k, paths, &plugin_inv);
        }
    }

    if !report.installed_disabled.is_empty() {
        println!("\n{}", "Plugins — installed but disabled".bold().yellow());
        for k in &report.installed_disabled {
            print_plugin_line("•", k, paths, &plugin_inv);
        }
    }

    if !report.enabled_orphan.is_empty() {
        println!(
            "\n{}",
            "Plugins — enabled but NOT installed (broken)".bold().red()
        );
        for k in &report.enabled_orphan {
            println!("  ✗ {}", k);
        }
    }

    if !report.installed_orphan.is_empty() {
        println!(
            "\n{}",
            "Plugins — installed from a marketplace that's no longer registered"
                .bold()
                .red()
        );
        for k in &report.installed_orphan {
            println!("  ✗ {}", k);
        }
    }

    println!("\n{}", "Agent Skills — managed by zskills".bold().green());
    if by_source.is_empty() {
        println!("  (none)");
    } else {
        for (src, names) in &by_source {
            print_group(src, names, verbose, paths, user_skills.as_deref());
        }
    }

    print_mcp_section(&mcps, paths);

    if !untracked.is_empty() {
        println!(
            "\n{}",
            "Agent Skills — on disk but not managed by zskills"
                .bold()
                .yellow()
        );
        // Hint when many untracked skills share a common prefix — likely one package.
        if let Some((prefix, count)) = common_prefix_summary(&untracked) {
            if count >= 5 {
                println!(
                    "  {} {} skill(s) share the prefix '{}' — likely from a single package.",
                    "ℹ".cyan(),
                    count,
                    prefix
                );
            }
        }
        for k in &untracked {
            println!("  • {}", k);
        }
        println!(
            "  {}",
            "(add a [[agent_skills]] entry to your manifest to take ownership)".dimmed()
        );
    }

    Ok(())
}

fn print_group(
    source: &str,
    names: &[String],
    verbose: bool,
    paths: bool,
    user_skills: Option<&std::path::Path>,
) {
    let count = names.len();
    if count == 1 {
        // Single skill from a source: "✓ <skill>  ← <source>  (<path>)"
        let path_suffix = if paths {
            agent_skill_path_suffix(&names[0], user_skills)
        } else {
            String::new()
        };
        println!(
            "  ✓ {}  {}{}",
            names[0],
            format!("← {}", source).dimmed(),
            path_suffix
        );
        return;
    }
    let (label, kind) = match source.split_once(':') {
        Some(("npm", pkg)) => (pkg.to_string(), "npm"),
        _ if source.contains('/') => (source.to_string(), "github"),
        _ => (source.to_string(), source),
    };
    println!(
        "  ✓ {} {}  {}",
        label.bold(),
        format!("({} skills)", count).dimmed(),
        format!("← {}", kind).dimmed()
    );
    if verbose || count <= 5 {
        for n in names {
            let path_suffix = if paths {
                agent_skill_path_suffix(n, user_skills)
            } else {
                String::new()
            };
            println!("      • {}{}", n, path_suffix);
        }
    } else {
        let preview: Vec<&str> = names.iter().take(5).map(|s| s.as_str()).collect();
        println!(
            "      {} {}",
            preview.join(", ").dimmed(),
            format!("… [-v to list all {}]", count).dimmed()
        );
    }
}

fn agent_skill_path_suffix(name: &str, user_skills: Option<&std::path::Path>) -> String {
    match user_skills {
        Some(base) => format!("  {}", base.join(name).display().to_string().dimmed()),
        None => String::new(),
    }
}

fn print_plugin_line(marker: &str, qualified: &str, paths: bool, inv: &serde_json::Value) {
    if !paths {
        println!("  {} {}", marker, qualified);
        return;
    }
    let path = plugin_install_path(qualified, inv).unwrap_or_else(|| "(unknown)".to_string());
    println!("  {} {}  {}", marker, qualified, path.dimmed());
}

fn plugin_install_path(qualified: &str, inv: &serde_json::Value) -> Option<String> {
    let entries = inv.get("plugins")?.get(qualified)?.as_array()?;
    let entry = entries.first()?;
    entry
        .get("installPath")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn print_mcp_section(mcps: &[crate::mcp::McpServer], paths: bool) {
    println!("\n{}", "MCP Servers".bold().green());
    if mcps.is_empty() {
        println!("  (none configured)");
        return;
    }

    let mut by_scope: BTreeMap<u8, (crate::mcp::Scope, Vec<&crate::mcp::McpServer>)> =
        BTreeMap::new();
    for m in mcps {
        by_scope
            .entry(m.scope.precedence())
            .or_insert_with(|| (m.scope.clone(), Vec::new()))
            .1
            .push(m);
    }

    let name_w = mcps.iter().map(|m| m.name.len()).max().unwrap_or(0).max(8);

    for (_, (scope, servers)) in by_scope.iter_mut() {
        servers.sort_by(|a, b| a.name.cmp(&b.name));
        println!(
            "  {} {}",
            scope.label().bold(),
            format!("({})", servers.len()).dimmed()
        );
        for m in servers {
            let attribution = match &m.source {
                crate::mcp::Source::FromPlugin { plugin } => {
                    format!("  ★ plugin:{}", plugin).cyan().to_string()
                }
                crate::mcp::Source::Manual => String::new(),
            };
            let sensitive = match m.transport.sensitive_count() {
                0 => String::new(),
                n => format!("  ({})", pluralize_sensitive(&m.transport, n))
                    .dimmed()
                    .to_string(),
            };
            let deprecated = if m.transport.kind() == "sse" {
                "  [sse: deprecated]".yellow().to_string()
            } else {
                String::new()
            };
            let path_suffix = if paths {
                format!("  {}", m.source_file.display().to_string().dimmed())
            } else {
                String::new()
            };
            println!(
                "    {:name_w$}  {:5}  {}{}{}{}{}",
                m.name,
                m.transport.kind(),
                short(&m.transport.short(), 60),
                attribution,
                sensitive,
                deprecated,
                path_suffix,
                name_w = name_w
            );
        }
    }
}

fn pluralize_sensitive(t: &crate::mcp::Transport, n: usize) -> String {
    let word = match t {
        crate::mcp::Transport::Stdio { .. } => {
            if n == 1 {
                "env"
            } else {
                "envs"
            }
        }
        _ => {
            if n == 1 {
                "header"
            } else {
                "headers"
            }
        }
    };
    format!("{} {}", n, word)
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

/// Find the longest prefix shared by ≥3 entries; report how many share it.
/// Heuristic for the "this looks like one package" hint in the untracked section.
fn common_prefix_summary(names: &[String]) -> Option<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for n in names {
        if let Some((prefix, _)) = n.split_once('-') {
            *counts.entry(format!("{}-", prefix)).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .filter(|(_, c)| *c >= 3)
}
