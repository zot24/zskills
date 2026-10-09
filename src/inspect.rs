//! Optional skillspector gate.
//!
//! `publish` is the hub write for `skill install`, `skill upgrade`, `plugin install`,
//! and `sync`. It stages the bytes that would land, scans that stage, and calls
//! `install_to_root` only after the scan passes. A failed scan deletes the stage
//! and leaves the existing hub directory in place.

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::agent_skill::InspectRecord;
use crate::manifest::{AgentSkillEntry, InspectOverride, Manifest};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOn {
    DoNotInstall,
    Caution,
    Findings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnMissing {
    Error,
    Warn,
}

#[derive(Debug, Clone)]
pub struct Gate {
    pub enabled: bool,
    command: String,
    args: Vec<String>,
    fail_on: FailOn,
    on_missing: OnMissing,
    skip: bool,
    dry_run: bool,
    default_inspect: Option<bool>,
    /// Reviewed overrides from `[[agent_skills]]` rows, keyed by exact skill name.
    overrides: HashMap<String, InspectOverride>,
}

impl Gate {
    #[cfg(test)]
    pub fn inactive() -> Self {
        Self {
            enabled: false,
            command: "skillspector".into(),
            args: Vec::new(),
            fail_on: FailOn::DoNotInstall,
            on_missing: OnMissing::Error,
            skip: false,
            dry_run: false,
            default_inspect: None,
            overrides: HashMap::new(),
        }
    }

    pub fn load(skip: bool, dry_run: bool) -> Result<Self> {
        let manifest = match crate::manifest::discover() {
            Some(path) if path.is_file() => crate::manifest::load(&path)?,
            _ => Manifest::default(),
        };
        Self::from_manifest(&manifest, skip, dry_run)
    }

    pub fn from_manifest(manifest: &Manifest, skip: bool, dry_run: bool) -> Result<Self> {
        let table = &manifest.skillspector;
        let fail_on = parse_fail_on(&table.fail_on)?;
        let on_missing = parse_on_missing(&table.on_missing)?;
        let command = if table.command.trim().is_empty() {
            "skillspector".to_string()
        } else {
            table.command.clone()
        };
        if skip {
            eprintln!(
                "{} --skip-inspect: skillspector did not scan. No passing scan was recorded.",
                "!".yellow()
            );
        }
        Ok(Self {
            enabled: table.enabled,
            command,
            args: table.args.clone(),
            fail_on,
            on_missing,
            skip,
            dry_run,
            default_inspect: manifest.defaults.inspect,
            overrides: manifest
                .agent_skills
                .iter()
                .flat_map(|entry| entry.inspect_override.iter())
                .map(|ov| (ov.skill.trim().to_string(), ov.clone()))
                .collect(),
        })
    }

    /// The reviewed override for this exact Agent Skill name, if the manifest has one.
    pub fn override_for(&self, skill: &str) -> Option<&InspectOverride> {
        self.overrides.get(skill)
    }

    /// ` (inspect_override: <reason>)` for plan lines, or empty.
    fn override_suffix(&self, skill: &str) -> String {
        self.override_for(skill)
            .map(|ov| format!(" (inspect_override: {})", ov.reason.trim()))
            .unwrap_or_default()
    }

    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn skipped(&self) -> bool {
        self.skip
    }

    /// Row wins, then `[defaults] inspect`, then scan when the gate is enabled.
    pub fn wants(&self, row: Option<bool>) -> bool {
        if self.skip || !self.enabled {
            return false;
        }
        row.unwrap_or(self.default_inspect.unwrap_or(true))
    }
}

pub fn row_inspect_plugin(manifest: &Manifest, qualified: &str) -> Option<bool> {
    manifest
        .skills
        .iter()
        .find(|entry| entry.qualified().as_deref() == Some(qualified))
        .and_then(|entry| entry.inspect)
}

pub enum ScanOutput {
    Accepted(InspectRecord),
    Rejected {
        record: InspectRecord,
        message: String,
    },
    /// Binary missing and `on_missing = "warn"`. Caller may publish. No record.
    SkippedMissing,
}

/// Stage `src_dir`, scan it, and copy into `hub_root/<skill_name>` only on pass.
/// `Ok(None)` means the copy happened with no scan record (gate off, or warn-and-continue).
/// Dry-run prints the plan and does not copy.
#[allow(clippy::too_many_arguments)]
pub fn publish(
    gate: &Gate,
    row: Option<bool>,
    hub_root: &Path,
    skill_name: &str,
    src_dir: &Path,
    allowed_root: &Path,
    previous: Option<&InspectRecord>,
    sparse_cache: Option<&Path>,
) -> Result<Option<InspectRecord>> {
    if gate.dry_run {
        if gate.wants(row) {
            println!(
                "  {} would inspect {}{}",
                "·".dimmed(),
                skill_name,
                gate.override_suffix(skill_name)
            );
        }
        println!("  {} would install {}", "·".dimmed(), skill_name);
        return Ok(None);
    }
    if !gate.wants(row) {
        copy_published(hub_root, skill_name, src_dir, allowed_root, sparse_cache)?;
        return Ok(None);
    }
    let stage = tempfile::tempdir().context("creating the skillspector stage")?;
    stage_into(
        stage.path(),
        skill_name,
        src_dir,
        allowed_root,
        sparse_cache,
    )?;
    let staged = stage.path().join(skill_name);
    match scan_staged(gate, skill_name, &staged, previous)? {
        ScanOutput::Accepted(record) => {
            copy_published(hub_root, skill_name, &staged, stage.path(), None)?;
            Ok(Some(record))
        }
        ScanOutput::SkippedMissing => {
            copy_published(hub_root, skill_name, &staged, stage.path(), None)?;
            Ok(None)
        }
        ScanOutput::Rejected { record, message } => {
            anyhow::bail!("{skill_name}: {message} (report: {})", record.report)
        }
    }
}

/// Stage `src_dir` and scan that copy. Does not publish.
pub fn scan_dir(
    gate: &Gate,
    skill_name: &str,
    src_dir: &Path,
    allowed_root: &Path,
    previous: Option<&InspectRecord>,
) -> Result<ScanOutput> {
    let stage = tempfile::tempdir().context("creating the skillspector stage")?;
    stage_into(stage.path(), skill_name, src_dir, allowed_root, None)?;
    scan_staged(gate, skill_name, &stage.path().join(skill_name), previous)
}

/// Scan a tree that is already the bytes under consideration. Does not publish.
/// An Agent Skill with a reviewed `inspect_override` is still scanned. A rejection
/// is printed as a warning and the record carries the override reason.
pub fn scan_staged(
    gate: &Gate,
    skill_name: &str,
    staged: &Path,
    previous: Option<&InspectRecord>,
) -> Result<ScanOutput> {
    scan_with_override(
        gate,
        skill_name,
        staged,
        previous,
        gate.override_for(skill_name),
    )
}

fn scan_with_override(
    gate: &Gate,
    skill_name: &str,
    staged: &Path,
    previous: Option<&InspectRecord>,
    reviewed: Option<&InspectOverride>,
) -> Result<ScanOutput> {
    let sha = tree_sha(staged)?;
    if let Some(prev) = previous {
        if prev.sha == sha && prev.passes(gate.fail_on) {
            return Ok(ScanOutput::Accepted(prev.clone()));
        }
    }
    if !command_exists(&gate.command) {
        return match gate.on_missing {
            OnMissing::Error => anyhow::bail!(
                "skillspector command `{}` was not found. Set [skillspector] command, or on_missing = \"warn\".",
                gate.command
            ),
            OnMissing::Warn => {
                eprintln!(
                    "{} skillspector command `{}` was not found. Installed without a scan.",
                    "!".yellow(),
                    gate.command
                );
                Ok(ScanOutput::SkippedMissing)
            }
        };
    }
    let report = report_path(skill_name)?;
    let output = run_scanner(gate, staged, &report)?;
    let mut record = InspectRecord {
        sha,
        recommendation: output.recommendation,
        exit_code: output.exit_code,
        issue_count: output.issue_count,
        max_severity: output.max_severity,
        scanned_at: crate::agent_skill::inventory_now(),
        report: report.display().to_string(),
        no_skill: false,
        override_reason: String::new(),
    };
    if record.passes(gate.fail_on) {
        return Ok(ScanOutput::Accepted(record));
    }
    let message = format!(
        "skillspector rejected the Agent Skill (recommendation {}, exit {}, {} issue(s), max severity {})",
        empty_as(&record.recommendation, "none"),
        record.exit_code,
        record.issue_count,
        empty_as(&record.max_severity, "none")
    );
    match reviewed {
        Some(ov) => {
            eprintln!(
                "{} {skill_name}: {message}. Installed under inspect_override: {}{}. Report: {}",
                "!".yellow(),
                ov.reason.trim(),
                ov.approval(),
                record.report
            );
            for line in &output.findings {
                eprintln!("    {line}");
            }
            record.override_reason = ov.reason.trim().to_string();
            Ok(ScanOutput::Accepted(record))
        }
        None => Ok(ScanOutput::Rejected { record, message }),
    }
}

/// Scan plugin skill trees before enable and before the hub copy.
/// A plugin with no `SKILL.md` trees records `no_skill` and does not fail.
pub fn gate_plugin(gate: &Gate, qualified: &str, row: Option<bool>) -> Result<()> {
    if !gate.wants(row) {
        return Ok(());
    }
    if gate.dry_run {
        println!("  {} would inspect plugin {}", "·".dimmed(), qualified);
        return Ok(());
    }
    let trees = crate::harness::plugin_skill_trees(qualified).unwrap_or_default();
    let mut inv = crate::agent_skill::load_inventory()?;
    if trees.is_empty() {
        println!(
            "  {} {}: no Agent Skill trees to inspect",
            "·".dimmed(),
            qualified
        );
        inv.plugin_scans.insert(
            qualified.to_string(),
            InspectRecord {
                sha: String::new(),
                recommendation: String::new(),
                exit_code: 0,
                issue_count: 0,
                max_severity: String::new(),
                scanned_at: crate::agent_skill::inventory_now(),
                report: String::new(),
                no_skill: true,
                override_reason: String::new(),
            },
        );
        crate::agent_skill::save_inventory(&inv)?;
        return Ok(());
    }
    inv.plugin_scans.remove(qualified);
    for (name, src) in &trees {
        let key = format!("{qualified}/{name}");
        let previous = inv.plugin_scans.get(&key).cloned();
        let stage = tempfile::tempdir().context("creating the skillspector stage")?;
        stage_into(stage.path(), name, src, src, None)?;
        let staged = stage.path().join(name);
        match scan_with_override(gate, name, &staged, previous.as_ref(), None)? {
            ScanOutput::Accepted(record) => {
                inv.plugin_scans.insert(key, record);
            }
            ScanOutput::SkippedMissing => {}
            ScanOutput::Rejected { record, message } => {
                inv.plugin_scans.insert(key, record);
                crate::agent_skill::save_inventory(&inv)?;
                anyhow::bail!("{qualified}/{name}: {message}");
            }
        }
    }
    crate::agent_skill::save_inventory(&inv)?;
    Ok(())
}

pub fn plan_lines(gate: &Gate, manifest: &Manifest) {
    if !gate.dry_run || !gate.enabled || gate.skip {
        return;
    }
    for entry in &manifest.agent_skills {
        if !gate.wants(entry.inspect) {
            continue;
        }
        let suffix = entry
            .name
            .as_deref()
            .map(|name| gate.override_suffix(name))
            .unwrap_or_default();
        println!(
            "  {} would inspect {}{}",
            "·".dimmed(),
            agent_label(entry),
            suffix
        );
    }
    for entry in &manifest.skills {
        if !gate.wants(entry.inspect) {
            continue;
        }
        if let Some(qualified) = entry.qualified() {
            println!("  {} would inspect plugin {}", "·".dimmed(), qualified);
        }
    }
}

pub fn missing_passes() -> Result<Vec<String>> {
    let Some(path) = crate::manifest::discover() else {
        return Ok(Vec::new());
    };
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let manifest = crate::manifest::load(&path)?;
    let gate = Gate::from_manifest(&manifest, false, false)?;
    if !gate.enabled {
        return Ok(Vec::new());
    }
    let inv = crate::agent_skill::load_inventory()?;
    let mut gaps = Vec::new();
    for entry in &manifest.agent_skills {
        if !gate.wants(entry.inspect) {
            continue;
        }
        match entry.name.as_deref() {
            Some(name) => {
                if !hub_passes(&gate, name, inv.agent_skills.get(name)) {
                    gaps.push(format!("Agent Skill {name}"));
                }
            }
            None => {
                let tag = entry.inventory_tag().unwrap_or_default();
                let owned: Vec<_> = inv
                    .agent_skills
                    .iter()
                    .filter(|(_, ent)| ent.source == tag)
                    .map(|(name, _)| name.clone())
                    .collect();
                if owned.is_empty() {
                    gaps.push(format!("Agent Skill row {}", agent_label(entry)));
                } else {
                    for name in owned {
                        if !hub_passes(&gate, &name, inv.agent_skills.get(&name)) {
                            gaps.push(format!("Agent Skill {name}"));
                        }
                    }
                }
            }
        }
    }
    for entry in &manifest.skills {
        if !gate.wants(entry.inspect) {
            continue;
        }
        let Some(qualified) = entry.qualified() else {
            continue;
        };
        if !plugin_passes(&gate, &qualified, &inv) {
            gaps.push(format!("plugin {qualified}"));
        }
    }
    Ok(gaps)
}

pub fn inspect_installed(names: Vec<String>) -> Result<()> {
    let gate = Gate::load(false, false)?;
    if !gate.enabled {
        anyhow::bail!(
            "skillspector gate is disabled. Set [skillspector] enabled = true in skills.toml."
        );
    }
    let manifest = match crate::manifest::discover() {
        Some(path) if path.is_file() => Some(crate::manifest::load(&path)?),
        _ => None,
    };
    let hub = crate::paths::user_skills_dir()?;
    let mut inv = crate::agent_skill::load_inventory()?;
    let targets = if names.is_empty() {
        inv.agent_skills
            .keys()
            .filter(|name| {
                let row = manifest.as_ref().and_then(|m| row_for_name(m, name));
                gate.wants(row)
            })
            .cloned()
            .collect::<Vec<_>>()
    } else {
        names
    };
    if targets.is_empty() {
        println!("No Agent Skills to inspect.");
        return Ok(());
    }
    let mut failed = 0usize;
    for name in &targets {
        let dir = hub.join(name);
        if !dir.join("SKILL.md").is_file() {
            eprintln!("{} {name}: not installed", "✗".red());
            failed += 1;
            continue;
        }
        let previous = inv
            .agent_skills
            .get(name)
            .and_then(|entry| entry.inspect.clone());
        let stage = tempfile::tempdir().context("creating the skillspector stage")?;
        stage_into(stage.path(), name, &dir, &hub, None)?;
        let staged = stage.path().join(name);
        match scan_staged(&gate, name, &staged, previous.as_ref()) {
            Ok(ScanOutput::Accepted(record)) => {
                println!("{} {name} {}", "✓".green(), record.recommendation);
                if let Some(entry) = inv.agent_skills.get_mut(name) {
                    entry.inspect = Some(record);
                }
            }
            Ok(ScanOutput::SkippedMissing) => {}
            Ok(ScanOutput::Rejected { record, message }) => {
                eprintln!("{} {name}: {message}", "✗".red());
                if let Some(entry) = inv.agent_skills.get_mut(name) {
                    entry.inspect = Some(record);
                }
                failed += 1;
            }
            Err(err) => {
                eprintln!("{} {name}: {err:#}", "✗".red());
                failed += 1;
            }
        }
    }
    crate::agent_skill::save_inventory(&inv)?;
    anyhow::ensure!(failed == 0, "{failed} Agent Skill inspect(s) failed");
    Ok(())
}

fn hub_passes(gate: &Gate, name: &str, entry: Option<&crate::agent_skill::Entry>) -> bool {
    let Some(record) = entry.and_then(|entry| entry.inspect.as_ref()) else {
        return false;
    };
    let reviewed = !record.override_reason.is_empty() && gate.override_for(name).is_some();
    if !record.passes(gate.fail_on) && !reviewed {
        return false;
    }
    let Ok(hub) = crate::paths::user_skills_dir() else {
        return false;
    };
    let dir = hub.join(name);
    if !dir.join("SKILL.md").is_file() {
        return false;
    }
    tree_sha(&dir).ok().as_ref() == Some(&record.sha)
}

fn plugin_passes(gate: &Gate, qualified: &str, inv: &crate::agent_skill::Inventory) -> bool {
    if inv
        .plugin_scans
        .get(qualified)
        .is_some_and(|record| record.no_skill && record.passes(gate.fail_on))
    {
        return true;
    }
    match crate::harness::plugin_skill_trees(qualified) {
        Ok(trees) if trees.is_empty() => false,
        Ok(trees) => trees.iter().all(|(name, path)| {
            let key = format!("{qualified}/{name}");
            let plugin_ok = inv.plugin_scans.get(&key).is_some_and(|record| {
                record.passes(gate.fail_on) && staged_sha_matches(name, path, &record.sha)
            });
            let hub_ok = inv.agent_skills.get(name).is_some_and(|entry| {
                entry.source == format!("plugin:{qualified}") && hub_passes(gate, name, Some(entry))
            });
            plugin_ok || hub_ok
        }),
        Err(_) => inv.plugin_scans.iter().any(|(key, record)| {
            (key == qualified || key.starts_with(&format!("{qualified}/")))
                && record.passes(gate.fail_on)
        }),
    }
}

/// The stored sha is of the staged copy (`install_to_root` drops `.git`).
/// Hash that same copy, not the marketplace tree.
fn staged_sha_matches(name: &str, src: &Path, sha: &str) -> bool {
    let Ok(stage) = tempfile::tempdir() else {
        return false;
    };
    if stage_into(stage.path(), name, src, src, None).is_err() {
        return false;
    }
    tree_sha(&stage.path().join(name)).ok().as_deref() == Some(sha)
}

fn row_for_name(manifest: &Manifest, name: &str) -> Option<bool> {
    manifest
        .agent_skills
        .iter()
        .find(|entry| entry.name.as_deref() == Some(name))
        .and_then(|entry| entry.inspect)
}

fn agent_label(entry: &AgentSkillEntry) -> String {
    entry
        .name
        .clone()
        .or_else(|| entry.npm.clone())
        .or_else(|| entry.marketplace.clone())
        .or_else(|| entry.source.clone())
        .unwrap_or_else(|| "agent skill".into())
}

fn stage_into(
    stage_root: &Path,
    skill_name: &str,
    src_dir: &Path,
    allowed_root: &Path,
    sparse_cache: Option<&Path>,
) -> Result<()> {
    if let Some(cache) = sparse_cache {
        crate::agent_skill::install_root_skill_sparse_to(stage_root, skill_name, cache)
    } else {
        crate::agent_skill::install_to_root(stage_root, skill_name, src_dir, allowed_root)
    }
}

fn copy_published(
    hub_root: &Path,
    skill_name: &str,
    src_dir: &Path,
    allowed_root: &Path,
    sparse_cache: Option<&Path>,
) -> Result<()> {
    if let Some(cache) = sparse_cache {
        crate::agent_skill::install_root_skill_sparse_to(hub_root, skill_name, cache)
    } else {
        crate::agent_skill::install_to_root(hub_root, skill_name, src_dir, allowed_root)
    }
}

struct ScannerOutput {
    exit_code: i32,
    recommendation: String,
    issue_count: u32,
    max_severity: String,
    /// One line per issue, for the override warning.
    findings: Vec<String>,
}

fn run_scanner(gate: &Gate, staged: &Path, report: &Path) -> Result<ScannerOutput> {
    if let Some(parent) = report.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut cmd = Command::new(&gate.command);
    cmd.arg("scan").arg(staged);
    if !gate.args.iter().any(|arg| arg == "--no-llm") {
        cmd.arg("--no-llm");
    }
    cmd.args(&gate.args);
    cmd.arg("-f").arg("json").arg("-o").arg(report);
    let output = cmd
        .output()
        .with_context(|| format!("running {}", gate.command))?;
    let exit_code = output.status.code().unwrap_or(2);
    if exit_code == 2 || (!output.status.success() && !report.is_file()) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "skillspector failed (exit {exit_code}){}",
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            }
        );
    }
    let raw =
        fs::read_to_string(report).with_context(|| format!("reading {}", report.display()))?;
    let body: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing skillspector report {}", report.display()))?;
    let recommendation = body
        .pointer("/risk_assessment/recommendation")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let issue_count = body
        .get("issues")
        .and_then(|value| value.as_array())
        .map(|issues| issues.len() as u32)
        .unwrap_or(0);
    Ok(ScannerOutput {
        exit_code,
        recommendation,
        issue_count,
        max_severity: report_max_severity(&body),
        findings: report_findings(&body),
    })
}

/// Collapse whitespace to single spaces and cut at `max` chars, so one finding
/// stays on one warning line.
fn one_line(raw: &str, max: usize) -> String {
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    format!("{cut}...")
}

/// `HIGH TM1 Tool Misuse: git push --force (playbooks/x.md:6)` per issue.
fn report_findings(body: &Value) -> Vec<String> {
    let Some(issues) = body.get("issues").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let text = |issue: &Value, key: &str| {
        issue
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    };
    issues
        .iter()
        .map(|issue| {
            let mut line = [
                text(issue, "severity").to_ascii_uppercase(),
                text(issue, "id"),
                text(issue, "category"),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
            let finding = one_line(&text(issue, "finding"), 160);
            if !finding.is_empty() {
                line.push_str(": ");
                line.push_str(&finding);
            }
            if let Some(file) = issue
                .pointer("/location/file")
                .and_then(|value| value.as_str())
            {
                match issue
                    .pointer("/location/start_line")
                    .and_then(|value| value.as_u64())
                {
                    Some(n) => line.push_str(&format!(" ({file}:{n})")),
                    None => line.push_str(&format!(" ({file})")),
                }
            }
            line
        })
        .collect()
}

/// `risk_assessment.max_issue_severity` when present. Otherwise the highest
/// `issues[].severity`. Rank matches skillspector: LOW, MEDIUM, HIGH, CRITICAL.
fn report_max_severity(body: &Value) -> String {
    if let Some(label) = body
        .pointer("/risk_assessment/max_issue_severity")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|label| !label.is_empty())
    {
        return label.to_ascii_uppercase();
    }
    let mut worst = 0u8;
    let mut label = String::new();
    let Some(issues) = body.get("issues").and_then(|value| value.as_array()) else {
        return label;
    };
    for issue in issues {
        let raw = issue
            .get("severity")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let rank = severity_rank(raw);
        if rank > worst {
            worst = rank;
            label = raw.trim().to_ascii_uppercase();
        }
    }
    label
}

fn severity_rank(raw: &str) -> u8 {
    match raw.trim().to_ascii_uppercase().as_str() {
        "LOW" => 1,
        "MEDIUM" => 2,
        "HIGH" => 3,
        "CRITICAL" => 4,
        _ => 0,
    }
}

fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        Path::new(command).is_file()
    } else {
        which::which(command).is_ok()
    }
}

fn report_path(skill_name: &str) -> Result<PathBuf> {
    let dir = crate::paths::user_skills_dir()?.join(".zskills-reports");
    fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{skill_name}.json")))
}

pub fn tree_sha(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in files {
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(root.join(&rel))
            .with_context(|| format!("reading {}", root.join(&rel).display()))?;
        hasher.update(&bytes);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    Ok(())
}

fn parse_fail_on(raw: &str) -> Result<FailOn> {
    match raw {
        "" | "do_not_install" => Ok(FailOn::DoNotInstall),
        "caution" => Ok(FailOn::Caution),
        "findings" => Ok(FailOn::Findings),
        other => anyhow::bail!(
            "unknown [skillspector] fail_on {other:?}. Use do_not_install, caution, or findings."
        ),
    }
}

fn parse_on_missing(raw: &str) -> Result<OnMissing> {
    match raw {
        "" | "error" => Ok(OnMissing::Error),
        "warn" => Ok(OnMissing::Warn),
        other => anyhow::bail!("unknown [skillspector] on_missing {other:?}. Use error or warn."),
    }
}

fn empty_as<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wants_row_overrides_default() {
        let mut gate = Gate::inactive();
        gate.enabled = true;
        gate.default_inspect = Some(false);
        assert!(!gate.wants(None));
        assert!(gate.wants(Some(true)));
        assert!(!gate.wants(Some(false)));
        gate.skip = true;
        assert!(!gate.wants(Some(true)));
    }

    #[test]
    fn caution_passes_default_policy_only() {
        let caution = InspectRecord {
            sha: "abc".into(),
            recommendation: "CAUTION".into(),
            exit_code: 0,
            issue_count: 0,
            max_severity: String::new(),
            scanned_at: "@0".into(),
            report: "r".into(),
            no_skill: false,
            override_reason: String::new(),
        };
        assert!(caution.passes(FailOn::DoNotInstall));
        assert!(!caution.passes(FailOn::Caution));
        assert!(caution.passes(FailOn::Findings));
        let findings = InspectRecord {
            issue_count: 2,
            ..caution.clone()
        };
        assert!(findings.passes(FailOn::DoNotInstall));
        assert!(!findings.passes(FailOn::Findings));
        let blocked = InspectRecord {
            exit_code: 1,
            recommendation: "DO_NOT_INSTALL".into(),
            ..caution
        };
        assert!(!blocked.passes(FailOn::DoNotInstall));
    }

    #[test]
    fn critical_fails_every_fail_on_even_when_exit_is_zero() {
        let critical = InspectRecord {
            sha: "abc".into(),
            recommendation: "SAFE".into(),
            exit_code: 0,
            issue_count: 1,
            max_severity: "CRITICAL".into(),
            scanned_at: "@0".into(),
            report: "r".into(),
            no_skill: false,
            override_reason: String::new(),
        };
        assert!(!critical.passes(FailOn::DoNotInstall));
        assert!(!critical.passes(FailOn::Caution));
        assert!(!critical.passes(FailOn::Findings));
        let high = InspectRecord {
            max_severity: "HIGH".into(),
            issue_count: 0,
            ..critical
        };
        assert!(high.passes(FailOn::DoNotInstall));
        assert!(high.passes(FailOn::Caution));
        let from_issues = report_max_severity(
            &serde_json::json!({"issues":[{"severity":"low"},{"severity":"CRITICAL"}]}),
        );
        assert_eq!(from_issues, "CRITICAL");
        let from_field = report_max_severity(&serde_json::json!({
            "risk_assessment": {"max_issue_severity": "high"},
            "issues": [{"severity": "CRITICAL"}]
        }));
        assert_eq!(from_field, "HIGH");
    }

    #[test]
    fn unchanged_sha_does_not_spawn() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: demo\n---\n").unwrap();
        let sha = tree_sha(&skill).unwrap();
        let count = dir.path().join("count");
        let script = dir.path().join("fake-skillspector");
        fs::write(
            &script,
            format!("#!/bin/sh\necho x >> {}\nexit 2\n", count.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = fs::metadata(&script).unwrap().permissions();
            perm.set_mode(0o755);
            fs::set_permissions(&script, perm).unwrap();
        }
        let gate = Gate {
            enabled: true,
            command: script.display().to_string(),
            args: Vec::new(),
            fail_on: FailOn::DoNotInstall,
            on_missing: OnMissing::Error,
            skip: false,
            dry_run: false,
            default_inspect: None,
            overrides: HashMap::new(),
        };
        let previous = InspectRecord {
            sha,
            recommendation: "SAFE".into(),
            exit_code: 0,
            issue_count: 0,
            max_severity: String::new(),
            scanned_at: "@1".into(),
            report: "old".into(),
            no_skill: false,
            override_reason: String::new(),
        };
        match scan_staged(&gate, "demo", &skill, Some(&previous)).unwrap() {
            ScanOutput::Accepted(record) => assert_eq!(record.report, "old"),
            ScanOutput::Rejected { .. } | ScanOutput::SkippedMissing => {
                panic!("expected reuse of the stored scan")
            }
        }
        assert!(!count.exists(), "scanner must not run");
    }

    fn rejecting_gate(dir: &Path) -> Gate {
        let script = dir.join("fake-skillspector");
        fs::write(
            &script,
            "#!/bin/sh\nout=\nprev=\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-o\" ]; then out=$a; fi\n  prev=$a\ndone\nmkdir -p \"$(dirname \"$out\")\"\necho '{\"risk_assessment\":{\"recommendation\":\"DO_NOT_INSTALL\"},\"issues\":[{\"id\":\"TM1\",\"severity\":\"HIGH\"}]}' > \"$out\"\nexit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = fs::metadata(&script).unwrap().permissions();
            perm.set_mode(0o755);
            fs::set_permissions(&script, perm).unwrap();
        }
        Gate {
            enabled: true,
            command: script.display().to_string(),
            args: Vec::new(),
            fail_on: FailOn::DoNotInstall,
            on_missing: OnMissing::Error,
            skip: false,
            dry_run: false,
            default_inspect: None,
            overrides: HashMap::new(),
        }
    }

    #[test]
    fn override_accepts_only_its_own_skill_and_records_the_reason() {
        let _guard = crate::paths::HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: held under HOME_ENV_LOCK; restored before the guard drops.
        let prev = std::env::var_os("AGENTS_HOME");
        std::env::set_var("AGENTS_HOME", dir.path().join("agents"));
        let mut gate = rejecting_gate(dir.path());
        gate.overrides.insert(
            "demo".into(),
            InspectOverride {
                skill: "demo".into(),
                reason: "reviewed".into(),
                approved_by: Some("IRL".into()),
                approved_on: None,
            },
        );
        for name in ["demo", "other"] {
            let skill = dir.path().join("src").join(name);
            fs::create_dir_all(&skill).unwrap();
            fs::write(skill.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
        }
        match scan_staged(&gate, "demo", &dir.path().join("src/demo"), None).unwrap() {
            ScanOutput::Accepted(record) => {
                assert_eq!(record.override_reason, "reviewed");
                assert_eq!(record.exit_code, 1, "the real verdict is kept");
                assert!(!record.passes(FailOn::DoNotInstall));
            }
            _ => panic!("the reviewed skill is accepted"),
        }
        assert!(matches!(
            scan_staged(&gate, "other", &dir.path().join("src/other"), None).unwrap(),
            ScanOutput::Rejected { .. }
        ));
        assert!(
            matches!(
                scan_with_override(&gate, "demo", &dir.path().join("src/demo"), None, None)
                    .unwrap(),
                ScanOutput::Rejected { .. }
            ),
            "plugin trees never use an Agent Skill override"
        );
        match prev {
            Some(v) => std::env::set_var("AGENTS_HOME", v),
            None => std::env::remove_var("AGENTS_HOME"),
        }
    }

    #[test]
    fn report_findings_names_severity_id_and_location() {
        let lines = report_findings(&serde_json::json!({"issues":[
            {"id":"TM1","category":"Tool Misuse","severity":"high","finding":"git push --force",
             "location":{"file":"a.md","start_line":6}},
            {"id":"SC2"},
            {"id":"RA2","finding":"line one\n2. line two"}
        ]}));
        assert_eq!(lines[0], "HIGH TM1 Tool Misuse: git push --force (a.md:6)");
        assert_eq!(lines[1], "SC2");
        assert_eq!(lines[2], "RA2: line one 2. line two");
        assert_eq!(one_line(&"x".repeat(200), 160).len(), 163);
    }
}
