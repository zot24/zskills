//! End-to-end CLI tests. We point CLAUDE_HOME at a tempdir so the binary
//! cannot touch the real `~/.claude/`.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn zskills(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("zskills").unwrap();
    cmd.env("CLAUDE_HOME", home.path());
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
    assert_eq!(v["active"][0], "foo@test-mp");
    // `bar@test-mp` is in enabledPlugins but value=false AND not installed → not active, not orphan
    assert!(v["enabled_orphan"].as_array().unwrap().is_empty());
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
