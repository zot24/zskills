//! `sync` — apply a declarative skills.toml manifest. The headline command.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

pub fn run(
    file: Option<PathBuf>,
    dry_run: bool,
    prune: bool,
    adopt: bool,
    force: bool,
) -> Result<()> {
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
    let extra = crate::marketplace::extra_known();
    let declared_mp: BTreeSet<String> = manifest
        .marketplaces
        .iter()
        .map(|m| m.name.clone())
        .collect();

    // Register declared-but-unknown marketplaces before resolving [[skills]].
    // A fresh machine has no known_marketplaces.json; without this step every
    // name@marketplace enable would be written against a tap that does not exist.
    let to_register: Vec<&crate::manifest::MarketplaceEntry> = manifest
        .marketplaces
        .iter()
        .filter(|m| !crate::marketplace::is_registered(&known, &extra, &m.name))
        .collect();
    let registerable: BTreeSet<String> = to_register
        .iter()
        .filter(|m| m.source_spec().is_some())
        .map(|m| m.name.clone())
        .collect();

    let mut names: BTreeSet<String> = known.keys().cloned().collect();
    names.extend(extra.keys().cloned());
    let marketplaces_to_adopt: Vec<crate::manifest::MarketplaceEntry> = names
        .iter()
        .filter(|n| !declared_mp.contains(*n))
        .filter_map(|n| {
            let entry = known.get(n).or_else(|| extra.get(n))?;
            let (repo, url) = crate::marketplace::source_for_manifest(entry)?;
            Some(crate::manifest::MarketplaceEntry {
                name: n.clone(),
                repo,
                url,
                pin: None,
            })
        })
        .collect();
    // Pin-only rows already in the manifest: fill `repo`/`url` so a later
    // fresh-machine sync can clone. Leave `pin` alone. Skip rows that already
    // declare a source.
    let marketplaces_to_fill: Vec<crate::manifest::MarketplaceEntry> = manifest
        .marketplaces
        .iter()
        .filter(|m| m.source_spec().is_none())
        .filter_map(|m| {
            let entry = known.get(&m.name).or_else(|| extra.get(&m.name))?;
            let (repo, url) = crate::marketplace::source_for_manifest(entry)?;
            Some(crate::manifest::MarketplaceEntry {
                name: m.name.clone(),
                repo,
                url,
                pin: None,
            })
        })
        .collect();

    let mut desired_plugins: BTreeSet<String> = BTreeSet::new();
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    let mut pending_unqualified: Vec<&crate::manifest::SkillEntry> = Vec::new();
    let mut plugin_copies: Vec<(String, Vec<crate::harness::Harness>)> = Vec::new();
    for entry in &manifest.skills {
        let qualified = match entry.qualified() {
            Some(q) => q,
            None => match crate::marketplace::resolve_spec(&entry.name, &known) {
                Ok(q) => q,
                Err(e) => {
                    // A name-only row cannot resolve against an empty known map.
                    // If we are about to register a marketplace, retry after clone.
                    if !registerable.is_empty() {
                        pending_unqualified.push(entry);
                    } else {
                        eprintln!("{} {}: {}", "✗".red(), entry.name, e);
                    }
                    continue;
                }
            },
        };
        if let Some((_, mp)) = qualified.rsplit_once('@') {
            let registered =
                crate::marketplace::is_registered(&known, &extra, mp) || registerable.contains(mp);
            if !registered {
                unresolved.insert(qualified);
                continue;
            }
        }
        let targets = match crate::harness::resolve(
            &[],
            &manifest.defaults.harnesses,
            &entry.harnesses,
            crate::harness::default_plugin(),
        ) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{} {}: {}", "✗".red(), entry.name, e);
                continue;
            }
        };
        if targets.contains(&crate::harness::Harness::Claude) {
            desired_plugins.insert(qualified.clone());
        }
        if targets
            .iter()
            .any(|h| h.needs_hub_copy() || h.skill_skip_reason().is_some())
        {
            plugin_copies.push((qualified, targets));
        }
    }

    let settings_path = crate::paths::settings_json()?;
    let settings = crate::settings::load(&settings_path)?;
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
    let plugins_to_disable: Vec<_> = current_plugins
        .difference(&desired_plugins)
        .filter(|k| !unresolved.contains(*k))
        .collect();
    let skip_plugin_diff = desired_plugins.is_empty()
        && pending_unqualified.is_empty()
        && !current_plugins.is_empty()
        && !force;
    if skip_plugin_diff {
        eprintln!(
            "{} skipping plugin reconcile: manifest has no [[skills]] ({} enabled); pass --force to disable extras",
            "!".yellow(),
            current_plugins.len()
        );
    }

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
        && mcps_to_remove.is_empty()
        && plugin_copies.is_empty()
        && to_register.is_empty()
        && unresolved.is_empty()
        && pending_unqualified.is_empty()
        && !(adopt && (!marketplaces_to_adopt.is_empty() || !marketplaces_to_fill.is_empty()));
    if nothing {
        println!("  (no changes — manifest matches current state)");
        return Ok(());
    }

    for m in &to_register {
        if let Some(spec) = m.source_spec() {
            println!(
                "  {} register marketplace  {} {}",
                "+".green(),
                m.name,
                format!("({spec})").dimmed()
            );
        } else {
            println!(
                "  {} skip    marketplace  {} {}",
                "✗".red(),
                m.name,
                "(not registered, no repo or url)".dimmed()
            );
        }
    }
    if adopt {
        for m in &marketplaces_to_adopt {
            println!(
                "  {} adopt   marketplace  {} {}",
                "+".cyan(),
                m.name,
                "(registered but not in manifest — adding)".dimmed()
            );
        }
        for m in &marketplaces_to_fill {
            println!(
                "  {} adopt   marketplace  {} {}",
                "+".cyan(),
                m.name,
                "(registered but no repo or url — filling)".dimmed()
            );
        }
    }
    for entry in &pending_unqualified {
        println!(
            "  {} resolve plugin  {} {}",
            "~".cyan(),
            entry.name,
            "(after marketplace register)".dimmed()
        );
    }
    for q in &unresolved {
        println!(
            "  {} skip    plugin  {} {}",
            "✗".red(),
            q,
            "(marketplace is not registered)".dimmed()
        );
    }
    for k in &plugins_to_enable {
        println!("  {} enable  plugin  {}", "+".green(), k);
    }
    for (q, hs) in &plugin_copies {
        let dests = hs
            .iter()
            .filter(|h| h.needs_hub_copy())
            .map(|h| h.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if !dests.is_empty() {
            println!("  {} copy    plugin  {} → hub ({})", "~".cyan(), q, dests);
        }
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
        for m in &marketplaces_to_adopt {
            if crate::manifest::append_marketplace(&path, m)? {
                adopted += 1;
            }
        }
        for m in &marketplaces_to_fill {
            if crate::manifest::fill_marketplace_source(
                &path,
                &m.name,
                m.repo.as_deref(),
                m.url.as_deref(),
            )? {
                adopted += 1;
            }
        }
        for k in &plugins_to_disable {
            let (name, mp) = k
                .split_once('@')
                .map(|(n, m)| (n.to_string(), Some(m.to_string())))
                .unwrap_or_else(|| ((*k).clone(), None));
            let entry = crate::manifest::SkillEntry {
                name,
                marketplace: mp,
                version: None,
                harnesses: Vec::new(),
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
    let mut register_failed = 0usize;
    for m in &to_register {
        let Some(spec) = m.source_spec() else {
            continue;
        };
        match crate::marketplace::register(&m.name, spec) {
            Ok(_) => println!("  {} registered marketplace {}", "✓".green(), m.name.bold()),
            Err(e) => {
                eprintln!("{} marketplace `{}`: {e}", "✗".red(), m.name);
                register_failed += 1;
            }
        }
    }
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    let extra = crate::marketplace::extra_known();
    let mut late_enables: Vec<String> = Vec::new();
    for entry in &pending_unqualified {
        match crate::marketplace::resolve_spec(&entry.name, &known) {
            Ok(q) => {
                if let Some((_, mp)) = q.rsplit_once('@') {
                    if !crate::marketplace::is_registered(&known, &extra, mp) {
                        unresolved.insert(q);
                        continue;
                    }
                }
                let targets = match crate::harness::resolve(
                    &[],
                    &manifest.defaults.harnesses,
                    &entry.harnesses,
                    crate::harness::default_plugin(),
                ) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("{} {}: {}", "✗".red(), entry.name, e);
                        continue;
                    }
                };
                if targets.contains(&crate::harness::Harness::Claude)
                    && !current_plugins.contains(&q)
                {
                    late_enables.push(q.clone());
                }
                if targets
                    .iter()
                    .any(|h| h.needs_hub_copy() || h.skill_skip_reason().is_some())
                {
                    plugin_copies.push((q, targets));
                }
            }
            Err(e) => eprintln!("{} {}: {}", "✗".red(), entry.name, e),
        }
    }
    for q in &desired_plugins {
        let Some((_, mp)) = q.rsplit_once('@') else {
            continue;
        };
        if !crate::marketplace::is_registered(&known, &extra, mp) {
            unresolved.insert(q.clone());
        }
    }
    for (q, _) in &plugin_copies {
        let Some((_, mp)) = q.rsplit_once('@') else {
            continue;
        };
        if !crate::marketplace::is_registered(&known, &extra, mp) {
            unresolved.insert(q.clone());
        }
    }

    // register() wrote extraKnownMarketplaces. Re-read so the enable pass
    // cannot overwrite that with the pre-register snapshot.
    let mut settings = crate::settings::load(&settings_path)?;
    if !skip_plugin_diff {
        let ep = crate::settings::enabled_plugins_mut(&mut settings);
        for k in &plugins_to_enable {
            if unresolved.contains(*k) {
                continue;
            }
            ep.insert((*k).clone(), Value::Bool(true));
        }
        for k in &late_enables {
            if unresolved.contains(k) {
                continue;
            }
            ep.insert(k.clone(), Value::Bool(true));
        }
        for k in &plugins_to_disable {
            ep.insert((*k).clone(), Value::Bool(false));
        }
        crate::settings::save(&settings_path, &settings)?;
    }

    for (q, hs) in &plugin_copies {
        if unresolved.contains(q) {
            continue;
        }
        match crate::harness::materialize_hub(q, hs, crate::harness::DEFAULT_HERMES_CATEGORY) {
            Ok(names) => {
                if !names.is_empty() {
                    println!(
                        "  {} {}  copied {} nested skill(s) into the Agent Skill hub",
                        "✓".green(),
                        q,
                        names.len()
                    );
                }
            }
            Err(e) => eprintln!("{} {q}: {e}", "✗".red()),
        }
    }

    for entry in &manifest.agent_skills {
        let hs = crate::harness::resolve(
            &[],
            &manifest.defaults.harnesses,
            &entry.harnesses,
            crate::harness::default_skill(),
        )?;
        crate::harness::ensure_pi_hub_if_targeted(&hs)?;
        if let Some(pkg) = entry.npm.as_deref() {
            match crate::agent_skill::install_npm(pkg, entry.install_cmd.as_deref(), &entry.claims)
            {
                Ok(names) => {
                    crate::harness::link_hub_to_harnesses(
                        &names,
                        &hs,
                        crate::harness::DEFAULT_HERMES_CATEGORY,
                    )?;
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
                        crate::harness::link_hub_to_harnesses(
                            &[n.to_string()],
                            &hs,
                            crate::harness::DEFAULT_HERMES_CATEGORY,
                        )?;
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
                        crate::harness::link_hub_to_harnesses(
                            &names,
                            &hs,
                            crate::harness::DEFAULT_HERMES_CATEGORY,
                        )?;
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
                //
                // The disk check is the point. Tracking a name that is not there writes an
                // inventory entry `doctor` immediately reports as "tracked in inventory but
                // missing on disk" — sync would manufacture the very defect doctor exists to
                // find. A typo in the manifest must not do that.
                let on_disk = crate::agent_skill::installed_on_disk().unwrap_or_default();
                let mut inv = crate::agent_skill::load_inventory()?;

                // A local entry may also `claims` globs, the same way an npm entry does.
                // Without this, `claims` on a local entry is silently ignored.
                let mut targets: Vec<String> = Vec::new();
                if on_disk.iter().any(|n| n == name) {
                    targets.push(name.to_string());
                } else if entry.claims.is_empty() {
                    println!(
                        "  {} {} {}",
                        "·".dimmed(),
                        name,
                        "(declared local, not on disk — nothing to track)".dimmed()
                    );
                }
                for pattern in &entry.claims {
                    let Ok(pat) = glob::Pattern::new(pattern) else {
                        eprintln!("{} invalid claims pattern {:?}", "✗".red(), pattern);
                        continue;
                    };
                    for n in on_disk.iter().filter(|n| pat.matches(n)) {
                        if !targets.contains(n) {
                            targets.push(n.clone());
                        }
                    }
                }

                let mut dirty = false;
                for n in &targets {
                    if inv.agent_skills.contains_key(n) {
                        continue;
                    }
                    inv.agent_skills.insert(
                        n.to_string(),
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
                            to: vec!["agents".into()],
                        },
                    );
                    dirty = true;
                    println!("  tracked local agent skill {}", n.bold());
                }
                if dirty {
                    crate::agent_skill::save_inventory(&inv)?;
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
            match crate::mcp::remove(scope, name) {
                Ok(true) => println!("  removed mcp {} ({})", name.bold(), scope.label()),
                Ok(false) => eprintln!(
                    "{} mcp `{}` ({}) not found in the attributed file",
                    "✗".red(),
                    name,
                    scope.label()
                ),
                Err(e) => eprintln!("{} mcp `{}`: {}", "✗".red(), name, e),
            }
        }
    }

    if !unresolved.is_empty() {
        eprintln!(
            "{} refused to write enabledPlugins for {} plugin(s) whose marketplace is not registered:",
            "✗".red(),
            unresolved.len()
        );
        for q in &unresolved {
            eprintln!("  {q}");
        }
        eprintln!(
            "  add `repo` or `url` on [[marketplaces]] and re-run sync, or: zskills marketplace add <owner/repo>"
        );
    }
    if register_failed > 0 {
        anyhow::bail!(
            "failed to register {register_failed} marketplace(s) declared in the manifest"
        );
    }
    if !unresolved.is_empty() {
        anyhow::bail!("marketplace is not registered — not writing dangling enabledPlugins keys");
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
