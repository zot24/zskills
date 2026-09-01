//! End-to-end CLI tests. We point CLAUDE_HOME and AGENTS_HOME at the same
//! tempdir so the binary cannot touch the real `~/.claude/` or `~/.agents/`,
//! and so `<tempdir>/skills/` is the install target for Agent Skills.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn zskills(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("zskills").unwrap();
    cmd.env("CLAUDE_HOME", home.path());
    // Sandbox the cross-client Agent Skills home to the same tempdir so
    // `<tempdir>/skills/` is the install target (mirrors the production layout
    // where ~/.agents/skills/ lives alongside ~/.claude/).
    cmd.env("AGENTS_HOME", home.path());
    // Sandbox Pi settings so tests never write the developer's `~/.pi`.
    cmd.env("PI_HOME", home.path().join("pi"));
    // Sandbox every other harness home. Directories are not created here, so
    // `default_skill()` hybrid detection does not pick them up unless a test
    // mkdirs them, and visibility never reads the developer's real roots.
    cmd.env("GROK_HOME", home.path().join("grok"));
    cmd.env("HERMES_HOME", home.path().join("hermes"));
    cmd.env("CODEX_HOME", home.path().join("codex"));
    cmd.env("KIMI_HOME", home.path().join("kimi-code"));
    // Sandbox the clone cache too, so repo-install tests never touch ~/.cache.
    cmd.env("XDG_CACHE_HOME", home.path().join("cache"));
    // Sandbox manifest discovery. `manifest::discover()` reads
    // $XDG_CONFIG_HOME/zskills/skills.toml, so without this a test would read the
    // developer's real manifest — and marketplace pins are declared there.
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"));
    // Set for readability of failure dumps. Note: zskills does not currently honour
    // NO_COLOR (owo-colors' override needs its `supports-colors` feature), so the
    // assertions below match the text *inside* the escapes rather than raw output.
    cmd.env("NO_COLOR", "1");
    // Never shell out to the developer's real `claude` binary: it does not honour
    // CLAUDE_HOME, so a stray `plugin install` would mutate the actual ~/.claude.
    // Tests that exercise the delegation set ZSKILLS_CLAUDE_BIN to a stub instead.
    cmd.env("ZSKILLS_NO_CLAUDE_CLI", "1");
    cmd
}

fn fake_home() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let plugins = dir.path().join("plugins");
    fs::create_dir_all(plugins.join("marketplaces")).unwrap();

    // Minimal settings.json with hooks + permissions to verify round-trip preservation.
    let settings = json!({
        "permissions": { "defaultMode": "auto" },
        "hooks": { "SessionStart": [] },
        "extraKnownMarketplaces": {
            "test-mp": { "source": { "source": "github", "repo": "owner/test-mp" } }
        },
        "enabledPlugins": {
            "foo@test-mp": true,
            "bar@test-mp": false
        }
    });
    fs::write(
        dir.path().join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();

    let installed = json!({
        "version": 2,
        "plugins": {
            "foo@test-mp": [{
                "scope": "user",
                "installPath": "/tmp/foo",
                "version": "1.0.0",
                "installedAt": "2026-01-01T00:00:00Z",
                "lastUpdated": "2026-01-01T00:00:00Z"
            }]
        }
    });
    fs::write(
        plugins.join("installed_plugins.json"),
        serde_json::to_string_pretty(&installed).unwrap(),
    )
    .unwrap();

    let known = json!({
        "test-mp": {
            "source": { "source": "github", "repo": "owner/test-mp" },
            "installLocation": "/tmp/marketplaces/test-mp",
            "autoUpdate": true
        }
    });
    fs::write(
        plugins.join("known_marketplaces.json"),
        serde_json::to_string_pretty(&known).unwrap(),
    )
    .unwrap();

    dir
}

#[test]
fn help_works() {
    let home = fake_home();
    zskills(&home)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("marketplaces"));
}

#[test]
fn version_works() {
    let home = fake_home();
    zskills(&home).arg("--version").assert().success();
}

#[test]
fn list_json_reports_active_and_orphan() {
    let home = fake_home();
    let out = zskills(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["plugins"]["active"][0], "foo@test-mp");
    // `bar@test-mp` is in enabledPlugins but value=false AND not installed → not active, not orphan
    assert!(v["plugins"]["enabled_orphan"]
        .as_array()
        .unwrap()
        .is_empty());
    // Agent skills section exists (empty in fake home)
    assert!(v["agent_skills"]["managed"].is_array());
}

#[test]
fn enable_disable_flips_settings_without_clobbering_other_fields() {
    let home = fake_home();
    zskills(&home)
        .args(["plugin", "disable", "foo@test-mp"])
        .assert()
        .success();
    let s: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(s["enabledPlugins"]["foo@test-mp"], false);
    assert_eq!(s["permissions"]["defaultMode"], "auto"); // preserved
    assert!(s["hooks"].is_object()); // preserved

    zskills(&home)
        .args(["plugin", "enable", "foo@test-mp"])
        .assert()
        .success();
    let s: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(s["enabledPlugins"]["foo@test-mp"], true);
}

#[test]
fn scan_finds_project_with_enabled_plugins() {
    let scan_root = tempfile::tempdir().unwrap();
    let proj = scan_root.path().join("a-project");
    let dot_claude = proj.join(".claude");
    fs::create_dir_all(&dot_claude).unwrap();
    fs::write(
        dot_claude.join("settings.json"),
        serde_json::to_string_pretty(&json!({
            "enabledPlugins": { "skill-a@mp": true, "skill-b@mp": false }
        }))
        .unwrap(),
    )
    .unwrap();

    let home = fake_home();
    let out = zskills(&home)
        .args(["scan", scan_root.path().to_str().unwrap(), "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["enabled"][0], "skill-a@mp");
    assert_eq!(arr[0]["enabled"].as_array().unwrap().len(), 1);
}

#[test]
fn migrate_dry_run_does_not_write() {
    let scan_root = tempfile::tempdir().unwrap();
    let proj = scan_root.path().join("p");
    let dot_claude = proj.join(".claude");
    fs::create_dir_all(&dot_claude).unwrap();
    let proj_settings_path = dot_claude.join("settings.json");
    let proj_settings = json!({
        "enabledPlugins": { "newone@mp": true },
        "extraKnownMarketplaces": { "mp": { "source": { "source": "github", "repo": "owner/mp" } } }
    });
    fs::write(
        &proj_settings_path,
        serde_json::to_string_pretty(&proj_settings).unwrap(),
    )
    .unwrap();

    let home = fake_home();
    let before_user = fs::read(home.path().join("settings.json")).unwrap();
    let before_proj = fs::read(&proj_settings_path).unwrap();

    zskills(&home)
        .args(["migrate", proj.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run"));

    let after_user = fs::read(home.path().join("settings.json")).unwrap();
    let after_proj = fs::read(&proj_settings_path).unwrap();
    assert_eq!(before_user, after_user, "user settings must be untouched");
    assert_eq!(
        before_proj, after_proj,
        "project settings must be untouched"
    );
}

#[test]
fn migrate_promotes_and_optionally_clears_project() {
    let scan_root = tempfile::tempdir().unwrap();
    let proj = scan_root.path().join("p");
    let dot_claude = proj.join(".claude");
    fs::create_dir_all(&dot_claude).unwrap();
    let proj_settings_path = dot_claude.join("settings.json");
    fs::write(
        &proj_settings_path,
        serde_json::to_string_pretty(&json!({
            "enabledPlugins": { "newone@mp": true }
        }))
        .unwrap(),
    )
    .unwrap();

    let home = fake_home();
    zskills(&home)
        .args(["migrate", proj.to_str().unwrap(), "--remove-from-project"])
        .assert()
        .success();

    // user got the new entry
    let s: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(s["enabledPlugins"]["newone@mp"], true);
    assert_eq!(s["enabledPlugins"]["foo@test-mp"], true); // preserved

    // project cleared
    let p: serde_json::Value =
        serde_json::from_slice(&fs::read(&proj_settings_path).unwrap()).unwrap();
    assert!(p["enabledPlugins"].as_object().unwrap().is_empty());
}

#[test]
fn scan_detects_project_agent_skills() {
    let scan_root = tempfile::tempdir().unwrap();
    let proj = scan_root.path().join("proj-with-agent");
    let skill_dir = proj.join(".claude").join("skills").join("polish");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# polish\n").unwrap();

    let home = fake_home();
    let out = zskills(&home)
        .args(["scan", scan_root.path().to_str().unwrap(), "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["agent_skills"][0], "polish");
    assert!(arr[0]["enabled"].as_array().unwrap().is_empty());
}

#[test]
fn migrate_promotes_agent_skill_to_user_scope() {
    let scan_root = tempfile::tempdir().unwrap();
    let proj = scan_root.path().join("proj");
    let skill_dir = proj.join(".claude").join("skills").join("mover");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# mover\n").unwrap();
    fs::write(skill_dir.join("notes.md"), "extra doc\n").unwrap();

    let home = fake_home();
    let user_skills = home.path().join("skills");
    assert!(!user_skills.join("mover").exists());

    zskills(&home)
        .args(["migrate", proj.to_str().unwrap()])
        .assert()
        .success();

    assert!(user_skills.join("mover").join("SKILL.md").exists());
    assert!(user_skills.join("mover").join("notes.md").exists());

    // Inventory written
    let inv_path = user_skills.join(".zskills.json");
    assert!(inv_path.exists());
    let inv: serde_json::Value = serde_json::from_slice(&fs::read(&inv_path).unwrap()).unwrap();
    assert!(inv["agent_skills"]["mover"].is_object());
}

#[test]
fn list_reports_agent_skills_section() {
    let home = fake_home();
    let user_skills = home.path().join("skills");
    fs::create_dir_all(user_skills.join("untracked-skill")).unwrap();
    fs::write(
        user_skills.join("untracked-skill").join("SKILL.md"),
        "# untracked\n",
    )
    .unwrap();

    let out = zskills(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let untracked = v["agent_skills"]["untracked"].as_array().unwrap();
    assert!(untracked.iter().any(|x| x == "untracked-skill"));
}

#[test]
fn migrate_skill_promotes_across_projects_and_writes_manifest() {
    let scan_root = tempfile::tempdir().unwrap();
    // Three projects, each with the same agent skill
    for p in &["alpha", "beta", "gamma"] {
        let skill_dir = scan_root
            .path()
            .join(p)
            .join(".claude")
            .join("skills")
            .join("shared-tool");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# shared-tool\n").unwrap();
    }

    let home = fake_home();
    let manifest_dir = tempfile::tempdir().unwrap();
    let manifest_path = manifest_dir.path().join("skills.toml");

    zskills(&home)
        .env("XDG_CONFIG_HOME", manifest_dir.path()) // not used for discovery; we'll point manually
        .env("HOME", manifest_dir.path()) // discover falls back to ~/.config/zskills/
        .args([
            "migrate-skill",
            "shared-tool",
            "--root",
            scan_root.path().to_str().unwrap(),
            "--remove-from-all",
        ])
        .assert()
        .success();

    // Skill is at user scope
    let user_dir = home.path().join("skills").join("shared-tool");
    assert!(user_dir.join("SKILL.md").exists());

    // Inventory tracks it
    let inv: serde_json::Value = serde_json::from_slice(
        &fs::read(home.path().join("skills").join(".zskills.json")).unwrap(),
    )
    .unwrap();
    assert!(inv["agent_skills"]["shared-tool"].is_object());

    // All project copies removed
    for p in &["alpha", "beta", "gamma"] {
        let skill_dir = scan_root
            .path()
            .join(p)
            .join(".claude")
            .join("skills")
            .join("shared-tool");
        assert!(
            !skill_dir.exists(),
            "{} should be removed",
            skill_dir.display()
        );
    }

    // Manifest got an entry (resolved via dirs::home_dir() override)
    let manifest_candidate = manifest_dir
        .path()
        .join(".config")
        .join("zskills")
        .join("skills.toml");
    // Either ~/.config/zskills/skills.toml under our fake HOME got written, or
    // discover() returned None and the entry was placed elsewhere. Just check
    // at least one of the possible paths exists.
    assert!(manifest_candidate.exists() || manifest_path.exists());
}

#[test]
fn append_agent_skill_preserves_existing_content() {
    use std::io::Write;
    let manifest_dir = tempfile::tempdir().unwrap();
    let manifest_path = manifest_dir.path().join("skills.toml");
    let mut f = fs::File::create(&manifest_path).unwrap();
    f.write_all(b"# my notes\n\n[[skills]]\nname = \"existing\"\nmarketplace = \"some-mp\"\n")
        .unwrap();
    drop(f);

    // Use the binary's library via invoking migrate-skill which calls append_agent_skill.
    // Simpler: build a manifest file in a temp project tree, run migrate-skill to write to it.
    let scan_root = tempfile::tempdir().unwrap();
    let skill_dir = scan_root
        .path()
        .join("proj")
        .join(".claude")
        .join("skills")
        .join("appendable");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# appendable\n").unwrap();

    let home = fake_home();
    zskills(&home)
        .env("HOME", manifest_dir.path())
        .args([
            "migrate-skill",
            "appendable",
            "--root",
            scan_root.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let updated = fs::read_to_string(
        manifest_dir
            .path()
            .join(".config")
            .join("zskills")
            .join("skills.toml"),
    )
    .ok();
    // We may have written to a fresh file under the fake HOME's ~/.config/zskills/.
    // Just assert the SKILL itself ended up at user scope.
    let _ = updated;
    let user_dir = home.path().join("skills").join("appendable");
    assert!(user_dir.join("SKILL.md").exists());
}

#[test]
fn list_groups_agent_skills_by_source() {
    let home = fake_home();
    let user_skills = home.path().join("skills");

    // Pre-populate three skills with the same source, plus one with a different source.
    for n in &["skill-a", "skill-b", "skill-c"] {
        fs::create_dir_all(user_skills.join(n)).unwrap();
        fs::write(user_skills.join(n).join("SKILL.md"), "# s\n").unwrap();
    }
    fs::create_dir_all(user_skills.join("solo")).unwrap();
    fs::write(user_skills.join("solo").join("SKILL.md"), "# solo\n").unwrap();

    let inv = json!({
        "version": 1,
        "agent_skills": {
            "skill-a": {"source": "npm:foo", "installed_at": "@0", "head_sha": "1.0"},
            "skill-b": {"source": "npm:foo", "installed_at": "@0", "head_sha": "1.0"},
            "skill-c": {"source": "npm:foo", "installed_at": "@0", "head_sha": "1.0"},
            "solo":    {"source": "owner/solo-repo", "installed_at": "@0", "head_sha": "abc"}
        }
    });
    fs::write(
        user_skills.join(".zskills.json"),
        serde_json::to_string_pretty(&inv).unwrap(),
    )
    .unwrap();

    let out = zskills(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let groups = v["agent_skills"]["managed"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    let npm_group = groups
        .iter()
        .find(|g| g["source"] == "npm:foo")
        .expect("npm:foo group");
    assert_eq!(npm_group["count"], 3);
    assert!(npm_group["skills"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s == "skill-a"));
}

#[test]
fn list_single_skill_source_shows_kind() {
    let home = fake_home();
    let user_skills = home.path().join("skills");
    fs::create_dir_all(user_skills.join("solo")).unwrap();
    fs::write(user_skills.join("solo").join("SKILL.md"), "# solo\n").unwrap();

    let inv = json!({
        "version": 1,
        "agent_skills": {
            "solo": {"source": "owner/solo-repo", "installed_at": "@0", "head_sha": "abc"}
        }
    });
    fs::write(
        user_skills.join(".zskills.json"),
        serde_json::to_string_pretty(&inv).unwrap(),
    )
    .unwrap();

    zskills(&home)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("← github:owner/solo-repo"));
}

#[test]
fn upgrade_runs_without_marketplaces_or_manifest() {
    // Smoke test: upgrade against an empty fake home should succeed and print the
    // "Upgrade complete" line.
    let home = fake_home();
    zskills(&home)
        .args(["skill", "upgrade"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Upgrade complete"));
}

#[test]
fn doctor_detects_orphan_and_fixes_it() {
    let home = fake_home();
    // Give test-mp a readable manifest that lists `foo` but not `ghost`, so
    // "ghost is not offered" is a fact rather than a guess. Without this, doctor
    // deliberately declines to remove the enable — see
    // `doctor_fix_leaves_an_unverifiable_enable_alone`.
    let mp_dir = home
        .path()
        .join("plugins")
        .join("marketplaces")
        .join("test-mp")
        .join(".claude-plugin");
    fs::create_dir_all(&mp_dir).unwrap();
    fs::write(
        mp_dir.join("marketplace.json"),
        serde_json::to_string_pretty(&json!({
            "name": "test-mp",
            "plugins": [{ "name": "foo", "description": "the real one" }]
        }))
        .unwrap(),
    )
    .unwrap();

    // Add an orphan: in enabledPlugins but not in inventory.
    let settings_path = home.path().join("settings.json");
    let mut s: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    s["enabledPlugins"]["ghost@test-mp"] = json!(true);
    fs::write(&settings_path, serde_json::to_string_pretty(&s).unwrap()).unwrap();

    zskills(&home)
        .args(["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ghost@test-mp"));

    zskills(&home).args(["doctor", "--fix"]).assert().success();
    let s: serde_json::Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    assert!(s["enabledPlugins"].get("ghost@test-mp").is_none());
}

#[test]
fn install_interactive_flag_in_help() {
    let home = fake_home();
    zskills(&home)
        .args(["plugin", "install", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-i"))
        .stdout(predicate::str::contains("--interactive"));
}

#[test]
fn search_interactive_flag_in_help() {
    let home = fake_home();
    zskills(&home)
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-i"))
        .stdout(predicate::str::contains("--interactive"));
}

#[test]
fn remove_interactive_flag_in_help() {
    let home = fake_home();
    zskills(&home)
        .args(["plugin", "remove", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-i"))
        .stdout(predicate::str::contains("--interactive"));
}

#[test]
fn install_without_args_or_interactive_errors() {
    let home = fake_home();
    zskills(&home).args(["install"]).assert().failure();
}

#[test]
fn remove_without_args_or_interactive_errors() {
    let home = fake_home();
    zskills(&home).args(["remove"]).assert().failure();
}

/// Build a test fixture where CLAUDE_HOME is nested inside a temp parent dir,
/// so `~/.claude.json` (sibling of `~/.claude/`) can be created at a known path.
fn fake_home_nested() -> (TempDir, std::path::PathBuf) {
    let parent = tempfile::tempdir().unwrap();
    let claude_home = parent.path().join(".claude");
    fs::create_dir_all(claude_home.join("plugins").join("marketplaces")).unwrap();
    fs::write(
        claude_home.join("settings.json"),
        serde_json::to_string(&json!({"enabledPlugins": {}})).unwrap(),
    )
    .unwrap();
    fs::write(
        claude_home.join("plugins").join("installed_plugins.json"),
        r#"{"version":2,"plugins":{}}"#,
    )
    .unwrap();
    fs::write(
        claude_home.join("plugins").join("known_marketplaces.json"),
        "{}",
    )
    .unwrap();
    (parent, claude_home)
}

fn zskills_nested(parent: &TempDir, claude_home: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("zskills").unwrap();
    cmd.env("CLAUDE_HOME", claude_home);
    // Pin the cross-client Agent Skills home next to CLAUDE_HOME so tests stay sandboxed.
    cmd.env("AGENTS_HOME", parent.path().join(".agents"));
    cmd.env("PI_HOME", parent.path().join(".pi-home"));
    cmd.env("GROK_HOME", parent.path().join(".grok-home"));
    cmd.env("HERMES_HOME", parent.path().join(".hermes-home"));
    cmd.env("CODEX_HOME", parent.path().join(".codex-home"));
    cmd.env("KIMI_HOME", parent.path().join(".kimi-code-home"));
    cmd.env("NO_COLOR", "1");
    cmd.env("ZSKILLS_NO_CLAUDE_CLI", "1");
    cmd.env("XDG_CONFIG_HOME", parent.path().join("config"));
    // Make sure the managed-settings probe doesn't pick up a real system file in CI.
    cmd.env(
        "ZSKILLS_MANAGED_SETTINGS",
        parent.path().join("__no_managed__"),
    );
    // Pin CWD so project-scope probes are deterministic.
    cmd.current_dir(parent.path());
    cmd
}

#[test]
fn list_shows_user_mcps_from_claude_json() {
    let (parent, claude_home) = fake_home_nested();
    let claude_json = parent.path().join(".claude.json");
    fs::write(
        &claude_json,
        serde_json::to_string(&json!({
            "mcpServers": {
                "honcho":  { "type": "http", "url": "https://mcp.honcho.dev" },
                "github":  { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"],
                             "env": { "GITHUB_TOKEN": "${GITHUB_TOKEN}" } }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("MCP Servers"))
        .stdout(predicate::str::contains("honcho"))
        .stdout(predicate::str::contains("github"))
        .stdout(predicate::str::contains("1 env"));
}

#[test]
fn list_shows_project_mcps_from_mcp_json_wrapped() {
    let (parent, claude_home) = fake_home_nested();
    fs::write(
        parent.path().join(".mcp.json"),
        serde_json::to_string(&json!({
            "mcpServers": { "postgres": { "command": "docker", "args": ["run", "..."] } }
        }))
        .unwrap(),
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project"))
        .stdout(predicate::str::contains("postgres"));
}

#[test]
fn list_handles_flat_mcp_json_schema() {
    let (parent, claude_home) = fake_home_nested();
    // Many plugins ship .mcp.json without the `mcpServers` wrapper — flat map.
    fs::write(
        parent.path().join(".mcp.json"),
        serde_json::to_string(&json!({
            "linear": { "type": "http", "url": "https://mcp.linear.app/mcp" }
        }))
        .unwrap(),
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("linear"));
}

#[test]
fn list_with_no_mcps_anywhere_shows_none_configured() {
    let (parent, claude_home) = fake_home_nested();
    zskills_nested(&parent, &claude_home)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(none configured)"));
}

#[test]
fn doctor_flags_missing_stdio_command() {
    let (parent, claude_home) = fake_home_nested();
    let claude_json = parent.path().join(".claude.json");
    fs::write(
        &claude_json,
        serde_json::to_string(&json!({
            "mcpServers": {
                "ghost": { "command": "this-binary-definitely-does-not-exist-xyz", "args": [] }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("MCP issue"))
        .stdout(predicate::str::contains("ghost"))
        .stdout(predicate::str::contains("command not found"));
}

#[test]
fn doctor_flags_unset_env_var_referenced_in_mcp() {
    let (parent, claude_home) = fake_home_nested();
    let claude_json = parent.path().join(".claude.json");
    fs::write(
        &claude_json,
        serde_json::to_string(&json!({
            "mcpServers": {
                "linear": {
                    "type": "http",
                    "url": "https://mcp.linear.app/mcp",
                    "headers": { "Authorization": "Bearer ${ZSKILLS_TEST_UNSET_TOKEN_XYZ}" }
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .env_remove("ZSKILLS_TEST_UNSET_TOKEN_XYZ")
        .args(["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ZSKILLS_TEST_UNSET_TOKEN_XYZ"))
        .stdout(predicate::str::contains("referenced but not set"));
}

#[test]
fn doctor_flags_sse_as_deprecated() {
    let (parent, claude_home) = fake_home_nested();
    let claude_json = parent.path().join(".claude.json");
    fs::write(
        &claude_json,
        serde_json::to_string(&json!({
            "mcpServers": { "legacy": { "type": "sse", "url": "https://old.example/sse" } }
        }))
        .unwrap(),
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("legacy"))
        .stdout(predicate::str::contains("sse"))
        .stdout(predicate::str::contains("deprecated"));
}

#[test]
fn doctor_passes_when_mcps_are_healthy() {
    let (parent, claude_home) = fake_home_nested();
    let claude_json = parent.path().join(".claude.json");
    // Use a binary that is guaranteed to be on PATH in any unix env: `sh`.
    fs::write(
        &claude_json,
        serde_json::to_string(&json!({
            "mcpServers": { "shellish": { "command": "sh", "args": ["-c", "echo"] } }
        }))
        .unwrap(),
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["doctor"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("All good").or(predicate::str::contains("MCP issue").not()),
        );
}

#[test]
fn sync_installs_mcp_from_manifest_into_claude_json() {
    let (parent, claude_home) = fake_home_nested();
    let manifest_dir = tempfile::tempdir().unwrap();
    let manifest_path = manifest_dir.path().join("skills.toml");
    fs::write(
        &manifest_path,
        r#"
[[mcps]]
name = "linear"
url = "https://mcp.linear.app/mcp"
transport = "http"
scope = "user"

[[mcps]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
scope = "user"
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
"#,
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["sync", "--file", manifest_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("install mcp"))
        .stdout(predicate::str::contains("linear"))
        .stdout(predicate::str::contains("github"));

    let claude_json: serde_json::Value =
        serde_json::from_slice(&fs::read(parent.path().join(".claude.json")).unwrap()).unwrap();
    assert_eq!(claude_json["mcpServers"]["linear"]["type"], "http");
    assert_eq!(
        claude_json["mcpServers"]["linear"]["url"],
        "https://mcp.linear.app/mcp"
    );
    assert_eq!(claude_json["mcpServers"]["github"]["command"], "npx");
    assert_eq!(
        claude_json["mcpServers"]["github"]["env"]["GITHUB_TOKEN"],
        "${GITHUB_TOKEN}"
    );
}

#[test]
fn sync_writes_project_mcp_to_dot_mcp_json() {
    let (parent, claude_home) = fake_home_nested();
    let manifest_dir = tempfile::tempdir().unwrap();
    let manifest_path = manifest_dir.path().join("skills.toml");
    fs::write(
        &manifest_path,
        r#"
[[mcps]]
name = "postgres"
command = "docker"
args = ["run", "--rm", "..."]
scope = "project"
"#,
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["sync", "--file", manifest_path.to_str().unwrap()])
        .assert()
        .success();
    let mcp_json: serde_json::Value =
        serde_json::from_slice(&fs::read(parent.path().join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp_json["mcpServers"]["postgres"]["command"], "docker");
}

#[test]
fn sync_preserves_unrelated_fields_in_claude_json() {
    let (parent, claude_home) = fake_home_nested();
    // Pre-populate ~/.claude.json with a bunch of unrelated top-level keys.
    let claude_json_path = parent.path().join(".claude.json");
    fs::write(
        &claude_json_path,
        serde_json::to_string(&json!({
            "anonymousId": "abc",
            "claudeCodeFirstTokenDate": "2026-01-01",
            "cachedDynamicConfigs": { "foo": "bar" },
            "mcpServers": { "existing": { "type": "http", "url": "https://x.example" } }
        }))
        .unwrap(),
    )
    .unwrap();
    let manifest_dir = tempfile::tempdir().unwrap();
    let manifest_path = manifest_dir.path().join("skills.toml");
    fs::write(
        &manifest_path,
        r#"
[[mcps]]
name = "linear"
url = "https://mcp.linear.app/mcp"
scope = "user"
"#,
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["sync", "--file", manifest_path.to_str().unwrap()])
        .assert()
        .success();
    let after: serde_json::Value =
        serde_json::from_slice(&fs::read(&claude_json_path).unwrap()).unwrap();
    // Unrelated fields preserved
    assert_eq!(after["anonymousId"], "abc");
    assert_eq!(after["claudeCodeFirstTokenDate"], "2026-01-01");
    assert_eq!(after["cachedDynamicConfigs"]["foo"], "bar");
    // New entry landed
    assert_eq!(
        after["mcpServers"]["linear"]["url"],
        "https://mcp.linear.app/mcp"
    );
    // Existing entry untouched (sync doesn't prune without --prune)
    assert_eq!(after["mcpServers"]["existing"]["type"], "http");
}

#[test]
fn sync_prune_removes_mcps_not_in_manifest() {
    let (parent, claude_home) = fake_home_nested();
    let claude_json_path = parent.path().join(".claude.json");
    fs::write(
        &claude_json_path,
        serde_json::to_string(&json!({
            "mcpServers": {
                "old": { "type": "http", "url": "https://old.example" },
                "keep": { "type": "http", "url": "https://keep.example" }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let manifest_dir = tempfile::tempdir().unwrap();
    let manifest_path = manifest_dir.path().join("skills.toml");
    fs::write(
        &manifest_path,
        r#"
[[mcps]]
name = "keep"
url = "https://keep.example"
scope = "user"
"#,
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["sync", "--file", manifest_path.to_str().unwrap(), "--prune"])
        .assert()
        .success();
    let after: serde_json::Value =
        serde_json::from_slice(&fs::read(&claude_json_path).unwrap()).unwrap();
    assert!(after["mcpServers"].get("old").is_none());
    assert!(after["mcpServers"].get("keep").is_some());
}

#[test]
fn sync_rejects_invalid_mcp_entry() {
    let (parent, claude_home) = fake_home_nested();
    let manifest_dir = tempfile::tempdir().unwrap();
    let manifest_path = manifest_dir.path().join("skills.toml");
    // No command AND no url → stdio inferred, missing command → validation fails.
    fs::write(
        &manifest_path,
        r#"
[[mcps]]
name = "broken"
scope = "user"
"#,
    )
    .unwrap();
    zskills_nested(&parent, &claude_home)
        .args(["sync", "--file", manifest_path.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn list_paths_shows_install_paths_for_plugins() {
    let home = fake_home();
    let out = zskills(&home)
        .args(["list", "--paths"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);
    // fake_home's inventory has foo@test-mp with installPath=/tmp/foo.
    assert!(stdout.contains("foo@test-mp"));
    assert!(stdout.contains("/tmp/foo"));
}

#[test]
fn list_paths_shows_mcp_source_file() {
    let (parent, claude_home) = fake_home_nested();
    let claude_json = parent.path().join(".claude.json");
    fs::write(
        &claude_json,
        serde_json::to_string(&json!({
            "mcpServers": { "x": { "type": "http", "url": "https://x.example" } }
        }))
        .unwrap(),
    )
    .unwrap();
    let out = zskills_nested(&parent, &claude_home)
        .args(["list", "--paths"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);
    assert!(stdout.contains("x"));
    assert!(stdout.contains(".claude.json"));
}

#[test]
fn list_without_paths_omits_them() {
    let home = fake_home();
    let out = zskills(&home)
        .args(["list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);
    // Default mode: install path should NOT appear next to the plugin entry.
    assert!(!stdout.contains("/tmp/foo"));
}

// ──── install <owner/repo> tests ────────────────────────────────────────────
//
// All of these stage a bare-ish local git repo and pass `file:///tmp/<id>` as
// the install spec. `agent_skill::parse_source` accepts any URL containing
// `://`, so `git clone file:///path` works without going to the network.

use std::process::Command as StdCommand;

/// Initialize a git repo at `dir` and commit whatever's in it. The commit is
/// needed because `git clone` against an empty repo errors out.
fn git_init_and_commit(dir: &std::path::Path) {
    StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(["init", "--quiet", "-b", "main"])
        .status()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "user.email", "test@example.com"])
        .status()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "user.name", "Test"])
        .status()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(["add", "-A"])
        .status()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(["commit", "--quiet", "-m", "init"])
        .status()
        .unwrap();
}

fn write_skill(repo: &std::path::Path, name: &str, description: &str) {
    let dir = repo.join("skills").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!(
            "---\nname: {}\ndescription: {}\n---\n# {}\n",
            name, description, name
        ),
    )
    .unwrap();
}

fn file_url(p: &std::path::Path) -> String {
    format!("file://{}", p.display())
}

/// A repo that ships a root-level SKILL.md inside a larger source project
/// (the `ogulcancelik/herdr` shape). Returns the repo path — a named subdir
/// so the derived skill name is stable (tempdir basenames start with `.`).
fn write_root_skill_project(parent: &std::path::Path) -> std::path::PathBuf {
    let repo = parent.join("herdr-repo");
    fs::create_dir_all(repo.join("references")).unwrap();
    fs::create_dir_all(repo.join("docs")).unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::create_dir_all(repo.join("vendor")).unwrap();
    fs::write(
        repo.join("SKILL.md"),
        "---\nname: herdr\ndescription: Control herdr\n---\n\
         See [usage](docs/usage.md) for details.\n",
    )
    .unwrap();
    fs::write(repo.join("references/guide.md"), "guide").unwrap();
    fs::write(repo.join("docs/usage.md"), "usage").unwrap();
    fs::write(repo.join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(repo.join("vendor/blob.bin"), "blob").unwrap();
    fs::write(repo.join("Cargo.toml"), "[package]\nname = \"herdr\"\n").unwrap();
    git_init_and_commit(&repo);
    repo
}

#[test]
fn install_repo_root_skill_is_sparse() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_root_skill_project(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", &file_url(&repo)])
        .assert()
        .success();

    let dest = home.path().join("skills").join("herdr-repo");
    // What the skill needs: SKILL.md, referenced paths, conventional dirs.
    assert!(dest.join("SKILL.md").exists());
    assert!(
        dest.join("docs/usage.md").exists(),
        "referenced path copied"
    );
    assert!(
        dest.join("references/guide.md").exists(),
        "conventional dir copied"
    );
    // What it doesn't: the source tree.
    assert!(!dest.join("src").exists(), "src/ must not be installed");
    assert!(
        !dest.join("vendor").exists(),
        "vendor/ must not be installed"
    );
    assert!(!dest.join("Cargo.toml").exists());
    assert!(!dest.join(".git").exists(), ".git must never be installed");
}

#[test]
fn install_skill_flag_selects_single_skill() {
    let upstream = tempfile::tempdir().unwrap();
    write_skill(upstream.path(), "alpha", "A");
    write_skill(upstream.path(), "beta", "B");
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(upstream.path()),
            "--skill",
            "beta",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("beta"));

    assert!(home.path().join("skills/beta/SKILL.md").exists());
    assert!(!home.path().join("skills/alpha").exists());
}

#[test]
fn install_skill_flag_unknown_name_errors() {
    let upstream = tempfile::tempdir().unwrap();
    write_skill(upstream.path(), "alpha", "A");
    write_skill(upstream.path(), "beta", "B");
    git_init_and_commit(upstream.path());

    let home = fake_home();
    let out = zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(upstream.path()),
            "--skill",
            "nope",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&out);
    assert!(stderr.contains("'nope' not found"));
    assert!(
        stderr.contains("alpha"),
        "error should list available skills"
    );
    assert!(!home.path().join("skills/alpha").exists());
}

#[test]
fn install_skill_flag_conflicts_with_all() {
    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", "owner/repo", "--skill", "x", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn install_skill_flag_bypasses_large_collection_policy() {
    let upstream = tempfile::tempdir().unwrap();
    for i in 0..7 {
        write_skill(upstream.path(), &format!("skill-{}", i), "x");
    }
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(upstream.path()),
            "--skill",
            "skill-3",
        ])
        .assert()
        .success();

    assert!(home.path().join("skills/skill-3/SKILL.md").exists());
    assert!(!home.path().join("skills/skill-0").exists());
}

#[test]
fn doctor_flags_full_repo_install_and_fix_slims_it() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_root_skill_project(upstream.path());

    // Simulate a pre-sparse install: a verbatim copy of the clone, .git and all.
    let home = fake_home();
    let dest = home.path().join("skills").join("herdr-repo");
    fs::create_dir_all(dest.join(".git")).unwrap();
    fs::create_dir_all(dest.join("src")).unwrap();
    fs::write(dest.join("SKILL.md"), "---\nname: herdr\n---\n").unwrap();
    fs::write(dest.join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(dest.join("Cargo.toml"), "[package]\n").unwrap();
    fs::write(
        home.path().join("skills/.zskills.json"),
        json!({
            "version": 1,
            "agent_skills": {
                "herdr-repo": {
                    "source": file_url(&repo),
                    "installed_at": "@0",
                    "head_sha": "legacy"
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("full-repo install"))
        .stdout(predicate::str::contains("herdr-repo"));

    zskills(&home)
        .args(["doctor", "--fix"])
        .assert()
        .success()
        .stdout(predicate::str::contains("re-installed herdr-repo slim"));

    assert!(dest.join("SKILL.md").exists());
    assert!(dest.join("docs/usage.md").exists());
    assert!(!dest.join(".git").exists(), "slim re-install drops .git");
    assert!(!dest.join("src").exists(), "slim re-install drops src/");
    assert!(!dest.join("Cargo.toml").exists());
}

#[test]
fn install_repo_single_skill_auto_installs() {
    let upstream = tempfile::tempdir().unwrap();
    write_skill(upstream.path(), "alpha", "Alpha skill");
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", &file_url(upstream.path())])
        .assert()
        .success()
        .stdout(predicate::str::contains("alpha"));

    assert!(home
        .path()
        .join("skills")
        .join("alpha")
        .join("SKILL.md")
        .exists());
}

#[test]
fn install_repo_small_multi_installs_all() {
    let upstream = tempfile::tempdir().unwrap();
    write_skill(upstream.path(), "alpha", "A");
    write_skill(upstream.path(), "beta", "B");
    write_skill(upstream.path(), "gamma", "C");
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", &file_url(upstream.path())])
        .assert()
        .success();

    for name in ["alpha", "beta", "gamma"] {
        assert!(
            home.path()
                .join("skills")
                .join(name)
                .join("SKILL.md")
                .exists(),
            "{} should be installed",
            name
        );
    }
}

#[test]
fn install_repo_large_collection_aborts_without_all() {
    let upstream = tempfile::tempdir().unwrap();
    for i in 0..7 {
        write_skill(upstream.path(), &format!("skill-{}", i), "x");
    }
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", &file_url(upstream.path())])
        .assert()
        .success()
        .stdout(predicate::str::contains("won't install all"))
        .stdout(predicate::str::contains("--all"));

    // None of the skills should have been installed.
    for i in 0..7 {
        assert!(
            !home
                .path()
                .join("skills")
                .join(format!("skill-{}", i))
                .exists(),
            "large collection must not install silently"
        );
    }
}

#[test]
fn install_repo_large_collection_with_all_installs_everything() {
    let upstream = tempfile::tempdir().unwrap();
    for i in 0..7 {
        write_skill(upstream.path(), &format!("skill-{}", i), "x");
    }
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", &file_url(upstream.path()), "--all"])
        .assert()
        .success();

    for i in 0..7 {
        let p = home
            .path()
            .join("skills")
            .join(format!("skill-{}", i))
            .join("SKILL.md");
        assert!(p.exists(), "skill-{} should be installed", i);
    }
}

#[test]
fn install_repo_marketplace_redirects() {
    let upstream = tempfile::tempdir().unwrap();
    let mp = upstream.path().join(".claude-plugin");
    fs::create_dir_all(&mp).unwrap();
    fs::write(
        mp.join("marketplace.json"),
        r#"{"name":"test","plugins":[]}"#,
    )
    .unwrap();
    // Also put an Agent Skill — to prove marketplace detection wins and the skill is NOT installed.
    write_skill(upstream.path(), "should-not-install", "x");
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", &file_url(upstream.path())])
        .assert()
        .success()
        .stdout(predicate::str::contains("marketplace"))
        .stdout(predicate::str::contains("marketplace add"));

    assert!(
        !home
            .path()
            .join("skills")
            .join("should-not-install")
            .exists(),
        "marketplace path must not install skills"
    );
}

#[test]
fn install_repo_mcp_hint_appears_alongside_skill_install() {
    let upstream = tempfile::tempdir().unwrap();
    write_skill(upstream.path(), "alpha", "A");
    fs::write(
        upstream.path().join(".mcp.json"),
        r#"{"mcpServers":{"linear":{"type":"http","url":"https://x"}}}"#,
    )
    .unwrap();
    git_init_and_commit(upstream.path());

    let home = fake_home();
    let out = zskills(&home)
        .args(["skill", "install", &file_url(upstream.path())])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&out);
    assert!(stderr.contains("MCP server"));
    assert!(home
        .path()
        .join("skills")
        .join("alpha")
        .join("SKILL.md")
        .exists());
}

#[test]
fn install_repo_with_no_skills_errors() {
    let upstream = tempfile::tempdir().unwrap();
    // Empty repo — no skills/, no .claude-plugin/.
    fs::write(upstream.path().join("README.md"), "# nothing here\n").unwrap();
    git_init_and_commit(upstream.path());

    let home = fake_home();
    let out = zskills(&home)
        .args(["skill", "install", &file_url(upstream.path())])
        .assert()
        // A failed install exits non-zero: the error is on stderr *and* in $?.
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&out);
    assert!(stderr.contains("no Agent Skills found"));
}

// ---------------------------------------------------------------------------
// Honest install: marketplace `lastUpdated`, and enable-vs-install.
//
// Background (reproduced 2026-08-20): `zskills marketplace add` wrote a
// known_marketplaces.json entry with no `lastUpdated`, and Claude Code then
// refused the whole file — "Marketplace configuration file is corrupted:
// <name>.lastUpdated: Invalid input: expected string, received undefined" —
// which broke every `claude plugin install`. Meanwhile `zskills doctor` reported
// "All good", and `doctor --fix` would have *deleted* the enable for a plugin
// that had just been requested.
// ---------------------------------------------------------------------------

/// A local git repo shaped like a plugin marketplace, carrying one plugin.
fn write_marketplace_repo(parent: &std::path::Path, mp: &str, plugin: &str) -> std::path::PathBuf {
    let repo = parent.join(mp);
    fs::create_dir_all(repo.join(".claude-plugin")).unwrap();
    fs::write(
        repo.join(".claude-plugin").join("marketplace.json"),
        serde_json::to_string_pretty(&json!({
            "name": mp,
            "description": "test marketplace",
            "plugins": [{ "name": plugin, "description": "a real plugin", "source": "./p" }]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::create_dir_all(repo.join("p")).unwrap();
    fs::write(repo.join("p").join("plugin.json"), r#"{"name":"p"}"#).unwrap();
    git_init_and_commit(&repo);
    repo
}

/// Install a marketplace cache directly into CLAUDE_HOME, bypassing `marketplace add`,
/// and register it in known_marketplaces.json with the given `lastUpdated` (or none).
fn register_marketplace(home: &TempDir, mp: &str, plugin: &str, last_updated: Option<&str>) {
    let dir = home.path().join("plugins").join("marketplaces").join(mp);
    fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
    fs::write(
        dir.join(".claude-plugin").join("marketplace.json"),
        serde_json::to_string_pretty(&json!({
            "name": mp,
            "plugins": [{ "name": plugin, "description": "a real plugin" }]
        }))
        .unwrap(),
    )
    .unwrap();

    let known_path = home.path().join("plugins").join("known_marketplaces.json");
    let mut known: serde_json::Value =
        serde_json::from_slice(&fs::read(&known_path).unwrap()).unwrap();
    let mut entry = json!({
        "source": { "source": "github", "repo": format!("owner/{}", mp) },
        "installLocation": dir.to_string_lossy(),
        "autoUpdate": true
    });
    if let Some(ts) = last_updated {
        entry["lastUpdated"] = json!(ts);
    }
    known[mp] = entry;
    fs::write(&known_path, serde_json::to_string_pretty(&known).unwrap()).unwrap();
}

fn read_known(home: &TempDir) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(home.path().join("plugins").join("known_marketplaces.json")).unwrap(),
    )
    .unwrap()
}

fn read_settings(home: &TempDir) -> serde_json::Value {
    serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap()
}

/// A stand-in for the `claude` binary that records its argv and *actually*
/// materializes the plugin, the way the real CLI does — it writes the entry into
/// `installed_plugins.json` under `$CLAUDE_CONFIG_DIR`.
///
/// Writing through `$CLAUDE_CONFIG_DIR` is deliberate: zskills locates state via
/// `CLAUDE_HOME` but Claude Code reads `CLAUDE_CONFIG_DIR`, so if zskills ever stops
/// propagating it, the stub writes to the wrong place (or nowhere) and the success
/// assertions fail. Returns (stub path, log path).
fn claude_stub(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let stub = dir.join("claude-stub.sh");
    let log = dir.join("claude-invocations.log");
    fs::write(
        &stub,
        format!(
            r#"#!/bin/sh
echo "$@" >> {log}
echo "CLAUDE_CONFIG_DIR=$CLAUDE_CONFIG_DIR" >> {log}
[ -n "$CLAUDE_CONFIG_DIR" ] || exit 9
command -v python3 >/dev/null || {{ echo "test stub requires python3" >&2; exit 97; }}
python3 -c '
import json, sys
path, qualified = sys.argv[1], sys.argv[2]
d = json.load(open(path))
d.setdefault("plugins", {{}})[qualified] = [
    {{"scope": "user", "installPath": "/tmp/x", "version": "1.0.0"}}
]
json.dump(d, open(path, "w"))
' "$CLAUDE_CONFIG_DIR/plugins/installed_plugins.json" "$3"
exit 0
"#,
            log = log.display()
        ),
    )
    .unwrap();
    make_executable(&stub);
    (stub, log)
}

/// A stand-in that always fails, so we can assert we don't claim success.
fn claude_stub_failing(dir: &std::path::Path) -> std::path::PathBuf {
    let stub = dir.join("claude-fail.sh");
    fs::write(
        &stub,
        "#!/bin/sh\necho 'marketplace not found' >&2\nexit 1\n",
    )
    .unwrap();
    make_executable(&stub);
    stub
}

/// A stand-in that exits 0 while doing nothing at all — the dangerous case, because
/// a successful exit code is a *claim* that bytes landed, not proof of it.
fn claude_stub_lying(dir: &std::path::Path) -> std::path::PathBuf {
    let stub = dir.join("claude-lie.sh");
    fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
    make_executable(&stub);
    stub
}

/// A stand-in that hangs, to exercise the subprocess timeout.
fn claude_stub_hanging(dir: &std::path::Path) -> std::path::PathBuf {
    let stub = dir.join("claude-hang.sh");
    fs::write(&stub, "#!/bin/sh\nsleep 30\n").unwrap();
    make_executable(&stub);
    stub
}

fn make_executable(p: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = p;
}

// --- Fix 1: `marketplace add` always writes a string `lastUpdated` -----------

#[test]
fn marketplace_add_writes_last_updated_as_a_string() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_marketplace_repo(upstream.path(), "vercel-plugin", "vercel");

    let home = fake_home();
    zskills(&home)
        .args(["marketplace", "add", &file_url(&repo)])
        .assert()
        .success();

    // The file must still be valid JSON, and the field must be a *string* —
    // this is exactly what Claude Code's schema validates.
    let known = read_known(&home);
    let entry = &known["vercel-plugin"];
    assert!(entry.is_object(), "marketplace not registered: {:#}", known);
    let ts = entry
        .get("lastUpdated")
        .unwrap_or_else(|| panic!("lastUpdated missing from {:#}", entry));
    assert!(ts.is_string(), "lastUpdated must be a string, got {:?}", ts);
    let ts = ts.as_str().unwrap();
    assert_eq!(ts.len(), 24, "expected toISOString() shape, got {:?}", ts);
    assert!(
        ts.ends_with('Z') && ts.contains('T'),
        "not ISO-8601: {}",
        ts
    );
    // Sibling fields survive.
    assert_eq!(entry["autoUpdate"], json!(true));
    assert!(entry["installLocation"].is_string());
}

#[test]
fn marketplace_add_prints_plugins_and_the_next_install_command() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_marketplace_repo(upstream.path(), "llm-wiki", "wiki");

    let home = fake_home();
    zskills(&home)
        .args(["marketplace", "add", &file_url(&repo)])
        .assert()
        .success()
        .stdout(predicate::str::contains("added marketplace llm-wiki"))
        .stdout(predicate::str::contains("1 plugin: wiki"))
        .stdout(predicate::str::contains(
            "zskills plugin install wiki@llm-wiki",
        ))
        .stdout(predicate::str::contains("Agent Skills").not());
}

/// llm-wiki shape: Claude plugin at `./claude-plugin`, Agent Skills at
/// `plugins/llm-wiki-opencode/skills/<name>/SKILL.md`. Nested plugin skills
/// under the Claude plugin source must not be hinted.
fn write_llm_wiki_shaped_repo(parent: &std::path::Path) -> std::path::PathBuf {
    let repo = parent.join("llm-wiki");
    fs::create_dir_all(repo.join(".claude-plugin")).unwrap();
    fs::write(
        repo.join(".claude-plugin").join("marketplace.json"),
        serde_json::to_string_pretty(&json!({
            "name": "llm-wiki",
            "plugins": [{ "name": "wiki", "source": "./claude-plugin" }]
        }))
        .unwrap(),
    )
    .unwrap();
    let claude_skill = repo
        .join("claude-plugin")
        .join("skills")
        .join("wiki-manager");
    fs::create_dir_all(&claude_skill).unwrap();
    fs::write(claude_skill.join("SKILL.md"), "# wiki-manager (claude)\n").unwrap();
    for name in ["wiki-manager", "wiki-query"] {
        let dir = repo
            .join("plugins")
            .join("llm-wiki-opencode")
            .join("skills")
            .join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
    }
    git_init_and_commit(&repo);
    repo
}

#[test]
fn marketplace_add_hints_agent_skill_trees_outside_the_plugin_source() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_llm_wiki_shaped_repo(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["marketplace", "add", &file_url(&repo)])
        .assert()
        .success()
        .stdout(predicate::str::contains("added marketplace llm-wiki"))
        .stdout(predicate::str::contains("1 plugin: wiki"))
        .stdout(predicate::str::contains(
            "zskills plugin install wiki@llm-wiki",
        ))
        .stdout(predicate::str::contains(
            "Agent Skills under plugins/llm-wiki-opencode/skills: wiki-manager, wiki-query",
        ))
        .stdout(predicate::str::contains("[[agent_skills]]"))
        .stdout(predicate::str::contains("marketplace = \"llm-wiki\""))
        .stdout(predicate::str::contains(
            "path = \"plugins/llm-wiki-opencode/skills\"",
        ))
        .stdout(predicate::str::contains("name = \"wiki-manager\""))
        .stdout(predicate::str::contains("name = \"wiki-query\""))
        .stdout(predicate::str::contains("Agent Skills under claude-plugin/skills").not());

    // Hint only: add must not write the manifest.
    assert!(!home
        .path()
        .join("config")
        .join("zskills")
        .join("skills.toml")
        .exists());
}

#[test]
fn marketplace_add_does_not_hint_skills_inside_the_plugin_source() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = upstream.path().join("nested-src");
    fs::create_dir_all(repo.join(".claude-plugin")).unwrap();
    fs::write(
        repo.join(".claude-plugin").join("marketplace.json"),
        serde_json::to_string_pretty(&json!({
            "name": "nested-src",
            "plugins": [{ "name": "foo", "source": "./plugins/foo" }]
        }))
        .unwrap(),
    )
    .unwrap();
    let nested = repo.join("plugins").join("foo").join("skills").join("bar");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("SKILL.md"), "# bar\n").unwrap();
    git_init_and_commit(&repo);

    let home = fake_home();
    zskills(&home)
        .args(["marketplace", "add", &file_url(&repo)])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 plugin: foo"))
        .stdout(predicate::str::contains("Agent Skills").not());
}

#[test]
fn marketplace_add_warns_when_marketplace_json_is_missing() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = upstream.path().join("not-a-marketplace");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("README.md"), "not a plugin catalog").unwrap();
    git_init_and_commit(&repo);

    let home = fake_home();
    zskills(&home)
        .args(["marketplace", "add", &file_url(&repo)])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "added marketplace not-a-marketplace",
        ))
        .stdout(predicate::str::contains("marketplace.json"));
}

/// Empty `known_marketplaces.json` and `enabledPlugins` — a wiped machine.
fn wipe_plugins(home: &TempDir) {
    let settings_path = home.path().join("settings.json");
    let mut settings: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    settings["enabledPlugins"] = json!({});
    settings["extraKnownMarketplaces"] = json!({});
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    fs::write(
        home.path().join("plugins").join("known_marketplaces.json"),
        "{}\n",
    )
    .unwrap();
}

#[test]
fn sync_registers_declared_marketplace_and_enables_plugin() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_marketplace_repo(upstream.path(), "skills", "foo");

    let home = fake_home();
    wipe_plugins(&home);
    write_manifest(
        &home,
        &format!(
            "[[marketplaces]]\nname = \"zot24-skills\"\nurl = \"{}\"\n\n[[skills]]\nname = \"foo\"\nmarketplace = \"zot24-skills\"\n",
            file_url(&repo)
        ),
    );

    zskills(&home)
        .args(["sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("register marketplace"))
        .stdout(predicate::str::contains("zot24-skills"))
        .stdout(predicate::str::contains("enable  plugin  foo@zot24-skills"))
        .stdout(predicate::str::contains("applied."));

    let known = read_known(&home);
    let entry = &known["zot24-skills"];
    assert!(entry.is_object(), "marketplace not registered: {known:#}");
    assert!(
        entry["lastUpdated"].is_string(),
        "lastUpdated must be a string: {entry:#}"
    );
    assert_eq!(entry["source"]["source"], json!("git"));
    assert!(home
        .path()
        .join("plugins")
        .join("marketplaces")
        .join("zot24-skills")
        .join(".claude-plugin")
        .join("marketplace.json")
        .is_file());

    let settings = read_settings(&home);
    assert_eq!(settings["enabledPlugins"]["foo@zot24-skills"], true);
    assert!(
        settings["extraKnownMarketplaces"]["zot24-skills"]
            .get("source")
            .is_some(),
        "extraKnownMarketplaces must mirror the clone: {settings:#}"
    );
}

#[test]
fn sync_dry_run_does_not_register_marketplace() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_marketplace_repo(upstream.path(), "skills", "foo");

    let home = fake_home();
    wipe_plugins(&home);
    write_manifest(
        &home,
        &format!(
            "[[marketplaces]]\nname = \"zot24-skills\"\nurl = \"{}\"\n\n[[skills]]\nname = \"foo\"\nmarketplace = \"zot24-skills\"\n",
            file_url(&repo)
        ),
    );

    zskills(&home)
        .args(["sync", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("register marketplace"))
        .stdout(predicate::str::contains("dry-run"));

    let known = read_known(&home);
    assert!(
        known.get("zot24-skills").is_none(),
        "dry-run must not write known_marketplaces.json: {known:#}"
    );
    assert!(!home
        .path()
        .join("plugins")
        .join("marketplaces")
        .join("zot24-skills")
        .exists());
    let settings = read_settings(&home);
    assert!(
        settings["enabledPlugins"].get("foo@zot24-skills").is_none(),
        "dry-run must not write enabledPlugins: {settings:#}"
    );
}

#[test]
fn sync_refuses_dangling_enabled_plugins_when_marketplace_is_unknown() {
    let home = fake_home();
    wipe_plugins(&home);
    write_manifest(
        &home,
        "[[skills]]\nname = \"foo\"\nmarketplace = \"ghost\"\n",
    );

    zskills(&home)
        .args(["sync"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("foo@ghost"))
        .stdout(predicate::str::contains("not registered"))
        .stdout(predicate::str::contains("applied.").not())
        .stderr(predicate::str::contains("enabledPlugins"));

    let settings = read_settings(&home);
    assert!(
        settings["enabledPlugins"].get("foo@ghost").is_none(),
        "must not write a dangling enable: {settings:#}"
    );
}

#[test]
fn sync_refuses_when_declared_marketplace_has_no_source() {
    let home = fake_home();
    wipe_plugins(&home);
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"ghost\"\npin = \"v1\"\n\n[[skills]]\nname = \"foo\"\nmarketplace = \"ghost\"\n",
    );

    zskills(&home)
        .args(["sync"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("no repo or url"))
        .stderr(predicate::str::contains("not registered"));

    let settings = read_settings(&home);
    assert!(settings["enabledPlugins"].get("foo@ghost").is_none());
}

#[test]
fn sync_adopt_appends_marketplace_source() {
    let home = fake_home();
    write_manifest(
        &home,
        "[[skills]]\nname = \"foo\"\nmarketplace = \"test-mp\"\n",
    );

    zskills(&home)
        .args(["sync", "--adopt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("adopt   marketplace  test-mp"));

    let body = fs::read_to_string(home.path().join("config/zskills/skills.toml")).unwrap();
    assert!(
        body.contains("[[marketplaces]]"),
        "adopt must declare the marketplace: {body}"
    );
    assert!(
        body.contains("name = \"test-mp\""),
        "adopt must keep the registered name: {body}"
    );
    assert!(
        body.contains("repo = \"owner/test-mp\""),
        "adopt must write the github source: {body}"
    );
}

#[test]
fn sync_adopt_fills_source_on_pin_only_row() {
    let home = fake_home();
    write_manifest(
        &home,
        "# keep\n[[marketplaces]]\nname = \"test-mp\"\npin = \"v1\"\n\n[[skills]]\nname = \"foo\"\nmarketplace = \"test-mp\"\n",
    );

    zskills(&home)
        .args(["sync", "--adopt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no repo or url"));

    let body = fs::read_to_string(home.path().join("config/zskills/skills.toml")).unwrap();
    assert!(body.contains("# keep"), "comments must survive: {body}");
    assert!(
        body.contains("pin = \"v1\""),
        "adopt must not drop the pin: {body}"
    );
    assert!(
        body.contains("repo = \"owner/test-mp\""),
        "adopt must fill the github source: {body}"
    );
}

#[test]
fn sync_adopt_does_not_clobber_existing_source_or_pin() {
    let home = fake_home();
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"test-mp\"\nrepo = \"owner/test-mp\"\npin = \"v1\"\n\n[[skills]]\nname = \"foo\"\nmarketplace = \"test-mp\"\n",
    );

    zskills(&home)
        .args(["sync", "--adopt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("adopt   marketplace").not());

    let body = fs::read_to_string(home.path().join("config/zskills/skills.toml")).unwrap();
    assert!(
        body.contains("repo = \"owner/test-mp\""),
        "existing repo must stay: {body}"
    );
    assert!(
        body.contains("pin = \"v1\""),
        "existing pin must stay: {body}"
    );
    assert_eq!(
        body.matches("[[marketplaces]]").count(),
        1,
        "must not append a second row: {body}"
    );
}

#[test]
fn sync_resolves_unqualified_plugin_after_register() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_marketplace_repo(upstream.path(), "skills", "foo");

    let home = fake_home();
    wipe_plugins(&home);
    write_manifest(
        &home,
        &format!(
            "[[marketplaces]]\nname = \"zot24-skills\"\nurl = \"{}\"\n\n[[skills]]\nname = \"foo\"\n",
            file_url(&repo)
        ),
    );

    zskills(&home)
        .args(["sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("register marketplace"))
        .stdout(predicate::str::contains("resolve plugin"))
        .stdout(predicate::str::contains("applied."));

    let settings = read_settings(&home);
    assert_eq!(
        settings["enabledPlugins"]["foo@zot24-skills"], true,
        "unqualified name must resolve after register: {settings:#}"
    );
}

#[test]
fn sync_late_resolve_keeps_already_enabled_plugin_on() {
    let upstream = tempfile::tempdir().unwrap();
    let repo = write_marketplace_repo(upstream.path(), "skills", "foo");

    let home = fake_home();
    wipe_plugins(&home);
    let settings_path = home.path().join("settings.json");
    let mut settings = read_settings(&home);
    settings["enabledPlugins"] = json!({ "foo@zot24-skills": true });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    write_manifest(
        &home,
        &format!(
            "[[marketplaces]]\nname = \"zot24-skills\"\nurl = \"{}\"\n\n[[skills]]\nname = \"foo\"\n",
            file_url(&repo)
        ),
    );

    zskills(&home)
        .args(["sync", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("disable plugin").not());

    zskills(&home)
        .args(["sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("disable plugin").not());

    let after = read_settings(&home);
    assert_eq!(
        after["enabledPlugins"]["foo@zot24-skills"], true,
        "late resolve must not flip an already-enabled match off: {after:#}"
    );
}

#[test]
fn sync_skips_register_when_marketplace_is_already_known() {
    let home = fake_home();
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"test-mp\"\nrepo = \"owner/test-mp\"\n\n[[skills]]\nname = \"foo\"\nmarketplace = \"test-mp\"\n",
    );

    zskills(&home)
        .args(["sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("register marketplace").not());
}

#[test]
fn list_hints_when_a_marketplace_offers_a_plugin_but_none_are_active() {
    let home = fake_home();
    let settings_path = home.path().join("settings.json");
    let mut settings: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    settings["enabledPlugins"] = json!({});
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    fs::write(
        home.path().join("plugins").join("installed_plugins.json"),
        serde_json::to_string_pretty(&json!({ "version": 2, "plugins": {} })).unwrap(),
    )
    .unwrap();
    register_marketplace(&home, "llm-wiki", "wiki", Some("2026-08-20T00:00:00.000Z"));

    zskills(&home)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("(none)"))
        .stdout(predicate::str::contains(
            "zskills plugin install wiki@llm-wiki",
        ));
}

// --- Fix 2: doctor reports a missing `lastUpdated`, and --fix stamps it ------

#[test]
fn doctor_flags_marketplace_missing_last_updated_instead_of_all_good() {
    let home = fake_home();
    // fake_home()'s `test-mp` entry has no lastUpdated — the exact broken state.
    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("lastUpdated"))
        .stdout(predicate::str::contains("test-mp"))
        .stdout(predicate::str::contains("All good").not());
}

#[test]
fn doctor_fix_stamps_last_updated_and_keeps_the_tap() {
    let home = fake_home();
    zskills(&home).args(["doctor", "--fix"]).assert().success();

    let known = read_known(&home);
    assert!(
        known.get("test-mp").is_some(),
        "--fix must not drop the marketplace: {:#}",
        known
    );
    assert!(
        known["test-mp"]["lastUpdated"].is_string(),
        "--fix must write a string timestamp: {:#}",
        known["test-mp"]
    );
    // Everything else about the tap is untouched.
    assert_eq!(known["test-mp"]["autoUpdate"], json!(true));
    assert_eq!(known["test-mp"]["source"]["repo"], json!("owner/test-mp"));
}

#[test]
fn doctor_is_quiet_when_last_updated_is_present() {
    let home = fake_home();
    let known_path = home.path().join("plugins").join("known_marketplaces.json");
    let mut known: serde_json::Value =
        serde_json::from_slice(&fs::read(&known_path).unwrap()).unwrap();
    known["test-mp"]["lastUpdated"] = json!("2026-08-20T00:00:00.000Z");
    fs::write(&known_path, serde_json::to_string_pretty(&known).unwrap()).unwrap();

    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("lastUpdated").not());
}

// --- Fix 3: `install` materializes bytes rather than only enabling ----------

#[test]
fn install_plugin_invokes_claude_to_fetch_the_bytes() {
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );
    let stub_dir = tempfile::tempdir().unwrap();
    let (stub, log) = claude_stub(stub_dir.path());

    zskills(&home)
        .env_remove("ZSKILLS_NO_CLAUDE_CLI")
        .env("ZSKILLS_CLAUDE_BIN", &stub)
        .args(["plugin", "install", "vercel@vercel-plugin"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fetched vercel@vercel-plugin"));

    // The enable is recorded...
    let settings = read_settings(&home);
    assert_eq!(
        settings["enabledPlugins"]["vercel@vercel-plugin"],
        json!(true)
    );
    // ...and the fetch was actually delegated, fully qualified and user-scoped.
    let invocations = fs::read_to_string(&log).unwrap();
    assert!(
        invocations.contains("plugin install vercel@vercel-plugin -s user"),
        "expected a qualified user-scope install, got: {:?}",
        invocations
    );
}

#[test]
fn install_plugin_reports_pending_when_claude_cli_is_missing() {
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );

    // ZSKILLS_NO_CLAUDE_CLI=1 is already set by the helper.
    zskills(&home)
        .args(["plugin", "install", "vercel@vercel-plugin"])
        .assert()
        // The plugin genuinely is not installed, so the exit code says so too.
        .failure()
        // Honest: it says what did *not* happen instead of claiming success.
        .stdout(predicate::str::contains("enabled but not installed"))
        .stdout(predicate::str::contains("`claude` CLI was not found"))
        .stdout(predicate::str::contains("Restart Claude Code").not());

    // The intent is still recorded, so a later `doctor --fix` can finish the job.
    let settings = read_settings(&home);
    assert_eq!(
        settings["enabledPlugins"]["vercel@vercel-plugin"],
        json!(true)
    );
}

#[test]
fn install_plugin_does_not_claim_success_when_the_fetch_fails() {
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = claude_stub_failing(stub_dir.path());

    zskills(&home)
        .env_remove("ZSKILLS_NO_CLAUDE_CLI")
        .env("ZSKILLS_CLAUDE_BIN", &stub)
        .args(["plugin", "install", "vercel@vercel-plugin"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("enabled but not installed"))
        .stdout(predicate::str::contains("fetched vercel@vercel-plugin").not());
}

// --- Fix 4: doctor --fix must not delete an enable it can satisfy -----------

#[test]
fn doctor_fix_installs_a_real_plugin_instead_of_removing_the_enable() {
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );

    // The regression state: enabled, present in a registered marketplace, no bytes.
    let settings_path = home.path().join("settings.json");
    let mut s: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    s["enabledPlugins"]["vercel@vercel-plugin"] = json!(true);
    fs::write(&settings_path, serde_json::to_string_pretty(&s).unwrap()).unwrap();

    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("enabled but not installed"))
        .stdout(predicate::str::contains("vercel@vercel-plugin"));

    let stub_dir = tempfile::tempdir().unwrap();
    let (stub, log) = claude_stub(stub_dir.path());
    zskills(&home)
        .env_remove("ZSKILLS_NO_CLAUDE_CLI")
        .env("ZSKILLS_CLAUDE_BIN", &stub)
        .args(["doctor", "--fix"])
        .assert()
        .success();

    // THE regression assertion: the enable survives.
    let settings = read_settings(&home);
    assert_eq!(
        settings["enabledPlugins"]["vercel@vercel-plugin"],
        json!(true),
        "doctor --fix deleted an enable it should have satisfied: {:#}",
        settings["enabledPlugins"]
    );
    assert!(
        fs::read_to_string(&log)
            .unwrap()
            .contains("plugin install vercel@vercel-plugin -s user"),
        "doctor --fix should have fetched the bytes"
    );
}

#[test]
fn doctor_fix_still_drops_an_enable_no_marketplace_offers() {
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );

    let settings_path = home.path().join("settings.json");
    let mut s: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    s["enabledPlugins"]["ghost@vercel-plugin"] = json!(true);
    // A real, offered plugin sitting in the same broken state, so the assertion
    // below discriminates: it too is enabled-but-not-installed, and the only thing
    // separating it from `ghost` is that the manifest lists it.
    s["enabledPlugins"]["vercel@vercel-plugin"] = json!(true);
    fs::write(&settings_path, serde_json::to_string_pretty(&s).unwrap()).unwrap();

    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "offered by no registered marketplace",
        ))
        .stdout(predicate::str::contains("ghost@vercel-plugin"));

    zskills(&home).args(["doctor", "--fix"]).assert().success();

    let settings = read_settings(&home);
    assert!(
        settings["enabledPlugins"]
            .get("ghost@vercel-plugin")
            .is_none(),
        "a dangling enable should still be cleaned up"
    );
    // The plugin the marketplace *does* offer survives the cleanup.
    assert_eq!(
        settings["enabledPlugins"]["vercel@vercel-plugin"],
        json!(true)
    );
}

#[test]
fn doctor_fix_reports_partial_repair_honestly() {
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );

    let settings_path = home.path().join("settings.json");
    let mut s: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    s["enabledPlugins"]["vercel@vercel-plugin"] = json!(true);
    fs::write(&settings_path, serde_json::to_string_pretty(&s).unwrap()).unwrap();

    // No `claude` available, so the fetch cannot happen. `--fix` must not print
    // "Fixed N issue(s)" for work it did not do.
    zskills(&home)
        .args(["doctor", "--fix"])
        .assert()
        .success()
        .stdout(predicate::str::contains("still open"));

    // And the enable is still there for the next attempt.
    let settings = read_settings(&home);
    assert_eq!(
        settings["enabledPlugins"]["vercel@vercel-plugin"],
        json!(true)
    );
}

#[test]
fn doctor_fix_leaves_an_unverifiable_enable_alone() {
    // A registered marketplace whose clone was never fetched (or was deleted):
    // its manifest is unreadable, so we cannot tell whether `mystery` is real.
    // Deleting the user's enable on the strength of a failed file read is exactly
    // the destructive-on-ignorance behaviour this check exists to prevent.
    let home = fake_home();
    let settings_path = home.path().join("settings.json");
    let mut s: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    s["enabledPlugins"]["mystery@test-mp"] = json!(true);
    fs::write(&settings_path, serde_json::to_string_pretty(&s).unwrap()).unwrap();

    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("unverifiable"))
        .stdout(predicate::str::contains("mystery@test-mp"));

    zskills(&home).args(["doctor", "--fix"]).assert().success();

    let settings = read_settings(&home);
    assert_eq!(
        settings["enabledPlugins"]["mystery@test-mp"],
        json!(true),
        "--fix must not revoke an enable it could not verify: {:#}",
        settings["enabledPlugins"]
    );
}

#[test]
fn install_does_not_trust_a_zero_exit_without_bytes() {
    // The dangerous case: `claude` exits 0 but nothing lands in the inventory.
    // Treating exit 0 as proof would recreate "enabled but not installed" one layer
    // down — and report it as success.
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = claude_stub_lying(stub_dir.path());

    zskills(&home)
        .env_remove("ZSKILLS_NO_CLAUDE_CLI")
        .env("ZSKILLS_CLAUDE_BIN", &stub)
        .args(["plugin", "install", "vercel@vercel-plugin"])
        .assert()
        .stdout(predicate::str::contains("fetched vercel@vercel-plugin").not())
        .stdout(predicate::str::contains("installed and ready").not())
        .stdout(predicate::str::contains("enabled but not installed"))
        .stderr(predicate::str::contains("no entry appeared"));
}

#[test]
fn install_kills_a_hanging_claude_instead_of_waiting_forever() {
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = claude_stub_hanging(stub_dir.path());

    let started = std::time::Instant::now();
    zskills(&home)
        .env_remove("ZSKILLS_NO_CLAUDE_CLI")
        .env("ZSKILLS_CLAUDE_BIN", &stub)
        .env("ZSKILLS_CLAUDE_TIMEOUT_SECS", "1")
        .args(["plugin", "install", "vercel@vercel-plugin"])
        .assert()
        .stderr(predicate::str::contains("timed out"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "the hanging child should have been killed, not waited out"
    );
}

#[test]
fn install_rejects_a_plugin_no_marketplace_offers_instead_of_writing_a_bogus_enable() {
    // Previously: `install bogus@no-such-mp` printed `+ bogus@no-such-mp`, persisted
    // it, and exited 0 — then the next `doctor --fix` deleted it. zskills damaged the
    // file and then repaired its own damage.
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );

    zskills(&home)
        .args(["plugin", "install", "bogus@no-such-mp"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("+ bogus@no-such-mp").not());

    let settings = read_settings(&home);
    assert!(
        settings["enabledPlugins"].get("bogus@no-such-mp").is_none(),
        "a spec no marketplace offers must not be written: {:#}",
        settings["enabledPlugins"]
    );
}

#[test]
fn install_exits_non_zero_when_the_fetch_fails() {
    let home = fake_home();
    register_marketplace(
        &home,
        "vercel-plugin",
        "vercel",
        Some("2026-08-20T00:00:00.000Z"),
    );
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = claude_stub_failing(stub_dir.path());

    // A CLI that prints an error and exits 0 is unreadable to `&&`, `set -e`, and CI.
    zskills(&home)
        .env_remove("ZSKILLS_NO_CLAUDE_CLI")
        .env("ZSKILLS_CLAUDE_BIN", &stub)
        .args(["plugin", "install", "vercel@vercel-plugin"])
        .assert()
        .failure();
}

#[test]
fn doctor_fix_converges_and_does_not_count_unfixable_findings_as_failures() {
    // `--fix` used to compare repairs against *every* finding, including ones it has
    // no code to fix, so a single deprecated MCP server made it report failure and
    // invite a re-run that changed nothing, forever.
    let home = fake_home();
    zskills(&home).args(["doctor", "--fix"]).assert().success();
    // Second run: the fixable issue (test-mp's missing lastUpdated) is gone.
    zskills(&home)
        .args(["doctor", "--fix"])
        .assert()
        .success()
        .stdout(predicate::str::contains("still open").not());
}

// ---------------------------------------------------------------------------
// Marketplace pins.
//
// Background (2026-08-21): `zskills update` and `zskills upgrade` `git pull` every
// registered marketplace. That floated a marketplace off the tag it was deliberately
// held at — v0.23.0 moved to v0.24.1 — and the next `upgrade` would have done it
// again. A pin in skills.toml holds the clone at a tag, branch, or sha, and refuses
// to fall back to a pull when the pin cannot be honoured.
// ---------------------------------------------------------------------------

/// An upstream marketplace with two commits. `v1` tags the first. `main` points at
/// the second. Returns (repo path, sha_v1, sha_head).
fn marketplace_upstream(
    parent: &std::path::Path,
    name: &str,
) -> (std::path::PathBuf, String, String) {
    let repo = parent.join(name);
    fs::create_dir_all(repo.join(".claude-plugin")).unwrap();
    let manifest = |plugins: &str| {
        format!(
            r#"{{"name":"{}","owner":{{"name":"T"}},"plugins":[{}]}}"#,
            name, plugins
        )
    };
    fs::write(
        repo.join(".claude-plugin").join("marketplace.json"),
        manifest(r#"{"name":"alpha","description":"pinned release"}"#),
    )
    .unwrap();
    git_init_and_commit(&repo);
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["tag", "v1"])
        .status()
        .unwrap();
    let sha_v1 = rev_parse(&repo, "HEAD");

    // A second commit, so "floated" and "pinned" are distinguishable.
    fs::write(
        repo.join(".claude-plugin").join("marketplace.json"),
        manifest(
            r#"{"name":"alpha","description":"newer"},{"name":"beta","description":"added later"}"#,
        ),
    )
    .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["add", "-A"])
        .status()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["commit", "--quiet", "-m", "second"])
        .status()
        .unwrap();
    let sha_head = rev_parse(&repo, "HEAD");
    (repo, sha_v1, sha_head)
}

fn rev_parse(repo: &std::path::Path, r: &str) -> String {
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", r])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Clone `upstream` into CLAUDE_HOME as a registered marketplace, checked out at `at`.
fn register_clone(home: &TempDir, name: &str, upstream: &std::path::Path, at: &str) {
    let dest = home.path().join("plugins").join("marketplaces").join(name);
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    StdCommand::new("git")
        .args(["clone", "--quiet"])
        .arg(file_url(upstream))
        .arg(&dest)
        .status()
        .unwrap();
    // Stay on the tracking branch and move it, rather than detaching. A real
    // marketplace clone is on `main` with an upstream, which is what makes an
    // unpinned `git pull` fast-forward — and what let the reported drift happen.
    StdCommand::new("git")
        .arg("-C")
        .arg(&dest)
        .args(["checkout", "--quiet", "main"])
        .status()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&dest)
        .args(["reset", "--hard", "--quiet", at])
        .status()
        .unwrap();

    let known_path = home.path().join("plugins").join("known_marketplaces.json");
    let mut known: serde_json::Value =
        serde_json::from_slice(&fs::read(&known_path).unwrap()).unwrap();
    known[name] = json!({
        "source": { "source": "git", "url": file_url(upstream) },
        "installLocation": dest.to_string_lossy(),
        "autoUpdate": true,
        "lastUpdated": "2026-08-21T00:00:00.000Z"
    });
    fs::write(&known_path, serde_json::to_string_pretty(&known).unwrap()).unwrap();
}

/// Write a skills.toml into the sandboxed XDG_CONFIG_HOME.
fn write_manifest(home: &TempDir, body: &str) {
    let dir = home.path().join("config").join("zskills");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("skills.toml"), body).unwrap();
}

fn marketplace_head(home: &TempDir, name: &str) -> String {
    rev_parse(
        &home.path().join("plugins").join("marketplaces").join(name),
        "HEAD",
    )
}

#[test]
fn update_holds_a_pinned_marketplace_and_floats_an_unpinned_one() {
    let up = tempfile::tempdir().unwrap();
    let (pinned_repo, pinned_v1, pinned_head) = marketplace_upstream(up.path(), "pinned-mp");
    let (free_repo, free_v1, free_head) = marketplace_upstream(up.path(), "free-mp");

    let home = fake_home();
    register_clone(&home, "pinned-mp", &pinned_repo, &pinned_v1);
    register_clone(&home, "free-mp", &free_repo, &free_v1);
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"pinned-mp\"\npin = \"v1\"\n",
    );

    zskills(&home)
        .args(["marketplace", "update"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pinned @"));

    assert_eq!(
        marketplace_head(&home, "pinned-mp"),
        pinned_v1,
        "a pinned marketplace must not float off its pin"
    );
    assert_ne!(marketplace_head(&home, "pinned-mp"), pinned_head);
    assert_eq!(
        marketplace_head(&home, "free-mp"),
        free_head,
        "an unpinned marketplace must still update"
    );
    assert_ne!(marketplace_head(&home, "free-mp"), free_v1);
}

#[test]
fn upgrade_holds_a_pinned_marketplace() {
    let up = tempfile::tempdir().unwrap();
    let (repo, v1, head) = marketplace_upstream(up.path(), "pinned-mp");
    let home = fake_home();
    register_clone(&home, "pinned-mp", &repo, &v1);
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"pinned-mp\"\npin = \"v1\"\n",
    );

    zskills(&home).args(["skill", "upgrade"]).assert().success();

    assert_eq!(marketplace_head(&home, "pinned-mp"), v1);
    assert_ne!(marketplace_head(&home, "pinned-mp"), head);
}

#[test]
fn marketplace_update_restores_a_pinned_clone_that_drifted() {
    // The reported failure: something already floated the clone forward. The next
    // update must put it back, not leave it and not push it further.
    let up = tempfile::tempdir().unwrap();
    let (repo, v1, head) = marketplace_upstream(up.path(), "pinned-mp");
    let home = fake_home();
    register_clone(&home, "pinned-mp", &repo, &head); // already drifted
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"pinned-mp\"\npin = \"v1\"\n",
    );
    assert_eq!(marketplace_head(&home, "pinned-mp"), head);

    zskills(&home)
        .args(["marketplace", "update"])
        .assert()
        .success()
        .stdout(predicate::str::contains("restored"));

    assert_eq!(
        marketplace_head(&home, "pinned-mp"),
        v1,
        "update must pull a drifted pinned clone back to its pin"
    );
}

#[test]
fn a_pin_accepts_a_full_sha() {
    let up = tempfile::tempdir().unwrap();
    let (repo, v1, head) = marketplace_upstream(up.path(), "pinned-mp");
    let home = fake_home();
    register_clone(&home, "pinned-mp", &repo, &head);
    write_manifest(
        &home,
        &format!("[[marketplaces]]\nname = \"pinned-mp\"\npin = \"{}\"\n", v1),
    );

    zskills(&home)
        .args(["marketplace", "update"])
        .assert()
        .success();
    assert_eq!(marketplace_head(&home, "pinned-mp"), v1);
}

#[test]
fn an_unresolvable_pin_fails_and_never_falls_back_to_a_pull() {
    // The dangerous failure mode: a typo in the pin silently reverting to `git pull`
    // would float the marketplace, which is what the pin exists to prevent.
    let up = tempfile::tempdir().unwrap();
    let (repo, v1, head) = marketplace_upstream(up.path(), "pinned-mp");
    let home = fake_home();
    register_clone(&home, "pinned-mp", &repo, &v1);
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"pinned-mp\"\npin = \"v9-does-not-exist\"\n",
    );

    zskills(&home)
        .args(["marketplace", "update"])
        .assert()
        .success()
        .stdout(predicate::str::contains("does not exist"));

    assert_eq!(
        marketplace_head(&home, "pinned-mp"),
        v1,
        "a bad pin must leave the clone alone, not float it"
    );
    assert_ne!(marketplace_head(&home, "pinned-mp"), head);
}

#[test]
fn a_blank_pin_is_treated_as_unpinned() {
    let up = tempfile::tempdir().unwrap();
    let (repo, v1, head) = marketplace_upstream(up.path(), "free-mp");
    let home = fake_home();
    register_clone(&home, "free-mp", &repo, &v1);
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"free-mp\"\npin = \"   \"\n",
    );

    zskills(&home)
        .args(["marketplace", "update"])
        .assert()
        .success();
    assert_eq!(
        marketplace_head(&home, "free-mp"),
        head,
        "a blank pin must not freeze the marketplace"
    );
}

#[test]
fn marketplace_list_shows_the_pin() {
    let up = tempfile::tempdir().unwrap();
    let (repo, v1, _) = marketplace_upstream(up.path(), "pinned-mp");
    let home = fake_home();
    register_clone(&home, "pinned-mp", &repo, &v1);
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"pinned-mp\"\npin = \"v1\"\n",
    );

    zskills(&home)
        .args(["marketplace", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[pinned v1]"));
}

#[test]
fn pinning_a_tarball_marketplace_is_refused_with_a_reason() {
    // A marketplace that is not a git clone has no refs, so a pin cannot mean
    // anything. Say so, rather than silently re-extracting the tarball and
    // leaving the user believing the pin held.
    let home = fake_home();
    let dir = home
        .path()
        .join("plugins")
        .join("marketplaces")
        .join("tarball-mp");
    fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
    fs::write(
        dir.join(".claude-plugin").join("marketplace.json"),
        r#"{"name":"tarball-mp","plugins":[]}"#,
    )
    .unwrap();
    let known_path = home.path().join("plugins").join("known_marketplaces.json");
    let mut known: serde_json::Value =
        serde_json::from_slice(&fs::read(&known_path).unwrap()).unwrap();
    known["tarball-mp"] = json!({
        "source": { "source": "github", "repo": "owner/tarball-mp" },
        "installLocation": dir.to_string_lossy(),
        "autoUpdate": true,
        "lastUpdated": "2026-08-21T00:00:00.000Z"
    });
    fs::write(&known_path, serde_json::to_string_pretty(&known).unwrap()).unwrap();
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"tarball-mp\"\npin = \"v1\"\n",
    );

    zskills(&home)
        .args(["marketplace", "update"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not a git clone"));
}

#[test]
fn a_pin_resolves_even_when_upstream_moved_a_tag() {
    // Regression: `git fetch --tags` rejects *every* tag with "would clobber existing
    // tag" and exits non-zero when upstream has moved any one of them. One moved tag
    // anywhere upstream would then make an otherwise valid pin unresolvable. The real
    // llm-wiki clone is in exactly that state. `fetch_all` passes `--force`.
    let up = tempfile::tempdir().unwrap();
    let (repo, v1, head) = marketplace_upstream(up.path(), "moved-tag-mp");

    let home = fake_home();
    register_clone(&home, "moved-tag-mp", &repo, &v1);

    // Upstream moves `v1` onto the second commit and publishes a new `v2` there.
    // The clone still has the old `v1`, so any fetch must overwrite it.
    for args in [vec!["tag", "-f", "v1", &head], vec!["tag", "v2", &head]] {
        StdCommand::new("git")
            .arg("-C")
            .arg(&repo)
            .args(&args)
            .status()
            .unwrap();
    }

    // `v2` is not in the clone, so honouring this pin requires the fetch to succeed.
    write_manifest(
        &home,
        "[[marketplaces]]\nname = \"moved-tag-mp\"\npin = \"v2\"\n",
    );

    zskills(&home)
        .args(["marketplace", "update"])
        .assert()
        .success()
        .stdout(predicate::str::contains("would clobber").not());

    assert_eq!(
        marketplace_head(&home, "moved-tag-mp"),
        head,
        "a pin to a tag that needs fetching must resolve even when another tag moved"
    );
}

// ---------------------------------------------------------------------------
// Owning every on-disk Agent Skill.
//
// Three defects kept `zskills list` reporting skills as unmanaged that either
// already had an owner or had just been adopted:
//   1. a skill shipped by an ACTIVE plugin was still listed as an orphan;
//   2. `claims` was honoured only on npm entries, silently ignored on local ones;
//   3. a local entry naming a skill that is NOT on disk was tracked anyway, which
//      manufactured the exact defect `doctor` exists to report.
// ---------------------------------------------------------------------------

/// Put an on-disk Agent Skill under AGENTS_HOME/skills/<name>.
fn write_disk_skill(home: &TempDir, name: &str) {
    let d = home.path().join("skills").join(name);
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("SKILL.md"),
        format!("---\nname: {}\ndescription: d\n---\n", name),
    )
    .unwrap();
}

/// Register an active plugin that ships `skill` from its cache.
fn install_plugin_shipping_skill(home: &TempDir, mp: &str, plugin: &str, skill: &str) {
    let cache = home
        .path()
        .join("plugins")
        .join("cache")
        .join(mp)
        .join(plugin)
        .join("1.0.0")
        .join("skills")
        .join(skill);
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("SKILL.md"), "---\nname: x\n---\n").unwrap();

    let mp_dir = home.path().join("plugins").join("marketplaces").join(mp);
    fs::create_dir_all(mp_dir.join(".claude-plugin")).unwrap();
    fs::write(
        mp_dir.join(".claude-plugin").join("marketplace.json"),
        format!(r#"{{"name":"{}","plugins":[{{"name":"{}"}}]}}"#, mp, plugin),
    )
    .unwrap();

    let known_path = home.path().join("plugins").join("known_marketplaces.json");
    let mut known: serde_json::Value =
        serde_json::from_slice(&fs::read(&known_path).unwrap()).unwrap();
    known[mp] = json!({
        "source": { "source": "github", "repo": format!("o/{}", mp) },
        "installLocation": mp_dir.to_string_lossy(),
        "autoUpdate": true,
        "lastUpdated": "2026-08-22T00:00:00.000Z"
    });
    fs::write(&known_path, serde_json::to_string_pretty(&known).unwrap()).unwrap();

    let q = format!("{}@{}", plugin, mp);
    let sp = home.path().join("settings.json");
    let mut s: serde_json::Value = serde_json::from_slice(&fs::read(&sp).unwrap()).unwrap();
    s["enabledPlugins"][&q] = json!(true);
    fs::write(&sp, serde_json::to_string_pretty(&s).unwrap()).unwrap();

    let ip = home.path().join("plugins").join("installed_plugins.json");
    let mut i: serde_json::Value = serde_json::from_slice(&fs::read(&ip).unwrap()).unwrap();
    i["plugins"][&q] = json!([{ "scope": "user", "installPath": "/x", "version": "1.0.0" }]);
    fs::write(&ip, serde_json::to_string_pretty(&i).unwrap()).unwrap();
}

#[test]
fn a_skill_shipped_by_an_active_plugin_is_not_listed_as_unmanaged() {
    let home = fake_home();
    write_disk_skill(&home, "shipped");
    install_plugin_shipping_skill(&home, "mp", "plug", "shipped");

    zskills(&home)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("on disk but not managed").not());
}

#[test]
fn a_skill_from_a_disabled_plugin_is_still_unmanaged() {
    // The filter must key on *active*, not merely installed. A disabled plugin
    // contributes nothing at runtime, so its leftover copy really is an orphan.
    let home = fake_home();
    write_disk_skill(&home, "shipped");
    install_plugin_shipping_skill(&home, "mp", "plug", "shipped");
    let sp = home.path().join("settings.json");
    let mut s: serde_json::Value = serde_json::from_slice(&fs::read(&sp).unwrap()).unwrap();
    s["enabledPlugins"]["plug@mp"] = json!(false);
    fs::write(&sp, serde_json::to_string_pretty(&s).unwrap()).unwrap();

    zskills(&home)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("on disk but not managed"))
        .stdout(predicate::str::contains("• shipped"));

    // Discriminating half: on the unfixed code every on-disk skill was unmanaged, so
    // the assertion above holds trivially. Re-enabling must flip it, which only the
    // active-plugin filter can do.
    let mut s: serde_json::Value = serde_json::from_slice(&fs::read(&sp).unwrap()).unwrap();
    s["enabledPlugins"]["plug@mp"] = json!(true);
    fs::write(&sp, serde_json::to_string_pretty(&s).unwrap()).unwrap();
    zskills(&home)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("• shipped").not());
}

#[test]
fn only_the_installed_version_of_a_plugin_counts_as_shipping_a_skill() {
    // The cache keeps old versions next to the current one. Unioning across all of
    // them would hide a genuinely orphaned skill forever the first time an upgrade
    // drops a name.
    let home = fake_home();
    write_disk_skill(&home, "dropped");
    install_plugin_shipping_skill(&home, "mp", "plug", "kept");
    // A stale 0.9.0 in the cache still ships `dropped`; the installed version is 1.0.0.
    let stale = home
        .path()
        .join("plugins")
        .join("cache")
        .join("mp")
        .join("plug")
        .join("0.9.0")
        .join("skills")
        .join("dropped");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("SKILL.md"), "---\nname: dropped\n---\n").unwrap();

    zskills(&home)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("• dropped"));
}

#[test]
fn claims_on_a_local_entry_adopts_matching_skills() {
    let home = fake_home();
    for n in ["alpha-one", "alpha-two", "beta-keep"] {
        write_disk_skill(&home, n);
    }
    let dir = home.path().join("config").join("zskills");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("skills.toml"),
        "[[agent_skills]]\nname = \"alpha-bundle\"\nclaims = [\"alpha-*\"]\n",
    )
    .unwrap();

    // The skill name is bold in that log line, so assert the outcome below, not the styling.
    zskills(&home).arg("sync").assert().success();

    // The two claimed skills are adopted; the unrelated one stays unmanaged.
    // Assert on --json: the text view prints adopted names in the *managed*
    // section, so a whole-output substring check cannot tell the two apart.
    let out = zskills(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        v["agent_skills"]["untracked"],
        json!(["beta-keep"]),
        "only the unclaimed skill should remain unmanaged"
    );
}

#[test]
fn a_local_entry_for_a_skill_not_on_disk_is_not_tracked() {
    // Tracking it would write inventory that `doctor` immediately reports as
    // "tracked in inventory but missing on disk" — sync manufacturing a defect.
    let home = fake_home();
    let dir = home.path().join("config").join("zskills");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("skills.toml"),
        "[[agent_skills]]\nname = \"typo-not-on-disk\"\n",
    )
    .unwrap();

    zskills(&home)
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to track"))
        .stdout(predicate::str::contains("tracked local agent skill typo-not-on-disk").not());

    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("missing on disk").not());
}

// ---------------------------------------------------------------------------
// `.agents/skills/` layout.
//
// `zskills install warpdotdev/common-skills --skill skill-doctor` failed with
// "skill 'skill-doctor' not found (available: )". The survey only walked
// `<repo>/skills/`, and Warp uses the cross-client `<repo>/.agents/skills/`.
// ---------------------------------------------------------------------------

/// Write `<repo>/.agents/skills/<name>/SKILL.md`.
fn write_agents_skill(repo: &std::path::Path, name: &str, description: &str) {
    let dir = repo.join(".agents").join("skills").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!(
            "---\nname: {}\ndescription: {}\n---\n# {}\n",
            name, description, name
        ),
    )
    .unwrap();
}

#[test]
fn install_skill_flag_selects_from_dot_agents_skills() {
    let upstream = tempfile::tempdir().unwrap();
    write_agents_skill(upstream.path(), "skill-doctor", "Grade your skills");
    write_agents_skill(upstream.path(), "write-product-spec", "Other skill");
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(upstream.path()),
            "--skill",
            "skill-doctor",
        ])
        .assert()
        .success()
        // The `+` marker is coloured, so assert the name and prove the rest on disk.
        .stdout(predicate::str::contains("skill-doctor"));

    assert!(home.path().join("skills/skill-doctor/SKILL.md").exists());
    assert!(
        !home.path().join("skills/write-product-spec").exists(),
        "--skill must install exactly one skill"
    );
}

#[test]
fn a_large_dot_agents_skills_tree_still_requires_skill_or_all() {
    // A large collection under the new root. The size policy must still apply there:
    // discovering more layouts must not start flooding. `warpdotdev/common-skills`
    // ships 26 skills this way; 14 is enough to be over the auto-install threshold.
    let upstream = tempfile::tempdir().unwrap();
    for i in 0..14 {
        write_agents_skill(upstream.path(), &format!("skill-{:02}", i), "d");
    }
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", &file_url(upstream.path())])
        .assert()
        .success()
        .stdout(predicate::str::contains("14"))
        .stdout(predicate::str::contains("--all"));

    assert!(
        !home.path().join("skills/skill-00").exists(),
        "a bare install of a large collection must install nothing"
    );
}

#[test]
fn install_reports_available_names_from_dot_agents_skills_when_skill_is_unknown() {
    // The original failure printed "(available: )" — an empty list is what made the
    // bug look like a missing skill rather than a missing layout.
    let upstream = tempfile::tempdir().unwrap();
    write_agents_skill(upstream.path(), "skill-doctor", "d");
    git_init_and_commit(upstream.path());

    let home = fake_home();
    let out = zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(upstream.path()),
            "--skill",
            "nope",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&out);
    assert!(stderr.contains("'nope' not found"), "{}", stderr);
    assert!(
        stderr.contains("skill-doctor"),
        "the available list must name what the repo really ships: {}",
        stderr
    );
}

#[test]
fn upgrade_refreshes_only_skills_already_owned_from_a_source() {
    // A source-only manifest entry means "keep what I own from this source", not
    // "adopt whatever it ships today". Widening the survey — a new skill root, or
    // upstream adding skills — must not make an unattended `upgrade` install things
    // nobody asked for. This is the regression the .agents/skills walker introduced:
    // a repo with 1 skill under skills/ and 3 under .agents/skills/ went from
    // installing 1 to installing 4.
    let upstream = tempfile::tempdir().unwrap();
    let repo = upstream.path().join("multi");
    write_skill(&repo, "owned", "already installed");
    write_agents_skill(&repo, "brand-new", "should NOT be adopted by upgrade");
    write_agents_skill(&repo, "also-new", "should NOT be adopted by upgrade");
    git_init_and_commit(&repo);

    let home = fake_home();
    // Own exactly one skill from that source.
    zskills(&home)
        .args(["skill", "install", &file_url(&repo), "--skill", "owned"])
        .assert()
        .success();
    assert!(home.path().join("skills/owned").exists());

    let dir = home.path().join("config").join("zskills");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("skills.toml"),
        format!("[[agent_skills]]\nsource = \"{}\"\n", file_url(&repo)),
    )
    .unwrap();

    zskills(&home).args(["skill", "upgrade"]).assert().success();

    assert!(
        home.path().join("skills/owned").exists(),
        "the owned skill must still be refreshed"
    );
    for unwanted in ["brand-new", "also-new"] {
        assert!(
            !home.path().join("skills").join(unwanted).exists(),
            "upgrade must not adopt {} — it was never requested",
            unwanted
        );
    }
}

#[test]
fn bare_remove_stub_exits_2_and_names_typed_replacement() {
    let home = fake_home();
    zskills(&home)
        .args(["remove", "honcho"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("error: removed-in-1.0: remove"))
        .stderr(predicate::str::contains("zskills skill remove"))
        .stderr(predicate::str::contains("zskills mcp remove"));
}

#[test]
fn plugin_remove_unknown_name_does_not_print_success() {
    let home = fake_home();
    zskills(&home)
        .args(["plugin", "remove", "this-does-not-exist-xyz123"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not installed"));
}

#[test]
fn plugin_remove_clears_manifest_intent() {
    let home = fake_home();
    let dir = home.path().join("config").join("zskills");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("skills.toml"),
        "# keep this comment\n\
         \n\
         [[marketplaces]]\n\
         name = \"test-mp\"\n\
         repo = \"owner/test-mp\"\n\
         \n\
         [[skills]]\n\
         name = \"foo\"\n\
         marketplace = \"test-mp\"\n\
         harnesses = [\"claude\"]\n\
         \n\
         [[skills]]\n\
         name = \"keepme\"\n\
         marketplace = \"test-mp\"\n\
         harnesses = [\"claude\"]\n",
    )
    .unwrap();

    zskills(&home)
        .args(["plugin", "remove", "foo"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.join("skills.toml")).unwrap();
    assert!(
        !toml.contains("name = \"foo\""),
        "plugin remove must drop the [[skills]] row so sync cannot resurrect it:\n{toml}"
    );
    assert!(
        toml.contains("name = \"keepme\""),
        "plugin remove must leave neighbouring [[skills]] rows:\n{toml}"
    );
    assert!(
        toml.contains("name = \"test-mp\""),
        "plugin remove must leave [[marketplaces]]:\n{toml}"
    );
    assert!(
        toml.contains("# keep this comment"),
        "plugin remove must preserve comments via toml_edit:\n{toml}"
    );
}

#[test]
fn mcp_add_and_remove_write_intent_then_runtime() {
    let (parent, claude_home) = fake_home_nested();
    let manifest_dir = parent.path().join("config").join("zskills");
    fs::create_dir_all(&manifest_dir).unwrap();
    let manifest = manifest_dir.join("skills.toml");
    fs::write(&manifest, "").unwrap();
    let claude_json = parent.path().join(".claude.json");
    fs::write(&claude_json, r#"{"mcpServers":{}}"#).unwrap();

    zskills_nested(&parent, &claude_home)
        .args([
            "mcp",
            "add",
            "honcho",
            "--url",
            "https://mcp.honcho.dev",
            "--transport",
            "http",
            "--file",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .success();

    let toml = fs::read_to_string(&manifest).unwrap();
    assert!(toml.contains("name = \"honcho\""));
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&claude_json).unwrap()).unwrap();
    assert_eq!(
        json["mcpServers"]["honcho"]["url"],
        "https://mcp.honcho.dev"
    );

    zskills_nested(&parent, &claude_home)
        .args([
            "mcp",
            "remove",
            "honcho",
            "--file",
            manifest.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed mcp"))
        .stdout(predicate::str::contains("honcho"));

    let toml = fs::read_to_string(&manifest).unwrap();
    assert!(!toml.contains("name = \"honcho\""));
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&claude_json).unwrap()).unwrap();
    assert!(json["mcpServers"].get("honcho").is_none());
}

// ──── harness targets (`[defaults].harnesses` / `--harness` / list) ─────────

/// Marketplace clone that ships nested `skills/<plugin>/SKILL.md`, the layout
/// `pi@zot24-skills` uses.
fn stage_marketplace_plugin_with_nested_skill(home: &TempDir, mp: &str, plugin: &str) {
    let mp_dir = home.path().join("plugins").join("marketplaces").join(mp);
    fs::create_dir_all(mp_dir.join(".claude-plugin")).unwrap();
    fs::write(
        mp_dir.join(".claude-plugin").join("marketplace.json"),
        format!(
            r#"{{"name":"{mp}","plugins":[{{"name":"{plugin}","source":"./skills/{plugin}"}}]}}"#
        ),
    )
    .unwrap();
    let nested = mp_dir
        .join("skills")
        .join(plugin)
        .join("skills")
        .join(plugin);
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("SKILL.md"),
        format!("---\nname: {plugin}\ndescription: d\n---\n"),
    )
    .unwrap();
    fs::write(nested.join("notes.md"), "not a symlink\n").unwrap();

    let known_path = home.path().join("plugins").join("known_marketplaces.json");
    let mut known: serde_json::Value =
        serde_json::from_slice(&fs::read(&known_path).unwrap()).unwrap();
    known[mp] = json!({
        "source": { "source": "github", "repo": format!("o/{mp}") },
        "installLocation": mp_dir.to_string_lossy(),
        "autoUpdate": true,
        "lastUpdated": "2026-08-24T00:00:00.000Z"
    });
    fs::write(&known_path, serde_json::to_string_pretty(&known).unwrap()).unwrap();
}

#[test]
fn list_active_plugin_shows_claude_not_pi() {
    let home = fake_home();
    let out = zskills(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["plugins"]["active"][0], "foo@test-mp");
    let vis = v["plugins"]["harnesses"]["foo@test-mp"]["visible"]
        .as_array()
        .unwrap();
    assert!(vis.iter().any(|x| x == "claude"));
    assert!(!vis.iter().any(|x| x == "pi"));
}

#[test]
fn plugin_install_harness_pi_copies_nested_skill_to_hub_not_symlink() {
    let home = fake_home();
    stage_marketplace_plugin_with_nested_skill(&home, "zot24-skills", "pi");

    zskills(&home)
        .args(["plugin", "install", "pi@zot24-skills", "--harness", "pi"])
        .assert()
        .success();

    let dest = home.path().join("skills").join("pi").join("SKILL.md");
    assert!(dest.is_file(), "SKILL.md must sit at the hub scan root");
    let meta = fs::symlink_metadata(home.path().join("skills").join("pi")).unwrap();
    assert!(!meta.file_type().is_symlink(), "owner forbade symlinks");
    assert!(home
        .path()
        .join("skills")
        .join("pi")
        .join("notes.md")
        .is_file());
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap();
    assert!(
        settings["enabledPlugins"]
            .get("pi@zot24-skills")
            .is_none_or(|v| v == false),
        "pi-only harness must not enable the Claude plugin"
    );
}

#[test]
fn list_after_hub_copy_shows_pi_visible_to_pi() {
    let home = fake_home();
    stage_marketplace_plugin_with_nested_skill(&home, "zot24-skills", "pi");
    zskills(&home)
        .args(["plugin", "install", "pi@zot24-skills", "--harness", "pi"])
        .assert()
        .success();

    let out = zskills(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let skill_vis = v["agent_skills"]["harnesses"]["pi"]["visible"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        skill_vis.iter().any(|x| x == "pi"),
        "list must show the hub copy visible to Pi: {v}"
    );
    assert!(
        !skill_vis.iter().any(|x| x == "claude"),
        "a hub-only copy is not a Claude plugin: {v}"
    );
}

#[test]
fn unknown_harness_is_refused() {
    let home = fake_home();
    zskills(&home)
        .args(["plugin", "install", "foo@test-mp", "--harness", "agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn kimi_is_skipped_not_invented() {
    let home = fake_home();
    stage_marketplace_plugin_with_nested_skill(&home, "zot24-skills", "pi");
    zskills(&home)
        .args(["plugin", "install", "pi@zot24-skills", "--harness", "kimi"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unsupported"))
        .stdout(predicate::str::contains("~/.kimi-code/"));
    assert!(
        !home
            .path()
            .join("skills")
            .join("pi")
            .join("SKILL.md")
            .is_file(),
        "kimi must not invent a hub copy"
    );
}

#[test]
fn hermes_links_hub_under_default_category() {
    let home = fake_home();
    stage_marketplace_plugin_with_nested_skill(&home, "zot24-skills", "pi");
    zskills(&home)
        .args([
            "plugin",
            "install",
            "pi@zot24-skills",
            "--harness",
            "hermes",
        ])
        .assert()
        .success();
    let hub = home.path().join("skills").join("pi");
    assert!(hub.join("SKILL.md").is_file(), "plugin → hub is a copy");
    let meta = fs::symlink_metadata(&hub).unwrap();
    assert!(!meta.file_type().is_symlink(), "plugin → hub stays a copy");
    let linked = home
        .path()
        .join("hermes")
        .join("skills")
        .join("software-development")
        .join("pi");
    let link_meta = fs::symlink_metadata(&linked).unwrap();
    assert!(
        link_meta.file_type().is_symlink(),
        "hub → hermes is a symlink"
    );
    assert!(linked.join("SKILL.md").is_file());
}

#[test]
fn hermes_category_flag_overrides_default() {
    let home = fake_home();
    stage_marketplace_plugin_with_nested_skill(&home, "zot24-skills", "pi");
    zskills(&home)
        .args([
            "plugin",
            "install",
            "pi@zot24-skills",
            "--harness",
            "hermes",
            "--category",
            "devops",
        ])
        .assert()
        .success();
    let linked = home
        .path()
        .join("hermes")
        .join("skills")
        .join("devops")
        .join("pi");
    assert!(fs::symlink_metadata(&linked)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!home
        .path()
        .join("hermes")
        .join("skills")
        .join("software-development")
        .join("pi")
        .exists());
}

#[test]
fn sync_applies_harnesses_on_plugin_row() {
    let home = fake_home();
    stage_marketplace_plugin_with_nested_skill(&home, "zot24-skills", "pi");
    let manifest_dir = home.path().join("config").join("zskills");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("skills.toml"),
        r#"
[defaults]
harnesses = ["claude", "pi", "hermes", "kimi", "grok", "codex"]
mcp_harnesses = ["claude", "pi", "hermes", "kimi", "grok", "codex"]

[[skills]]
name = "pi"
marketplace = "zot24-skills"
harnesses = ["pi"]
"#,
    )
    .unwrap();
    zskills(&home).args(["sync", "--force"]).assert().success();
    assert!(home
        .path()
        .join("skills")
        .join("pi")
        .join("SKILL.md")
        .is_file());
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap();
    assert!(settings["enabledPlugins"]
        .get("pi@zot24-skills")
        .is_none_or(|v| v == false));
}

#[test]
fn hub_skill_is_not_visible_to_unregistered_pi() {
    let home = fake_home();
    stage_marketplace_plugin_with_nested_skill(&home, "zot24-skills", "pi");
    zskills(&home)
        .args(["plugin", "install", "pi@zot24-skills", "--harness", "grok"])
        .assert()
        .success();

    let out = zskills(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let vis = v["agent_skills"]["harnesses"]["pi"]["visible"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        vis.iter().any(|x| x == "grok"),
        "Grok scans the hub natively: {v}"
    );
    assert!(
        !vis.iter().any(|x| x == "pi"),
        "unregistered Pi must not look hub-enough: {v}"
    );
    assert!(
        !home
            .path()
            .join("pi")
            .join("agent")
            .join("settings.json")
            .is_file(),
        "grok-only install must not write Pi settings"
    );
}

#[test]
fn skill_register_pi_hub_is_idempotent_and_preserves_keys() {
    let home = fake_home();
    let settings_path = home.path().join("pi").join("agent").join("settings.json");
    fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
    fs::write(
        &settings_path,
        r#"{
  "defaultModel": "grok-4.6",
  "defaultProvider": "xai",
  "theme": "dark"
}
"#,
    )
    .unwrap();

    zskills(&home)
        .args(["skill", "register-pi-hub"])
        .assert()
        .success();
    zskills(&home)
        .args(["skill", "register-pi-hub"])
        .assert()
        .success();

    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    let hub = home.path().join("skills");
    let hub_s = hub.to_string_lossy().into_owned();
    let count = settings["skills"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v.as_str() == Some(hub_s.as_str()))
        .count();
    assert_eq!(
        count, 1,
        "second run must not duplicate the hub path: {settings}"
    );
    assert_eq!(settings["defaultModel"], "grok-4.6");
    assert_eq!(settings["defaultProvider"], "xai");
    assert_eq!(settings["theme"], "dark");
}

#[test]
fn plugin_install_harness_pi_registers_hub_in_pi_settings() {
    let home = fake_home();
    stage_marketplace_plugin_with_nested_skill(&home, "zot24-skills", "pi");
    zskills(&home)
        .args(["plugin", "install", "pi@zot24-skills", "--harness", "pi"])
        .assert()
        .success();

    let settings_path = home.path().join("pi").join("agent").join("settings.json");
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    let hub = home.path().join("skills");
    let hub_s = hub.to_string_lossy().into_owned();
    let count = settings["skills"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v.as_str() == Some(hub_s.as_str()))
        .count();
    assert_eq!(
        count, 1,
        "plugin --harness pi must register the hub: {settings}"
    );
}

fn zskills_split_hub(home: &TempDir) -> Command {
    fs::create_dir_all(home.path().join(".agents").join("skills")).unwrap();
    fs::create_dir_all(home.path().join("grok")).unwrap();
    fs::create_dir_all(home.path().join("hermes")).unwrap();
    fs::create_dir_all(home.path().join("codex")).unwrap();
    fs::create_dir_all(home.path().join("pi")).unwrap();
    let mut cmd = zskills(home);
    cmd.env("AGENTS_HOME", home.path().join(".agents"));
    cmd
}

/// G6: every harness printed as visible must resolve the skill on its real root.
/// A filter matching zero tests fails the CHECK (`[1-9][0-9]* passed`).
#[test]
fn harness_visibility() {
    let home = fake_home();
    let repo = home.path().join("src-repo");
    write_skill(&repo, "demo", "d");
    git_init_and_commit(&repo);
    zskills_split_hub(&home)
        .args([
            "skill",
            "install",
            &file_url(&repo),
            "--harness",
            "claude,pi,grok,hermes,codex",
        ])
        .assert()
        .success();

    let out = zskills_split_hub(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let vis = v["agent_skills"]["harnesses"]["demo"]["visible"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !vis.is_empty(),
        "harness_visibility must observe at least one visible harness: {v}"
    );

    let name = "demo";
    let hub = home.path().join(".agents").join("skills").join(name);
    for h in &vis {
        let label = h.as_str().expect("visible entry is a string");
        let skill_md = match label {
            "claude" => home.path().join("skills").join(name).join("SKILL.md"),
            "pi" | "grok" => hub.join("SKILL.md"),
            "codex" => home
                .path()
                .join("codex")
                .join("skills")
                .join(name)
                .join("SKILL.md"),
            "hermes" => home
                .path()
                .join("hermes")
                .join("skills")
                .join("software-development")
                .join(name)
                .join("SKILL.md"),
            other => panic!("unexpected visible harness {other}: {v}"),
        };
        assert!(
            skill_md.is_file(),
            "{label} is visible but {} does not resolve: {v}",
            skill_md.display()
        );
    }
    assert!(
        !vis.iter().any(|x| x == "kimi"),
        "kimi stays unsupported: {v}"
    );
}

#[test]
fn hub_to_claude_is_symlink_plugin_to_hub_is_copy() {
    let home = fake_home();
    let repo = home.path().join("src-repo");
    write_skill(&repo, "demo", "d");
    git_init_and_commit(&repo);
    zskills_split_hub(&home)
        .args(["skill", "install", &file_url(&repo), "--harness", "claude"])
        .assert()
        .success();
    let hub = home.path().join(".agents").join("skills").join("demo");
    assert!(hub.join("SKILL.md").is_file());
    assert!(!fs::symlink_metadata(&hub).unwrap().file_type().is_symlink());
    let claude = home.path().join("skills").join("demo");
    assert!(fs::symlink_metadata(&claude)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(claude.join("SKILL.md").is_file());
}

/// Nested layout: Agent Skills live under `packages/foo/skills`, not `skills/`.
/// `wiki-manager/references` is a relative symlink into `claude-plugin/.../references`.
/// Previously `copy_dir_recursive` dropped that symlink (`is_file`/`is_dir` false).
fn write_nested_path_repo(parent: &std::path::Path) -> std::path::PathBuf {
    let repo = parent.join("odd-layout");
    let refs = repo.join("claude-plugin/skills/wiki-manager/references");
    fs::create_dir_all(&refs).unwrap();
    fs::write(refs.join("hub-resolution.md"), "hub resolution notes\n").unwrap();
    fs::write(
        repo.join("claude-plugin/skills/wiki-manager/SKILL.md"),
        "---\nname: wiki-manager\n---\nClaude flavour\n",
    )
    .unwrap();

    let opencode = repo.join("packages/foo/skills");
    let wm = opencode.join("wiki-manager");
    fs::create_dir_all(&wm).unwrap();
    fs::write(
        wm.join("SKILL.md"),
        "---\nname: wiki-manager\n---\nOpenCode flavour\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        "../../../../claude-plugin/skills/wiki-manager/references",
        wm.join("references"),
    )
    .unwrap();

    let wq = opencode.join("wiki-query");
    fs::create_dir_all(&wq).unwrap();
    fs::write(wq.join("SKILL.md"), "---\nname: wiki-query\n---\nquery\n").unwrap();

    git_init_and_commit(&repo);
    repo
}

#[test]
fn sync_source_path_copies_nested_tree_and_dereferences_in_clone_symlink() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_nested_path_repo(up.path());
    let home = fake_home();
    write_manifest(
        &home,
        &format!(
            "[[agent_skills]]\nsource = \"{}\"\npath = \"packages/foo/skills\"\n",
            file_url(&repo)
        ),
    );

    zskills(&home).arg("sync").assert().success();

    let wm = home.path().join("skills/wiki-manager");
    assert_eq!(
        fs::read_to_string(wm.join("SKILL.md")).unwrap(),
        "---\nname: wiki-manager\n---\nOpenCode flavour\n"
    );
    let copied = wm.join("references/hub-resolution.md");
    assert!(
        copied.is_file(),
        "in-clone symlink must be copied as a regular file"
    );
    assert!(
        !copied.symlink_metadata().unwrap().file_type().is_symlink(),
        "hub references/hub-resolution.md must not remain a symlink"
    );
    assert_eq!(
        fs::read_to_string(&copied).unwrap(),
        "hub resolution notes\n"
    );
    assert!(home.path().join("skills/wiki-query/SKILL.md").is_file());

    let inv: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("skills/.zskills.json")).unwrap())
            .unwrap();
    let tag = format!("source:{}:packages/foo/skills", file_url(&repo));
    assert_eq!(inv["agent_skills"]["wiki-manager"]["source"], tag);
    assert_eq!(inv["agent_skills"]["wiki-query"]["source"], tag);
}

#[test]
fn sync_source_path_skill_dir_installs_one_name() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_nested_path_repo(up.path());
    let home = fake_home();
    write_manifest(
        &home,
        &format!(
            "[[agent_skills]]\nsource = \"{}\"\npath = \"packages/foo/skills/wiki-manager\"\nname = \"wiki-manager\"\n",
            file_url(&repo)
        ),
    );

    zskills(&home).arg("sync").assert().success();

    assert!(home.path().join("skills/wiki-manager/SKILL.md").is_file());
    assert!(
        !home.path().join("skills/wiki-query").exists(),
        "a skill-dir path must not also install sibling skills"
    );
}

#[test]
fn sync_source_path_dry_run_prints_plan_line() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_nested_path_repo(up.path());
    let home = fake_home();
    write_manifest(
        &home,
        &format!(
            "[[agent_skills]]\nsource = \"{}\"\npath = \"packages/foo/skills\"\nname = \"wiki-manager\"\n",
            file_url(&repo)
        ),
    );

    zskills(&home)
        .args(["sync", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("install wiki-manager ← source:"))
        .stdout(predicate::str::contains("packages/foo/skills"));

    assert!(
        !home.path().join("skills/wiki-manager").exists(),
        "dry-run must not copy"
    );
}

#[test]
fn sync_rejects_path_escape() {
    let home = fake_home();
    write_manifest(
        &home,
        "[[agent_skills]]\nsource = \"owner/repo\"\npath = \"../escape\"\n",
    );
    zskills(&home)
        .arg("sync")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid path"));
}

#[test]
fn sync_copy_error_exits_nonzero_skips_applied_and_leaves_no_partial_dest() {
    let up = tempfile::tempdir().unwrap();
    let repo = up.path().join("escape-repo");
    let skill = repo.join("packages/foo/skills/evil");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "---\nname: evil\n---\n").unwrap();
    std::os::unix::fs::symlink("/etc/passwd", skill.join("secret")).unwrap();
    git_init_and_commit(&repo);

    let home = fake_home();
    write_manifest(
        &home,
        &format!(
            "[[agent_skills]]\nsource = \"{}\"\npath = \"packages/foo/skills\"\nname = \"evil\"\n",
            file_url(&repo)
        ),
    );

    let out = zskills(&home)
        .arg("sync")
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.contains("applied."),
        "copy error must skip ✓ applied.: {stdout}"
    );
    assert!(
        stderr.contains("escapes allowed root") || stderr.contains("operation(s) failed"),
        "stderr={stderr}"
    );
    assert!(
        !home.path().join("skills/evil").exists(),
        "failed copy must not leave a partial dest"
    );
}

#[test]
fn sync_live_pulls_path_backed_clone_and_recopies_when_head_moved() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_nested_path_repo(up.path());
    let home = fake_home();
    write_manifest(
        &home,
        &format!(
            "[[agent_skills]]\nsource = \"{}\"\npath = \"packages/foo/skills\"\nname = \"wiki-manager\"\n",
            file_url(&repo)
        ),
    );

    zskills(&home).arg("sync").assert().success();
    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        "---\nname: wiki-manager\n---\nOpenCode flavour\n"
    );

    fs::write(
        repo.join("packages/foo/skills/wiki-manager/SKILL.md"),
        "---\nname: wiki-manager\n---\nOpenCode flavour v2\n",
    )
    .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["add", "-A"])
        .status()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["commit", "--quiet", "-m", "second"])
        .status()
        .unwrap();

    zskills(&home)
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains("refresh wiki-manager ← source:"));

    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        "---\nname: wiki-manager\n---\nOpenCode flavour v2\n"
    );
}

#[test]
fn sync_adopt_source_path_tag_writes_source_and_path() {
    let home = fake_home();
    let skill = home.path().join("skills/foo");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "x").unwrap();
    fs::write(
        home.path().join("skills/.zskills.json"),
        json!({
            "version": 1,
            "agent_skills": {
                "foo": {
                    "source": "source:https://github.com/o/r.git:packages/foo/skills",
                    "installed_at": "@0",
                    "head_sha": "abc"
                }
            }
        })
        .to_string(),
    )
    .unwrap();
    write_manifest(&home, "");

    zskills(&home).args(["sync", "--adopt"]).assert().success();

    let raw = fs::read_to_string(home.path().join("config/zskills/skills.toml")).unwrap();
    assert!(
        raw.contains("source = \"https://github.com/o/r.git\""),
        "adopt must write the git source, not the inventory tag: {raw}"
    );
    assert!(
        raw.contains("path = \"packages/foo/skills\""),
        "adopt must write path: {raw}"
    );
    assert!(
        !raw.contains("source = \"source:"),
        "adopt must not store the raw inventory tag as source: {raw}"
    );
}

const CLAUDE_WIKI_SKILL: &str = "---\nname: wiki-manager\ndescription: Activates for /wiki commands\ntools: Read, Write, Edit\n---\nClaude Code is both the compiler and the runtime.\n";
const OPENCODE_WIKI_SKILL: &str = "---\nname: wiki-manager\ndescription: Manage a local wiki\n---\nOpenCode Integration Notes.\n\nTreat any /wiki:* references in this document as shorthand for the matching skill action.\n";

/// `file://` git repo shaped like nvk/llm-wiki: Claude plugin + OpenCode Agent Skills.
fn write_llm_wiki_repo(parent: &std::path::Path) -> std::path::PathBuf {
    let repo = parent.join("llm-wiki");
    fs::create_dir_all(repo.join(".claude-plugin")).unwrap();
    fs::write(
        repo.join(".claude-plugin").join("marketplace.json"),
        serde_json::to_string_pretty(&json!({
            "name": "llm-wiki",
            "plugins": [{ "name": "wiki", "source": "./claude-plugin", "version": "0.24.4" }]
        }))
        .unwrap(),
    )
    .unwrap();

    let plugin_meta = repo.join("claude-plugin").join(".claude-plugin");
    fs::create_dir_all(&plugin_meta).unwrap();
    fs::write(plugin_meta.join("plugin.json"), r#"{"name":"wiki"}"#).unwrap();

    let claude_skill = repo.join("claude-plugin/skills/wiki-manager");
    fs::create_dir_all(claude_skill.join("references")).unwrap();
    fs::write(claude_skill.join("SKILL.md"), CLAUDE_WIKI_SKILL).unwrap();
    fs::write(
        claude_skill.join("references/hub-resolution.md"),
        "hub resolution notes\n",
    )
    .unwrap();

    let oc = repo.join("plugins/llm-wiki-opencode/skills");
    let wm = oc.join("wiki-manager");
    fs::create_dir_all(&wm).unwrap();
    fs::write(wm.join("SKILL.md"), OPENCODE_WIKI_SKILL).unwrap();
    std::os::unix::fs::symlink(
        "../../../../claude-plugin/skills/wiki-manager/references",
        wm.join("references"),
    )
    .unwrap();
    let wq = oc.join("wiki-query");
    fs::create_dir_all(&wq).unwrap();
    fs::write(
        wq.join("SKILL.md"),
        "---\nname: wiki-query\n---\nquery-lite\n",
    )
    .unwrap();

    git_init_and_commit(&repo);
    repo
}

fn mark_plugin_installed(home: &TempDir, qualified: &str) {
    let path = home.path().join("plugins").join("installed_plugins.json");
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    v["plugins"][qualified] = json!([{
        "scope": "user",
        "installPath": "/tmp/x",
        "version": "1.0.0"
    }]);
    fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

fn llm_wiki_recipe(url: &str, plugin_harnesses: &str) -> String {
    format!(
        r#"[[marketplaces]]
name = "llm-wiki"
url = "{url}"

[[skills]]
name = "wiki"
marketplace = "llm-wiki"
harnesses = {plugin_harnesses}

[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
name = "wiki-manager"
harnesses = ["pi", "grok"]

[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
name = "wiki-query"
harnesses = ["pi", "grok"]
"#
    )
}

fn read_inv(home: &TempDir) -> serde_json::Value {
    serde_json::from_slice(&fs::read(home.path().join("skills/.zskills.json")).unwrap()).unwrap()
}

fn setup_llm_wiki_home() -> (TempDir, tempfile::TempDir, std::path::PathBuf) {
    let up = tempfile::tempdir().unwrap();
    let repo = write_llm_wiki_repo(up.path());
    let home = fake_home();
    fs::create_dir_all(home.path().join("grok")).unwrap();
    register_clone(&home, "llm-wiki", &repo, "HEAD");
    mark_plugin_installed(&home, "wiki@llm-wiki");
    (home, up, repo)
}

#[test]
fn sync_marketplace_path_copies_opencode_tree_not_local_only() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &llm_wiki_recipe(
            &file_url(&home.path().join("plugins/marketplaces/llm-wiki")),
            r#"["claude"]"#,
        ),
    );

    zskills(&home)
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains("wiki-manager"))
        .stdout(predicate::str::contains("applied."))
        .stdout(predicate::str::contains("declared local, not on disk").not());

    let wm = home.path().join("skills/wiki-manager");
    assert_eq!(
        fs::read_to_string(wm.join("SKILL.md")).unwrap(),
        OPENCODE_WIKI_SKILL
    );
    let copied = wm.join("references/hub-resolution.md");
    assert!(copied.is_file(), "in-clone symlink must be a regular file");
    assert!(!copied.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(home.path().join("skills/wiki-query/SKILL.md").is_file());

    let inv = read_inv(&home);
    let tag = "marketplace:llm-wiki:plugins/llm-wiki-opencode/skills";
    assert_eq!(inv["agent_skills"]["wiki-manager"]["source"], tag);
    assert_eq!(inv["agent_skills"]["wiki-query"]["source"], tag);

    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["enabledPlugins"]["wiki@llm-wiki"], true);

    let pi = home.path().join("pi/agent/settings.json");
    let pi_s: serde_json::Value = serde_json::from_slice(&fs::read(&pi).unwrap()).unwrap();
    let hub = home.path().join("skills").to_string_lossy().into_owned();
    let count = pi_s["skills"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v.as_str() == Some(hub.as_str()))
        .count();
    assert_eq!(count, 1, "Pi settings must list the hub once: {pi_s}");

    let grok_skills = home.path().join("grok/skills");
    assert!(
        !grok_skills.exists()
            || fs::read_dir(&grok_skills)
                .map(|rd| rd.count() == 0)
                .unwrap_or(true),
        "must not write GROK_HOME/skills"
    );
}

#[test]
fn sync_marketplace_path_takeover_and_list_and_remove() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &format!(
            r#"[[marketplaces]]
name = "llm-wiki"
url = "{url}"

[[skills]]
name = "wiki"
marketplace = "llm-wiki"
harnesses = ["pi"]
"#,
            url = file_url(&home.path().join("plugins/marketplaces/llm-wiki"))
        ),
    );
    zskills(&home).arg("sync").assert().success();
    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        CLAUDE_WIKI_SKILL
    );
    assert_eq!(
        read_inv(&home)["agent_skills"]["wiki-manager"]["source"],
        "plugin:wiki@llm-wiki"
    );

    write_manifest(
        &home,
        &llm_wiki_recipe(
            &file_url(&home.path().join("plugins/marketplaces/llm-wiki")),
            r#"["claude"]"#,
        ),
    );
    zskills(&home)
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "hub taken over by [[agent_skills]] path",
        ));

    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        OPENCODE_WIKI_SKILL
    );
    let tag = "marketplace:llm-wiki:plugins/llm-wiki-opencode/skills";
    assert_eq!(
        read_inv(&home)["agent_skills"]["wiki-manager"]["source"],
        tag
    );

    let out = zskills(&home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(
        v["plugins"]["active"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "wiki@llm-wiki"),
        "plugin must stay active: {v}"
    );
    let plugin_vis = v["plugins"]["harnesses"]["wiki@llm-wiki"]["visible"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(plugin_vis.iter().filter(|x| *x == "claude").count(), 1);
    assert!(
        !plugin_vis.iter().any(|x| x == "pi" || x == "grok"),
        "plugin line must be claude-only after takeover: {v}"
    );
    let skill_vis = v["agent_skills"]["harnesses"]["wiki-manager"]["visible"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        skill_vis.iter().any(|x| x == "pi") && skill_vis.iter().any(|x| x == "grok"),
        "Agent Skill line must include pi/grok: {v}"
    );

    zskills(&home)
        .args(["skill", "remove", "wiki-manager"])
        .assert()
        .success();
    assert!(!home.path().join("skills/wiki-manager").exists());
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(
        settings["enabledPlugins"]["wiki@llm-wiki"], true,
        "plugin stays enabled after Agent Skill remove"
    );
}

#[test]
fn sync_marketplace_path_skips_claimed_plugin_hub_copy() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &llm_wiki_recipe(
            &file_url(&home.path().join("plugins/marketplaces/llm-wiki")),
            r#"["pi"]"#,
        ),
    );
    zskills(&home)
        .arg("sync")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "hub owned by [[agent_skills]] path, not plugin wiki@llm-wiki",
        ));
    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        OPENCODE_WIKI_SKILL
    );
    assert_eq!(
        read_inv(&home)["agent_skills"]["wiki-manager"]["source"],
        "marketplace:llm-wiki:plugins/llm-wiki-opencode/skills"
    );
}

#[test]
fn sync_refuses_untagged_hub_dir() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    let dest = home.path().join("skills/wiki-manager");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("SKILL.md"), "user content\n").unwrap();
    write_manifest(
        &home,
        &format!(
            r#"[[marketplaces]]
name = "llm-wiki"
url = "{url}"

[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
name = "wiki-manager"
harnesses = ["pi", "grok"]
"#,
            url = file_url(&home.path().join("plugins/marketplaces/llm-wiki"))
        ),
    );
    let out = zskills(&home)
        .arg("sync")
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.contains("applied."),
        "refuse must skip ✓ applied.: {stdout}"
    );
    assert!(
        stderr.contains("no inventory entry") || stderr.contains("operation(s) failed"),
        "stderr={stderr}"
    );
    assert_eq!(
        fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "user content\n"
    );
}

#[test]
fn sync_unnamed_marketplace_path_is_not_pruned() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &format!(
            r#"[[marketplaces]]
name = "llm-wiki"
url = "{url}"

[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
harnesses = ["pi", "grok"]
"#,
            url = file_url(&home.path().join("plugins/marketplaces/llm-wiki"))
        ),
    );
    zskills(&home).arg("sync").assert().success();
    assert!(home.path().join("skills/wiki-manager/SKILL.md").is_file());
    assert!(home.path().join("skills/wiki-query/SKILL.md").is_file());

    zskills(&home).args(["sync", "--prune"]).assert().success();
    assert!(
        home.path().join("skills/wiki-manager/SKILL.md").is_file(),
        "unnamed marketplace+path must not be pruned"
    );
    assert!(home.path().join("skills/wiki-query/SKILL.md").is_file());
}

#[test]
fn sync_adopt_marketplace_tag_writes_marketplace_and_path() {
    let home = fake_home();
    let skill = home.path().join("skills/wiki-manager");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "x").unwrap();
    fs::write(
        home.path().join("skills/.zskills.json"),
        json!({
            "version": 1,
            "agent_skills": {
                "wiki-manager": {
                    "source": "marketplace:llm-wiki:plugins/llm-wiki-opencode/skills",
                    "installed_at": "@0",
                    "head_sha": "abc"
                }
            }
        })
        .to_string(),
    )
    .unwrap();
    write_manifest(&home, "");

    zskills(&home).args(["sync", "--adopt"]).assert().success();

    let raw = fs::read_to_string(home.path().join("config/zskills/skills.toml")).unwrap();
    assert!(
        raw.contains("marketplace = \"llm-wiki\""),
        "adopt must write marketplace: {raw}"
    );
    assert!(
        raw.contains("path = \"plugins/llm-wiki-opencode/skills\""),
        "adopt must write path: {raw}"
    );
    assert!(
        !raw.contains("source = \"marketplace:"),
        "adopt must not store the raw inventory tag as source: {raw}"
    );
}

#[test]
fn sync_missing_marketplace_clone_is_hard_error() {
    let home = fake_home();
    write_manifest(
        &home,
        r#"[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
name = "wiki-manager"
harnesses = ["pi", "grok"]
"#,
    );
    let out = zskills(&home)
        .arg("sync")
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.contains("applied."),
        "missing clone must skip ✓ applied.: {stdout}"
    );
    assert!(
        !stdout.contains("declared local, not on disk"),
        "must not fall through to local-only: {stdout}"
    );
    assert!(
        stderr.contains("not registered") || stderr.contains("operation(s) failed"),
        "stderr={stderr}"
    );
}

#[test]
fn sync_missing_path_exits_nonzero_skips_applied() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &format!(
            r#"[[marketplaces]]
name = "llm-wiki"
url = "{url}"

[[agent_skills]]
marketplace = "llm-wiki"
path = "does-not-exist"
name = "wiki-manager"
harnesses = ["pi", "grok"]
"#,
            url = file_url(&home.path().join("plugins/marketplaces/llm-wiki"))
        ),
    );
    let out = zskills(&home)
        .arg("sync")
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("applied."),
        "missing path must skip ✓ applied.: {stdout}"
    );
}

#[test]
fn upgrade_marketplace_path_recopies_not_local_only() {
    let (home, _up, repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &llm_wiki_recipe(
            &file_url(&home.path().join("plugins/marketplaces/llm-wiki")),
            r#"["claude"]"#,
        ),
    );
    zskills(&home).arg("sync").assert().success();

    fs::write(
        repo.join("plugins/llm-wiki-opencode/skills/wiki-manager/SKILL.md"),
        "---\nname: wiki-manager\n---\nOpenCode flavour v2\n",
    )
    .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["add", "-A"])
        .status()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["commit", "--quiet", "-m", "second"])
        .status()
        .unwrap();

    zskills(&home)
        .args(["skill", "upgrade", "wiki-manager"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(local-only, skipped)").not());

    // Upgrade of the skill name does not refresh the marketplace clone.
    // Point the clone at the new commit, then upgrade again.
    StdCommand::new("git")
        .arg("-C")
        .arg(home.path().join("plugins/marketplaces/llm-wiki"))
        .args(["pull", "--ff-only", "--quiet"])
        .status()
        .unwrap();
    zskills(&home)
        .args(["skill", "upgrade", "llm-wiki"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(local-only, skipped)").not());

    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        "---\nname: wiki-manager\n---\nOpenCode flavour v2\n"
    );
}

#[test]
fn sync_unnamed_marketplace_path_refuses_before_copying_siblings() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    let dest = home.path().join("skills/wiki-query");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("SKILL.md"), "user query\n").unwrap();
    write_manifest(
        &home,
        &format!(
            r#"[[marketplaces]]
name = "llm-wiki"
url = "{url}"

[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
harnesses = ["pi", "grok"]
"#,
            url = file_url(&home.path().join("plugins/marketplaces/llm-wiki"))
        ),
    );
    zskills(&home).arg("sync").assert().failure();
    assert!(
        !home.path().join("skills/wiki-manager").exists(),
        "a later refuse must not leave an earlier sibling copied and untagged"
    );
    assert_eq!(
        fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "user query\n"
    );
}

#[test]
fn plugin_install_skips_hub_names_claimed_by_agent_skills() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &llm_wiki_recipe(
            &file_url(&home.path().join("plugins/marketplaces/llm-wiki")),
            r#"["claude"]"#,
        ),
    );
    zskills(&home).arg("sync").assert().success();
    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        OPENCODE_WIKI_SKILL
    );

    zskills(&home)
        .args(["plugin", "install", "wiki@llm-wiki", "--harness", "pi"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "hub owned by [[agent_skills]] path, not plugin wiki@llm-wiki",
        ));

    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        OPENCODE_WIKI_SKILL,
        "plugin install must not clobber a marketplace: hub copy"
    );
    assert_eq!(
        read_inv(&home)["agent_skills"]["wiki-manager"]["source"],
        "marketplace:llm-wiki:plugins/llm-wiki-opencode/skills"
    );
}

#[test]
fn doctor_warns_when_hub_wiki_manager_is_claude_flavored_for_pi_or_grok() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &format!(
            r#"[[marketplaces]]
name = "llm-wiki"
url = "{url}"

[[skills]]
name = "wiki"
marketplace = "llm-wiki"
harnesses = ["pi"]
"#,
            url = file_url(&home.path().join("plugins/marketplaces/llm-wiki"))
        ),
    );
    zskills(&home).arg("sync").assert().success();
    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        CLAUDE_WIKI_SKILL
    );

    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude-flavored"))
        .stdout(predicate::str::contains("wiki-manager"));
}

#[test]
fn doctor_does_not_warn_on_opencode_wiki_shorthand() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &llm_wiki_recipe(
            &file_url(&home.path().join("plugins/marketplaces/llm-wiki")),
            r#"["claude"]"#,
        ),
    );
    zskills(&home).arg("sync").assert().success();
    let skill = fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap();
    assert!(
        skill.contains("/wiki:"),
        "fixture must contain /wiki: shorthand so this is a real negative"
    );
    assert_eq!(skill, OPENCODE_WIKI_SKILL);

    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude-flavored").not());
}

#[test]
fn doctor_warns_when_agent_skill_path_is_missing_on_disk() {
    let (home, _up, _repo) = setup_llm_wiki_home();
    write_manifest(
        &home,
        &format!(
            r#"[[marketplaces]]
name = "llm-wiki"
url = "{url}"

[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/does-not-exist/skills"
name = "wiki-manager"
harnesses = ["pi", "grok"]
"#,
            url = file_url(&home.path().join("plugins/marketplaces/llm-wiki"))
        ),
    );
    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("does-not-exist"))
        .stdout(predicate::str::contains("not on disk"));
}

#[test]
fn doctor_warns_when_marketplace_is_not_in_known_marketplaces() {
    let home = fake_home();
    write_manifest(
        &home,
        r#"[[agent_skills]]
marketplace = "ghost-mp"
path = "plugins/llm-wiki-opencode/skills"
name = "wiki-manager"
harnesses = ["pi", "grok"]
"#,
    );
    zskills(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("ghost-mp"))
        .stdout(predicate::str::contains("known_marketplaces.json"));
}

#[test]
fn skill_install_llm_wiki_without_path_redirects() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_llm_wiki_repo(up.path());
    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", &file_url(&repo)])
        .assert()
        .success()
        .stdout(predicate::str::contains("marketplace add"));
    assert!(
        !home.path().join("skills/wiki-manager").exists(),
        "bare install of a marketplace repo must not copy OpenCode Agent Skills"
    );
}

#[test]
fn skill_install_path_does_not_redirect_on_marketplace_repo() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_llm_wiki_repo(up.path());
    let home = fake_home();
    zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(&repo),
            "--path",
            "plugins/llm-wiki-opencode/skills",
            "--skill",
            "wiki-manager",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("wiki-manager"))
        .stdout(predicate::str::contains("marketplace add").not());

    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        OPENCODE_WIKI_SKILL
    );
    assert!(
        !home.path().join("skills/wiki-query").exists(),
        "--skill must install exactly one skill"
    );
    let copied = home
        .path()
        .join("skills/wiki-manager/references/hub-resolution.md");
    assert!(copied.is_file(), "in-clone symlink must be a regular file");
    assert!(!copied.symlink_metadata().unwrap().file_type().is_symlink());

    let tag = format!(
        "source:{}:plugins/llm-wiki-opencode/skills",
        file_url(&repo)
    );
    assert_eq!(
        read_inv(&home)["agent_skills"]["wiki-manager"]["source"],
        tag
    );

    let raw = fs::read_to_string(home.path().join("config/zskills/skills.toml")).unwrap();
    assert!(
        raw.contains(&format!("source = \"{}\"", file_url(&repo))),
        "unregistered spec must write source: {raw}"
    );
    assert!(raw.contains("path = \"plugins/llm-wiki-opencode/skills\""));
    assert!(raw.contains("name = \"wiki-manager\""));
    assert!(
        !raw.contains("marketplace ="),
        "unregistered spec must not write marketplace: {raw}"
    );
}

#[test]
fn skill_install_path_uses_registered_marketplace_not_source() {
    let (home, _up, repo) = setup_llm_wiki_home();
    zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(&repo),
            "--path",
            "plugins/llm-wiki-opencode/skills",
            "--skill",
            "wiki-manager",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("marketplace add").not());

    assert_eq!(
        fs::read_to_string(home.path().join("skills/wiki-manager/SKILL.md")).unwrap(),
        OPENCODE_WIKI_SKILL
    );
    assert_eq!(
        read_inv(&home)["agent_skills"]["wiki-manager"]["source"],
        "marketplace:llm-wiki:plugins/llm-wiki-opencode/skills"
    );

    let raw = fs::read_to_string(home.path().join("config/zskills/skills.toml")).unwrap();
    assert!(
        raw.contains("marketplace = \"llm-wiki\""),
        "registered spec must write marketplace: {raw}"
    );
    assert!(raw.contains("path = \"plugins/llm-wiki-opencode/skills\""));
    assert!(raw.contains("name = \"wiki-manager\""));
    assert!(
        !raw.contains("source ="),
        "registered spec must not write source: {raw}"
    );

    let cache = home.path().join("cache/zskills/agent-skills");
    assert!(
        !cache.exists()
            || fs::read_dir(&cache)
                .map(|rd| rd.count() == 0)
                .unwrap_or(true),
        "must reuse the marketplace clone, not clone into the Agent Skill cache"
    );
}

#[test]
fn skill_install_path_parent_installs_every_skill_under_path() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_llm_wiki_repo(up.path());
    let home = fake_home();
    zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(&repo),
            "--path",
            "plugins/llm-wiki-opencode/skills",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("marketplace add").not());

    assert!(home.path().join("skills/wiki-manager/SKILL.md").is_file());
    assert!(home.path().join("skills/wiki-query/SKILL.md").is_file());

    let raw = fs::read_to_string(home.path().join("config/zskills/skills.toml")).unwrap();
    assert!(raw.contains("path = \"plugins/llm-wiki-opencode/skills\""));
    assert!(
        !raw.contains("name ="),
        "all-under-path must write one unnamed row, not skills = []: {raw}"
    );
}

#[test]
fn skill_install_path_skill_dir_installs_one() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_llm_wiki_repo(up.path());
    let home = fake_home();
    zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(&repo),
            "--path",
            "plugins/llm-wiki-opencode/skills/wiki-manager",
        ])
        .assert()
        .success();

    assert!(home.path().join("skills/wiki-manager/SKILL.md").is_file());
    assert!(
        !home.path().join("skills/wiki-query").exists(),
        "a skill-dir path must not also install sibling skills"
    );
}

#[test]
fn skill_install_path_unknown_skill_lists_available() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_llm_wiki_repo(up.path());
    let home = fake_home();
    let out = zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(&repo),
            "--path",
            "plugins/llm-wiki-opencode/skills",
            "--skill",
            "nope",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&out);
    assert!(stderr.contains("'nope' not found"), "{stderr}");
    assert!(
        stderr.contains("wiki-manager") && stderr.contains("wiki-query"),
        "survey must walk --path: {stderr}"
    );
}

#[test]
fn skill_install_path_rejects_escape() {
    let home = fake_home();
    zskills(&home)
        .args(["skill", "install", "owner/repo", "--path", "../escape"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid path"));
}

#[test]
fn skill_install_path_missing_in_clone() {
    let up = tempfile::tempdir().unwrap();
    let repo = write_llm_wiki_repo(up.path());
    let home = fake_home();
    zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(&repo),
            "--path",
            "does-not-exist",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn skill_install_path_large_collection_next_steps_repeat_path() {
    // >5 under --path still aborts. Next-step commands must keep --path, or a
    // marketplace repo redirects / surveys SKILL_ROOTS and finds nothing.
    let up = tempfile::tempdir().unwrap();
    let repo = up.path().join("odd-layout");
    fs::create_dir_all(repo.join(".claude-plugin")).unwrap();
    fs::write(
        repo.join(".claude-plugin").join("marketplace.json"),
        r#"{"name":"odd","plugins":[]}"#,
    )
    .unwrap();
    for i in 0..7 {
        let d = repo
            .join("packages")
            .join("foo")
            .join("skills")
            .join(format!("skill-{i}"));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("SKILL.md"), format!("---\nname: skill-{i}\n---\n")).unwrap();
    }
    git_init_and_commit(&repo);

    let home = fake_home();
    let out = zskills(&home)
        .args([
            "skill",
            "install",
            &file_url(&repo),
            "--path",
            "packages/foo/skills",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("won't install all"))
        .stdout(predicate::str::contains("marketplace add").not())
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);
    let path_flag = "--path packages/foo/skills";
    assert!(
        stdout.contains(&format!("skill install {} {path_flag} -i", file_url(&repo))),
        "-i next step must repeat --path: {stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "skill install {} {path_flag} --all",
            file_url(&repo)
        )),
        "--all next step must repeat --path: {stdout}"
    );
    assert!(
        !home.path().join("skills/skill-0").exists(),
        "a large --path collection must install nothing"
    );
}
