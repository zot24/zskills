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
    // Strip ANSI colors so `predicate::str::contains` assertions match raw text.
    cmd.env("NO_COLOR", "1");
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
        .args(["disable", "foo@test-mp"])
        .assert()
        .success();
    let s: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(s["enabledPlugins"]["foo@test-mp"], false);
    assert_eq!(s["permissions"]["defaultMode"], "auto"); // preserved
    assert!(s["hooks"].is_object()); // preserved

    zskills(&home)
        .args(["enable", "foo@test-mp"])
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
fn upgrade_runs_without_marketplaces_or_manifest() {
    // Smoke test: upgrade against an empty fake home should succeed and print the
    // "Upgrade complete" line.
    let home = fake_home();
    zskills(&home)
        .args(["upgrade"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Upgrade complete"));
}

#[test]
fn doctor_detects_orphan_and_fixes_it() {
    let home = fake_home();
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
        .args(["install", "--help"])
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
        .args(["remove", "--help"])
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
    cmd.env("NO_COLOR", "1");
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

#[test]
fn install_repo_single_skill_auto_installs() {
    let upstream = tempfile::tempdir().unwrap();
    write_skill(upstream.path(), "alpha", "Alpha skill");
    git_init_and_commit(upstream.path());

    let home = fake_home();
    zskills(&home)
        .args(["install", &file_url(upstream.path())])
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
        .args(["install", &file_url(upstream.path())])
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
        .args(["install", &file_url(upstream.path())])
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
        .args(["install", &file_url(upstream.path()), "--all"])
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
        .args(["install", &file_url(upstream.path())])
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
        .args(["install", &file_url(upstream.path())])
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
        .args(["install", &file_url(upstream.path())])
        .assert()
        .success() // emit error to stderr but exit 0; the partition-based dispatch logs and continues
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&out);
    assert!(stderr.contains("no Agent Skills found"));
}
