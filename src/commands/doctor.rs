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

    issues += check_claude_flavored_hub();
    issues += check_agent_skill_path_and_marketplace(&known);

    let harness_findings = scan_harness_skill_roots();
    issues += harness_findings.len();
    fixable += harness_findings.iter().filter(|f| f.fixable()).count();
    for f in &harness_findings {
        f.print();
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

        for f in &harness_findings {
            if f.repair() {
                fixed += 1;
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

/// Bare verbs removed in 1.0, when each one moved under a typed group
/// (`zskills plugin install`, `zskills skill install`, `zskills mcp add`). The
/// trailing space stops `zskills plugin install ` from matching `zskills install `.
///
/// `check_stale_zskills_skill_md` reads this list at run time. The test
/// `shipped_skill_md_teaches_no_removed_verb` reads the same list against the
/// `SKILL.md` this repo ships, so the checker and the guard cannot drift apart.
pub(crate) const REMOVED_BARE_VERBS: &[&str] = &[
    "zskills install ",
    "zskills remove ",
    "zskills purge ",
    "zskills enable ",
    "zskills disable ",
    "zskills upgrade ",
];

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
        if REMOVED_BARE_VERBS.iter().any(|s| text.contains(s)) {
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

/// Frontmatter phrase on Claude `wiki-manager`. OpenCode rewrites this.
const CLAUDE_WIKI_ACTIVATION: &str = "Activates for /wiki commands";
/// Body sentence on Claude `wiki-manager`. OpenCode does not keep it.
const CLAUDE_COMPILER_SENTENCE: &str = "Claude Code is both the compiler";
/// Claude Code tool names cited as a flavour signal. Matching the trio
/// avoids treating a single "Read" token as Claude flavour.
const CLAUDE_TOOL_TRIO: &[&str] = &["Read", "Write", "Edit"];

/// Warn when Pi or Grok can see hub `wiki-manager` and SKILL.md is Claude-flavored.
///
/// Do **not** substring `/wiki:` — OpenCode SKILL.md documents `/wiki:*` as
/// shorthand, so that match would false-positive after a successful path copy.
fn check_claude_flavored_hub() -> usize {
    let Ok(hub) = crate::paths::user_skills_dir() else {
        return 0;
    };
    let path = hub.join("wiki-manager").join("SKILL.md");
    if !path.is_file() {
        return 0;
    }
    let vis = crate::harness::visibility_for_skill("wiki-manager");
    let targets: Vec<&str> = vis
        .visible
        .iter()
        .filter(|h| {
            matches!(
                h,
                crate::harness::Harness::Pi | crate::harness::Harness::Grok
            )
        })
        .map(|h| h.as_str())
        .collect();
    if targets.is_empty() {
        return 0;
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return 0;
    };
    if !is_claude_flavored(&text) {
        return 0;
    }
    println!(
        "{} hub Agent Skill `wiki-manager` is Claude-flavored and visible to {}",
        "!".yellow(),
        targets.join(", ")
    );
    println!(
        "  {}",
        "Pi and Grok need the OpenCode tree. Add [[agent_skills]] marketplace + path, then sync."
            .dimmed()
    );
    1
}

/// YAML frontmatter between leading `---` fences, if any.
fn yaml_frontmatter(text: &str) -> Option<&str> {
    let t = text.trim_start_matches('\u{feff}');
    let t = t.strip_prefix("---")?;
    let t = t.strip_prefix("\r\n").or_else(|| t.strip_prefix('\n'))?;
    let idx = t.find("\n---")?;
    Some(&t[..idx])
}

/// Body after the closing `---` fence. Whole text when there is no frontmatter.
fn markdown_body(text: &str) -> &str {
    let t = text.trim_start_matches('\u{feff}');
    let Some(rest) = t.strip_prefix("---") else {
        return text;
    };
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'));
    let Some(rest) = rest else {
        return text;
    };
    match rest.find("\n---") {
        Some(idx) => {
            let after = &rest[idx + 4..];
            after
                .strip_prefix("\r\n")
                .or_else(|| after.strip_prefix('\n'))
                .unwrap_or(after)
        }
        None => text,
    }
}

/// Claude flavour: activation phrase or `tools:` trio in frontmatter, or the
/// compiler sentence in the body. `/wiki:` alone is not a signal.
fn is_claude_flavored(text: &str) -> bool {
    if let Some(fm) = yaml_frontmatter(text) {
        if fm.contains(CLAUDE_WIKI_ACTIVATION) {
            return true;
        }
        if frontmatter_has_claude_tools(fm) {
            return true;
        }
    }
    markdown_body(text).contains(CLAUDE_COMPILER_SENTENCE)
}

fn frontmatter_has_claude_tools(fm: &str) -> bool {
    let lines: Vec<&str> = fm.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if let Some(rest) = trimmed.strip_prefix("tools:") {
            let mut blob = rest.trim().to_string();
            if blob.is_empty() {
                i += 1;
                while i < lines.len() {
                    let t = lines[i].trim();
                    if let Some(item) = t.strip_prefix('-') {
                        blob.push(' ');
                        blob.push_str(item.trim());
                        i += 1;
                    } else if t.is_empty() {
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            return claude_tool_trio(&blob);
        }
        i += 1;
    }
    false
}

fn claude_tool_trio(blob: &str) -> bool {
    let tokens: std::collections::BTreeSet<&str> = blob
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    CLAUDE_TOOL_TRIO.iter().all(|t| tokens.contains(t))
}

/// Dangling `path` on disk, and a `marketplace` name missing from
/// `known_marketplaces.json`. Neither is auto-fixable.
fn check_agent_skill_path_and_marketplace(
    known: &serde_json::Map<String, serde_json::Value>,
) -> usize {
    let Some(path) = crate::manifest::discover() else {
        return 0;
    };
    let Ok(manifest) = crate::manifest::load(&path) else {
        return 0;
    };
    let mut issues = 0;
    let mut warned_mp = std::collections::BTreeSet::new();
    let mut warned_path = std::collections::BTreeSet::new();
    for entry in &manifest.agent_skills {
        if let Some(mp) = &entry.marketplace {
            if !known.contains_key(mp) {
                if warned_mp.insert(mp.clone()) {
                    issues += 1;
                    println!(
                        "{} [[agent_skills]] marketplace `{}` is not in known_marketplaces.json",
                        "✗".red(),
                        mp
                    );
                    println!(
                        "  {}",
                        "register it with `zskills marketplace add`, or set repo = on [[marketplaces]] and run sync"
                            .dimmed()
                    );
                }
                continue;
            }
        }
        let Some(rel) = &entry.path else {
            continue;
        };
        let Some(clone) = clone_dir_for(entry) else {
            continue;
        };
        if crate::agent_skill::resolve_path_in_clone(&clone, rel).is_ok() {
            continue;
        }
        let key = match &entry.marketplace {
            Some(mp) => format!("marketplace:{mp}:{rel}"),
            None => format!("source:{}:{rel}", entry.source.as_deref().unwrap_or("")),
        };
        if !warned_path.insert(key) {
            continue;
        }
        issues += 1;
        let origin = match &entry.marketplace {
            Some(mp) => format!("marketplace `{mp}`"),
            None => match &entry.source {
                Some(src) => format!("source `{src}`"),
                None => "the clone".to_string(),
            },
        };
        println!(
            "{} [[agent_skills]] path `{}` is not on disk in {}",
            "✗".red(),
            rel,
            origin
        );
    }
    issues
}

fn clone_dir_for(entry: &crate::manifest::AgentSkillEntry) -> Option<std::path::PathBuf> {
    if let Some(mp) = entry.marketplace.as_deref() {
        let dir = crate::paths::marketplaces_dir().ok()?.join(mp);
        return dir.is_dir().then_some(dir);
    }
    let src = entry.source.as_deref()?;
    let (_, name) = crate::agent_skill::parse_source(src).ok()?;
    let cache = crate::paths::agent_skills_cache_dir().ok()?.join(name);
    cache.is_dir().then_some(cache)
}

enum HarnessRootKind {
    Dangling { target: std::path::PathBuf },
    Malformed,
    Shadow { newer: std::path::PathBuf },
    Fossil,
}

struct HarnessRootFinding {
    path: std::path::PathBuf,
    kind: HarnessRootKind,
}

impl HarnessRootFinding {
    fn fixable(&self) -> bool {
        matches!(
            self.kind,
            HarnessRootKind::Dangling { .. } | HarnessRootKind::Fossil
        )
    }

    fn print(&self) {
        match &self.kind {
            HarnessRootKind::Dangling { target } => {
                println!(
                    "{} dangling symlink {} → {}",
                    "✗".red(),
                    self.path.display(),
                    target.display()
                );
            }
            HarnessRootKind::Malformed => {
                println!(
                    "{} malformed Agent Skill {}",
                    "✗".red(),
                    self.path.display()
                );
            }
            HarnessRootKind::Shadow { newer } => {
                println!(
                    "{} {} shadows the hub copy (newer: {})",
                    "✗".red(),
                    self.path.display(),
                    newer.display()
                );
            }
            HarnessRootKind::Fossil => {
                println!("{} fossil inventory {}", "✗".red(), self.path.display());
            }
        }
    }

    fn repair(&self) -> bool {
        match self.kind {
            HarnessRootKind::Dangling { .. } => {
                let Ok(meta) = std::fs::symlink_metadata(&self.path) else {
                    return false;
                };
                if !meta.file_type().is_symlink() {
                    return false;
                }
                if std::fs::remove_file(&self.path).is_err() {
                    return false;
                }
                println!("  unlinked dangling symlink {}", self.path.display());
                true
            }
            HarnessRootKind::Fossil => {
                if self.path.file_name() != Some(std::ffi::OsStr::new(".zskills.json")) {
                    return false;
                }
                let Ok(meta) = std::fs::symlink_metadata(&self.path) else {
                    return false;
                };
                if meta.file_type().is_dir() {
                    return false;
                }
                if std::fs::remove_file(&self.path).is_err() {
                    return false;
                }
                println!("  removed fossil inventory {}", self.path.display());
                true
            }
            _ => false,
        }
    }
}

fn scan_harness_skill_roots() -> Vec<HarnessRootFinding> {
    let Ok(hub) = crate::paths::user_skills_dir() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for h in [
        crate::harness::Harness::Claude,
        crate::harness::Harness::Pi,
        crate::harness::Harness::Hermes,
        crate::harness::Harness::Kimi,
        crate::harness::Harness::Grok,
        crate::harness::Harness::Codex,
    ] {
        let roots = match h.skill_roots() {
            Ok(Some(r)) => r,
            _ => continue,
        };
        for root in roots {
            if !root.is_dir() {
                continue;
            }
            if root == hub {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                if let Some(finding) = classify_harness_entry(&entry.path(), &hub) {
                    out.push(finding);
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn classify_harness_entry(
    path: &std::path::Path,
    hub: &std::path::Path,
) -> Option<HarnessRootFinding> {
    let name = path.file_name()?;
    if name == ".zskills.json" {
        let meta = std::fs::symlink_metadata(path).ok()?;
        if meta.file_type().is_dir() {
            return None;
        }
        return Some(HarnessRootFinding {
            path: path.to_path_buf(),
            kind: HarnessRootKind::Fossil,
        });
    }
    if name.to_str()?.starts_with('.') {
        return None;
    }

    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(path).ok()?;
        if std::fs::metadata(path).is_err() {
            return Some(HarnessRootFinding {
                path: path.to_path_buf(),
                kind: HarnessRootKind::Dangling { target },
            });
        }
        return None;
    }
    if !meta.file_type().is_dir() {
        return None;
    }

    let skill_md = path.join("SKILL.md");
    let text = match std::fs::read_to_string(&skill_md) {
        Ok(t) => t,
        Err(_) => {
            return Some(HarnessRootFinding {
                path: path.to_path_buf(),
                kind: HarnessRootKind::Malformed,
            });
        }
    };
    if !skill_md_has_name(&text) {
        return Some(HarnessRootFinding {
            path: path.to_path_buf(),
            kind: HarnessRootKind::Malformed,
        });
    }

    let hub_copy = hub.join(name);
    if hub_copy == *path {
        return None;
    }
    let hub_md = hub_copy.join("SKILL.md");
    let Ok(hub_bytes) = std::fs::read(&hub_md) else {
        return None;
    };
    if hub_bytes == text.as_bytes() {
        return None;
    }
    let newer = if copy_mtime(path) >= copy_mtime(&hub_copy) {
        path.to_path_buf()
    } else {
        hub_copy
    };
    Some(HarnessRootFinding {
        path: path.to_path_buf(),
        kind: HarnessRootKind::Shadow { newer },
    })
}

fn skill_md_has_name(text: &str) -> bool {
    let Some(fm) = yaml_frontmatter(text) else {
        return false;
    };
    fm.lines().any(|line| {
        line.trim_start()
            .strip_prefix("name:")
            .is_some_and(|v| !v.trim().trim_matches(['"', '\'']).is_empty())
    })
}

fn copy_mtime(dir: &std::path::Path) -> std::time::SystemTime {
    let dir_t = std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let md_t = std::fs::metadata(dir.join("SKILL.md"))
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    dir_t.max(md_t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `doctor` tells a user to recopy `skills/zskills/SKILL.md` "from this
    /// version". That advice is circular while the shipped file is itself stale,
    /// which is what happened between 1.0 and 1.2.0: the file taught 36 removed
    /// verbs and every `doctor` run reported the copy the user had just made.
    ///
    /// Guard the shipped file with the checker's own list.
    #[test]
    fn shipped_skill_md_teaches_no_removed_verb() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("skills")
            .join("zskills")
            .join("SKILL.md");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        let mut found: Vec<String> = Vec::new();
        for (n, line) in text.lines().enumerate() {
            for verb in REMOVED_BARE_VERBS {
                if line.contains(verb) {
                    found.push(format!("  {}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }

        assert!(
            found.is_empty(),
            "the shipped SKILL.md teaches {} verb(s) removed in 1.0.\n\
             Each verb moved under a typed group: `zskills plugin install`, \
             `zskills skill install`, `zskills mcp add`.\n{}",
            found.len(),
            found.join("\n")
        );
    }

    /// The trailing space in each pattern is load-bearing. Without it every
    /// typed form would match its own removed verb and the guard would fire on
    /// correct documentation.
    #[test]
    fn typed_forms_do_not_match_removed_verbs() {
        for typed in [
            "zskills plugin install foo",
            "zskills plugin remove foo",
            "zskills plugin purge foo",
            "zskills plugin enable foo",
            "zskills plugin disable foo",
            "zskills skill install owner/repo",
            "zskills skill remove foo",
            "zskills skill upgrade foo",
        ] {
            assert!(
                !REMOVED_BARE_VERBS.iter().any(|v| typed.contains(v)),
                "typed form `{typed}` must not match a removed bare verb"
            );
        }
    }

    /// Every removed verb is still detected in its bare form.
    #[test]
    fn removed_verbs_are_detected_in_bare_form() {
        for verb in REMOVED_BARE_VERBS {
            let line = format!("Run `{verb}foo` to do the thing.");
            assert!(
                REMOVED_BARE_VERBS.iter().any(|v| line.contains(v)),
                "bare form of `{verb}` must be detected"
            );
        }
    }

    const CLAUDE_WIKI: &str = "---\nname: wiki-manager\ndescription: Activates for /wiki commands\ntools: Read, Write, Edit\n---\nClaude Code is both the compiler and the runtime.\n";
    const OPENCODE_WIKI: &str = "---\nname: wiki-manager\ndescription: Manage a local wiki\n---\nOpenCode Integration Notes.\n\nTreat any /wiki:* references in this document as shorthand for the matching skill action.\n";

    #[test]
    fn claude_wiki_skill_is_flavored() {
        assert!(is_claude_flavored(CLAUDE_WIKI));
    }

    #[test]
    fn opencode_wiki_shorthand_is_not_flavored() {
        assert!(
            OPENCODE_WIKI.contains("/wiki:"),
            "negative case must contain the shorthand that a `/wiki:` substring would trip"
        );
        assert!(!is_claude_flavored(OPENCODE_WIKI));
    }

    #[test]
    fn activation_phrase_in_frontmatter_is_flavored() {
        assert!(is_claude_flavored(
            "---\ndescription: Activates for /wiki commands. Use when querying.\n---\nbody\n"
        ));
    }

    #[test]
    fn tools_trio_in_frontmatter_is_flavored() {
        assert!(is_claude_flavored(
            "---\nname: wiki-manager\ntools: Read, Write, Edit, Bash\n---\nbody\n"
        ));
        assert!(is_claude_flavored(
            "---\ntools:\n  - Read\n  - Write\n  - Edit\n---\nbody\n"
        ));
    }

    #[test]
    fn compiler_sentence_in_body_is_flavored() {
        assert!(is_claude_flavored(
            "---\nname: wiki-manager\n---\nClaude Code is both the compiler and the runtime.\n"
        ));
    }

    #[test]
    fn wiki_slash_substring_alone_is_not_flavored() {
        let text = "---\nname: wiki-manager\n---\nTreat any /wiki:* references as shorthand.\n";
        assert!(text.contains("/wiki:"));
        assert!(!is_claude_flavored(text));
    }

    #[test]
    fn tools_in_body_are_not_flavored() {
        assert!(!is_claude_flavored(
            "---\nname: wiki-manager\n---\nUse tools: Read, Write, Edit\n"
        ));
    }

    #[test]
    fn activation_phrase_in_body_is_not_flavored() {
        assert!(!is_claude_flavored(
            "---\nname: wiki-manager\n---\nActivates for /wiki commands\n"
        ));
    }

    #[test]
    fn single_claude_tool_is_not_the_trio() {
        assert!(!is_claude_flavored(
            "---\nname: wiki-manager\ntools: Read\n---\nbody\n"
        ));
    }
}
