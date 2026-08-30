//! Marketplace (tap) parsing.
//!
//! known_marketplaces.json schema (observed):
//! {
//!   "<name>": {
//!     "source": { "source": "github", "repo": "owner/repo" },
//!     "installLocation": "/Users/.../marketplaces/<name>",
//!     "lastUpdated": "...",
//!     "autoUpdate": true
//!   }
//! }

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MarketplaceManifest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PluginEntry {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// `source` can be a string OR an object (`{ source, url, ref, sha, ... }`) depending on marketplace.
    #[serde(default)]
    pub source: Option<serde_json::Value>,
}

pub fn load_known(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let bytes = std::fs::read(path)?;
    let v: Value = serde_json::from_slice(&bytes)?;
    match v {
        Value::Object(m) => Ok(m),
        _ => anyhow::bail!("{} is not a JSON object", path.display()),
    }
}

pub fn save_known(path: &Path, map: &Map<String, Value>) -> Result<()> {
    crate::settings::save(path, map)
}

/// True when a `known_marketplaces.json` entry is missing a usable `lastUpdated`.
///
/// Claude Code validates the field as a non-empty **string**. A missing key, a `null`,
/// a number (epoch millis is the tempting-but-wrong encoding) or an empty string all
/// make it refuse the whole file with:
///
/// > Marketplace configuration file is corrupted: `<name>`.lastUpdated:
/// > Invalid input: expected string, received undefined
///
/// which takes down *every* `claude plugin install`, not just the offending tap.
pub fn missing_last_updated(entry: &Value) -> bool {
    match entry.get("lastUpdated") {
        Some(Value::String(s)) => s.trim().is_empty(),
        _ => true,
    }
}

/// Write `lastUpdated` on `name`'s entry, preserving every other field.
/// Returns whether anything changed (false if `name` isn't a known object entry).
pub fn stamp_last_updated(known: &mut Map<String, Value>, name: &str) -> bool {
    match known.get_mut(name).and_then(|e| e.as_object_mut()) {
        Some(entry) => {
            entry.insert(
                "lastUpdated".into(),
                Value::String(crate::timestamp::utc_now_iso8601()),
            );
            true
        }
        None => false,
    }
}

/// Recognize a remote-index entry by its JSON shape. Non-feature-gated so older configs
/// (entries written by a `skills-sh`-enabled build) are still tolerated when the feature
/// is off — we just skip them in list/update rather than crashing.
pub fn is_remote_index(entry: &Value) -> bool {
    entry
        .get("source")
        .and_then(|s| s.get("source"))
        .and_then(|v| v.as_str())
        == Some("remote-index")
}

/// `extraKnownMarketplaces` from `settings.json`. Empty when the file is missing.
pub fn extra_known() -> Map<String, Value> {
    let Ok(path) = crate::paths::settings_json() else {
        return Map::new();
    };
    let Ok(settings) = crate::settings::load(&path) else {
        return Map::new();
    };
    settings
        .get("extraKnownMarketplaces")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

/// True when `name` is in `known_marketplaces.json` or `extraKnownMarketplaces`.
///
/// Claude Code honours either file. Treating extraKnown-only as unregistered would
/// make `sync` re-clone a marketplace the user already has, and would refuse
/// `enabledPlugins` keys that `plugin_offer` already considers legitimate.
pub fn is_registered(known: &Map<String, Value>, extra: &Map<String, Value>, name: &str) -> bool {
    known.contains_key(name) || extra.contains_key(name)
}

/// `owner/repo` vs a git URL. `owner/repo` must be exactly two non-empty segments
/// and must not look like a URL — that is the shape Claude Code stores as
/// `"source": "github"`.
pub fn is_github_owner_repo(source: &str) -> bool {
    if source.contains("://") || source.starts_with('/') {
        return false;
    }
    let mut parts = source.split('/');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(a), Some(b), None) if !a.is_empty() && !b.is_empty()
    )
}

/// Parse `owner/repo` or a git URL into `(derived_name, clone_url)`.
///
/// `marketplace add` uses the derived name. `sync` ignores it and uses the name
/// declared on `[[marketplaces]]`, so `name = "zot24-skills"` can point at
/// `repo = "zot24/skills"`.
pub fn parse_source(source: &str) -> Result<(String, String)> {
    if source.contains("://") {
        let name = source
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or(source)
            .to_string();
        Ok((name, source.to_string()))
    } else if source.contains('/') && !source.starts_with('/') {
        let name = source.split('/').next_back().unwrap_or(source).to_string();
        let url = format!("https://github.com/{source}.git");
        Ok((name, url))
    } else {
        anyhow::bail!("unrecognized marketplace source: {source} (expected owner/repo or git URL)")
    }
}

fn source_object(source: &str, repo_url: &str) -> Value {
    if is_github_owner_repo(source) {
        json!({ "source": "github", "repo": source })
    } else {
        json!({ "source": "git", "url": repo_url })
    }
}

/// Clone `source` (if needed) and write `known_marketplaces.json` plus
/// `extraKnownMarketplaces`. Same bytes as `marketplace add`.
///
/// `name` is the key in those files, not necessarily the repo basename.
pub fn register(name: &str, source: &str) -> Result<PathBuf> {
    let (_, repo_url) = parse_source(source)?;
    let path = crate::paths::known_marketplaces_json()?;
    let mut known = load_known(&path)?;

    let install_location = crate::paths::marketplaces_dir()?.join(name);
    if !install_location.exists() {
        if let Some(parent) = install_location.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        println!(
            "Cloning {} into {} ...",
            repo_url,
            install_location.display()
        );
        crate::git::clone(&repo_url, &install_location)?;
    }

    let src_obj = source_object(source, &repo_url);
    let mut entry = Map::new();
    entry.insert("source".into(), src_obj.clone());
    entry.insert(
        "installLocation".into(),
        Value::String(install_location.to_string_lossy().to_string()),
    );
    entry.insert("autoUpdate".into(), Value::Bool(true));
    // Claude Code validates `lastUpdated` as a *string* when it loads
    // known_marketplaces.json. Omit it and every `claude plugin install` fails with
    // "Marketplace configuration file is corrupted: <name>.lastUpdated: Invalid
    // input: expected string, received undefined". This field is not optional.
    entry.insert(
        "lastUpdated".into(),
        Value::String(crate::timestamp::utc_now_iso8601()),
    );
    known.insert(name.to_string(), Value::Object(entry));
    save_known(&path, &known)?;

    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    crate::settings::extra_marketplaces_mut(&mut settings)
        .insert(name.to_string(), json!({ "source": src_obj }));
    crate::settings::save(&settings_path, &settings)?;
    Ok(install_location)
}

/// Shareable `repo` / `url` from a `known_marketplaces.json` or
/// `extraKnownMarketplaces` entry. `None` for remote indexes and unreadable shapes —
/// `--adopt` must not write a `[[marketplaces]]` row a fresh machine cannot clone.
pub fn source_for_manifest(entry: &Value) -> Option<(Option<String>, Option<String>)> {
    if is_remote_index(entry) {
        return None;
    }
    let src = entry.get("source")?;
    match src.get("source").and_then(|v| v.as_str()) {
        Some("github") => {
            let repo = src.get("repo").and_then(|v| v.as_str())?.to_string();
            Some((Some(repo), None))
        }
        Some("git") => {
            let url = src.get("url").and_then(|v| v.as_str())?.to_string();
            match github_from_url(&url) {
                Some(repo) => Some((Some(repo), None)),
                None => Some((None, Some(url))),
            }
        }
        _ => None,
    }
}

/// Resolve a marketplace's source into a GitHub `owner/repo`, if its `known_marketplaces.json`
/// entry encodes one. Used to update non-git marketplaces via tarball.
pub fn github_owner_repo(known: &Map<String, Value>, name: &str) -> Option<String> {
    let entry = known.get(name)?;
    let src = entry.get("source")?;
    // Two shapes observed:
    //   { "source": "github", "repo": "owner/repo" }
    //   { "source": "git", "url": "https://github.com/owner/repo.git" }
    if let Some("github") = src.get("source").and_then(|v| v.as_str()) {
        return src.get("repo").and_then(|v| v.as_str()).map(str::to_string);
    }
    if let Some("git") = src.get("source").and_then(|v| v.as_str()) {
        let url = src.get("url").and_then(|v| v.as_str())?;
        return github_from_url(url);
    }
    None
}

fn github_from_url(url: &str) -> Option<String> {
    let stripped = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("git@github.com:"))?;
    let stripped = stripped.trim_end_matches(".git");
    Some(stripped.to_string())
}

/// What a refresh did to a marketplace clone.
#[derive(Debug)]
pub enum Refresh {
    /// Unpinned: pulled (or re-extracted) to whatever upstream now says.
    Floated,
    /// Pinned: the clone is at the pinned commit. `moved` is false when it was
    /// already there, true when this call put it back.
    Pinned { sha: String, moved: bool },
}

/// Read the marketplace pins declared in the user's `skills.toml`.
///
/// A missing or unparseable manifest means "no pins": a broken manifest must not
/// silently turn every pin off, so a parse error is surfaced to the caller rather
/// than swallowed.
pub fn load_pins() -> Result<std::collections::BTreeMap<String, String>> {
    let Some(path) = crate::manifest::discover() else {
        return Ok(Default::default());
    };
    let manifest = crate::manifest::load(&path)?;
    Ok(manifest
        .marketplaces
        .iter()
        .filter_map(|m| {
            manifest
                .marketplace_pin(&m.name)
                .map(|p| (m.name.clone(), p.to_string()))
        })
        .collect())
}

/// Bring one marketplace clone to where it should be.
///
/// Without a pin this is the old behaviour: `git pull`, or a tarball re-extract for a
/// clone that is not a git working tree.
///
/// With a pin, the clone is checked out at that ref and **never pulled**. The ref is
/// resolved from what the clone already has; only when that fails do we `git fetch`,
/// which cannot move `HEAD`. If the ref still does not resolve, this is an error — a
/// pin that cannot be honoured must never fall through to a pull, because floating the
/// tap is the exact failure the pin exists to prevent.
pub fn refresh(name: &str, repo: &Path, pin: Option<&str>) -> Result<Refresh> {
    let Some(pin) = pin else {
        if crate::git::is_git_repo(repo) {
            crate::git::pull(repo)?;
        } else {
            update_via_tarball(name, repo)?;
        }
        return Ok(Refresh::Floated);
    };

    anyhow::ensure!(
        crate::git::is_git_repo(repo),
        "marketplace '{}' is pinned to {} but {} is not a git clone — \
         a tarball marketplace has no refs to pin to. Remove the pin, or re-add the \
         marketplace from a git source.",
        name,
        pin,
        repo.display()
    );

    let target = match crate::git::resolve_commit(repo, pin) {
        Some(sha) => sha,
        None => {
            crate::git::fetch_all(repo).with_context(|| {
                format!("fetching marketplace '{}' to resolve pin {}", name, pin)
            })?;
            crate::git::resolve_commit(repo, pin).ok_or_else(|| {
                anyhow::anyhow!(
                    "marketplace '{}' is pinned to {}, which does not exist in {} \
                     even after a fetch — refusing to update it to anything else",
                    name,
                    pin,
                    repo.display()
                )
            })?
        }
    };

    let head = crate::git::head_sha(repo).unwrap_or_default();
    if head == target {
        return Ok(Refresh::Pinned {
            sha: target,
            moved: false,
        });
    }
    crate::git::checkout_detached(repo, &target)?;
    Ok(Refresh::Pinned {
        sha: target,
        moved: true,
    })
}

/// One-line status for a refresh, for the three commands that print it.
pub fn refresh_label(r: &Refresh) -> String {
    match r {
        Refresh::Floated => "ok".to_string(),
        Refresh::Pinned { sha, moved: false } => {
            format!("pinned @ {}", &sha[..sha.len().min(7)])
        }
        Refresh::Pinned { sha, moved: true } => {
            format!("pinned @ {} (restored)", &sha[..sha.len().min(7)])
        }
    }
}

/// Fetch the marketplace's GitHub archive tarball and atomically replace `dest`.
/// Tries `HEAD.tar.gz` (default branch). Uses the system temp dir for extraction
/// then renames into place.
pub fn update_via_tarball(name: &str, dest: &Path) -> Result<()> {
    let known = load_known(&crate::paths::known_marketplaces_json()?)?;
    let owner_repo = github_owner_repo(&known, name)
        .with_context(|| format!("no GitHub source recorded for marketplace '{}'", name))?;

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("zskills/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    // Try HEAD (default branch), then fall back to main and master explicitly.
    let candidates = [
        format!("https://github.com/{}/archive/HEAD.tar.gz", owner_repo),
        format!(
            "https://github.com/{}/archive/refs/heads/main.tar.gz",
            owner_repo
        ),
        format!(
            "https://github.com/{}/archive/refs/heads/master.tar.gz",
            owner_repo
        ),
    ];
    let mut bytes: Option<bytes::Bytes> = None;
    let mut last_err: Option<anyhow::Error> = None;
    for url in &candidates {
        match client.get(url).send().and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes() {
                Ok(b) => {
                    bytes = Some(b);
                    break;
                }
                Err(e) => last_err = Some(anyhow::Error::from(e)),
            },
            Err(e) => last_err = Some(anyhow::Error::from(e)),
        }
    }
    let bytes = bytes.ok_or_else(|| {
        anyhow::anyhow!(
            "could not fetch any tarball variant for {} ({})",
            owner_repo,
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )
    })?;

    // Extract into a sibling temp dir so the final rename stays on the same filesystem.
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("dest has no parent"))?;
    std::fs::create_dir_all(parent).ok();
    let staging = tempfile::tempdir_in(parent)
        .with_context(|| format!("creating staging dir under {}", parent.display()))?;

    let decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(staging.path())
        .with_context(|| "extracting tarball")?;

    // GitHub archives unpack as a single top-level dir like `<repo>-<sha>/`.
    let mut entries = std::fs::read_dir(staging.path())?;
    let only = entries
        .next()
        .ok_or_else(|| anyhow::anyhow!("tarball had no entries"))??;
    anyhow::ensure!(entries.next().is_none(), "tarball had unexpected layout");
    let extracted = only.path();

    // Atomic-ish swap: keep a backup we restore on failure.
    let backup = parent.join(format!(".{}-zskills-backup", name));
    if backup.exists() {
        std::fs::remove_dir_all(&backup).ok();
    }
    if dest.exists() {
        std::fs::rename(dest, &backup)
            .with_context(|| format!("moving existing {} aside", dest.display()))?;
    }
    if let Err(e) = std::fs::rename(&extracted, dest) {
        // Roll back.
        if backup.exists() {
            std::fs::rename(&backup, dest).ok();
        }
        return Err(e).context(format!(
            "moving extracted tree into place at {}",
            dest.display()
        ));
    }
    if backup.exists() {
        std::fs::remove_dir_all(&backup).ok();
    }
    Ok(())
}

pub fn load_manifest(path: &Path) -> Result<MarketplaceManifest> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading marketplace manifest {}", path.display()))?;
    let m: MarketplaceManifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as marketplace manifest", path.display()))?;
    Ok(m)
}

/// What a registered marketplace can tell us about an enabled plugin.
///
/// The three states matter because `doctor --fix` acts differently on each, and
/// collapsing them is how you delete a user's plugin by accident.
#[derive(Debug, PartialEq, Eq)]
pub enum Offer {
    /// A registered marketplace's manifest lists this plugin. The enable is legitimate;
    /// it just has no bytes yet. Fix by installing.
    Yes,
    /// A registered marketplace's manifest was read successfully and does *not* list it,
    /// or no marketplace by that name is registered at all. The enable is dangling.
    /// Fix by removing.
    No,
    /// We could not read the marketplace manifest — the clone was never fetched, was
    /// deleted, or is corrupt. This is ignorance, not evidence. Fix nothing.
    Unknown,
}

/// Is `mp` declared in `settings.json -> extraKnownMarketplaces`?
///
/// A marketplace can be registered in either place: `zskills marketplace add` writes
/// both, but Claude Code also honours a settings-only declaration (how team and
/// enterprise configs ship a tap). Consulting only `known_marketplaces.json` would
/// classify every enable from such a tap as dangling — and `--fix` would delete them.
fn is_extra_marketplace(mp: &str) -> bool {
    let Ok(path) = crate::paths::settings_json() else {
        return false;
    };
    let Ok(settings) = crate::settings::load(&path) else {
        return false;
    };
    settings
        .get("extraKnownMarketplaces")
        .and_then(|v| v.as_object())
        .is_some_and(|m| m.contains_key(mp))
}

/// Classify an enabled `name@marketplace` key against the registered marketplaces.
///
/// The distinction between [`Offer::No`] and [`Offer::Unknown`] is the whole point:
/// a plugin `zskills install` just enabled resolves through a *readable* manifest, so an
/// unreadable one can never be grounds for revoking an enable. Destroying user intent
/// because we failed to read a file is worse than leaving a stale flag in place.
pub fn plugin_offer(known: &Map<String, Value>, qualified: &str) -> Offer {
    let Some((name, mp)) = qualified.rsplit_once('@') else {
        // Not a qualified key at all. We have no idea what it refers to.
        return Offer::Unknown;
    };
    if !known.contains_key(mp) && !is_extra_marketplace(mp) {
        return Offer::No;
    }
    let Ok(manifest_path) = crate::paths::marketplace_manifest(mp) else {
        return Offer::Unknown;
    };
    match load_manifest(&manifest_path) {
        Ok(m) if m.plugins.iter().any(|p| p.name == name) => Offer::Yes,
        Ok(_) => Offer::No,
        Err(_) => Offer::Unknown,
    }
}

/// Resolve a possibly-unqualified spec ("foo" or "foo@bar") against known marketplaces.
/// Returns the qualified form "name@marketplace".
pub fn resolve_spec(spec: &str, known: &Map<String, Value>) -> Result<String> {
    if let Some((name, mp)) = spec.split_once('@') {
        let qualified = format!("{}@{}", name, mp);
        // Accepting any string with an `@` in it meant `install bogus@no-such-mp`
        // happily wrote a dangling enable that the next `doctor --fix` then deleted:
        // zskills damaging the file and repairing its own damage. Verify first.
        return match plugin_offer(known, &qualified) {
            Offer::Yes => Ok(qualified),
            Offer::No if !known.contains_key(mp) => {
                anyhow::bail!(
                    "marketplace '{}' is not registered (try `zskills marketplace add <owner/repo>`)",
                    mp
                )
            }
            Offer::No => anyhow::bail!("marketplace '{}' does not offer a plugin '{}'", mp, name),
            Offer::Unknown => anyhow::bail!(
                "could not read the manifest for marketplace '{}' — run `zskills marketplace update {}` first",
                mp,
                mp
            ),
        };
    }
    let mut matches: Vec<String> = Vec::new();
    for mp_name in known.keys() {
        if let Ok(manifest) = load_manifest(&crate::paths::marketplace_manifest(mp_name)?) {
            if manifest.plugins.iter().any(|p| p.name == spec) {
                matches.push(format!("{}@{}", spec, mp_name));
            }
        }
    }
    match matches.len() {
        0 => anyhow::bail!("skill '{}' not found in any registered marketplace", spec),
        1 => Ok(matches.remove(0)),
        _ => Err(crate::error::Error::AmbiguousSkill(spec.to_string(), matches.join(", ")).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_last_updated_flags_absent_null_and_non_string() {
        assert!(missing_last_updated(&json!({ "autoUpdate": true })));
        assert!(missing_last_updated(&json!({ "lastUpdated": null })));
        // Epoch millis is a string-typed field in Claude Code's schema, so a
        // number is just as corrupt as a missing key.
        assert!(missing_last_updated(
            &json!({ "lastUpdated": 1755648000000i64 })
        ));
        assert!(missing_last_updated(&json!({ "lastUpdated": "" })));
        assert!(missing_last_updated(&json!({ "lastUpdated": "   " })));
    }

    #[test]
    fn missing_last_updated_accepts_a_string() {
        assert!(!missing_last_updated(
            &json!({ "lastUpdated": "2026-08-20T00:00:00.000Z" })
        ));
    }

    #[test]
    fn stamp_last_updated_preserves_other_fields() {
        let mut known = Map::new();
        known.insert(
            "mp".into(),
            json!({
                "source": { "source": "github", "repo": "owner/mp" },
                "installLocation": "/tmp/mp",
                "autoUpdate": true
            }),
        );
        assert!(stamp_last_updated(&mut known, "mp"));
        let entry = &known["mp"];
        assert!(!missing_last_updated(entry));
        assert_eq!(entry["installLocation"], json!("/tmp/mp"));
        assert_eq!(entry["autoUpdate"], json!(true));
        assert_eq!(entry["source"]["repo"], json!("owner/mp"));
    }

    #[test]
    fn plugin_offer_says_no_when_no_marketplace_is_registered() {
        let known = Map::new();
        assert_eq!(plugin_offer(&known, "ghost@nowhere"), Offer::No);
    }

    #[test]
    fn plugin_offer_says_unknown_for_an_unqualified_key() {
        let known = Map::new();
        assert_eq!(plugin_offer(&known, "bare-name"), Offer::Unknown);
    }

    #[test]
    fn stamp_last_updated_is_a_noop_for_unknown_names() {
        let mut known = Map::new();
        assert!(!stamp_last_updated(&mut known, "nope"));
        assert!(known.is_empty());
    }

    #[test]
    fn is_github_owner_repo_accepts_exactly_two_segments() {
        assert!(is_github_owner_repo("zot24/skills"));
        assert!(is_github_owner_repo("nvk/llm-wiki"));
        assert!(!is_github_owner_repo("https://github.com/zot24/skills.git"));
        assert!(!is_github_owner_repo("file:///tmp/skills"));
        assert!(!is_github_owner_repo("skills"));
        assert!(!is_github_owner_repo("/abs/path"));
        assert!(!is_github_owner_repo("a/b/c"));
    }

    #[test]
    fn parse_source_github_and_url() {
        let (name, url) = parse_source("zot24/skills").unwrap();
        assert_eq!(name, "skills");
        assert_eq!(url, "https://github.com/zot24/skills.git");
        let (name, url) = parse_source("file:///tmp/llm-wiki").unwrap();
        assert_eq!(name, "llm-wiki");
        assert_eq!(url, "file:///tmp/llm-wiki");
        assert!(parse_source("noshapes").is_err());
    }

    #[test]
    fn source_for_manifest_prefers_repo_for_github() {
        let github = json!({
            "source": { "source": "github", "repo": "nvk/llm-wiki" }
        });
        assert_eq!(
            source_for_manifest(&github),
            Some((Some("nvk/llm-wiki".into()), None))
        );
        let git_github = json!({
            "source": { "source": "git", "url": "https://github.com/zot24/skills.git" }
        });
        assert_eq!(
            source_for_manifest(&git_github),
            Some((Some("zot24/skills".into()), None))
        );
        let file_url = json!({
            "source": { "source": "git", "url": "file:///tmp/llm-wiki" }
        });
        assert_eq!(
            source_for_manifest(&file_url),
            Some((None, Some("file:///tmp/llm-wiki".into())))
        );
        let remote = json!({
            "source": { "source": "remote-index", "url": "https://skills.sh" }
        });
        assert_eq!(source_for_manifest(&remote), None);
    }

    #[test]
    fn is_registered_accepts_known_or_extra() {
        let mut known = Map::new();
        known.insert("a".into(), json!({}));
        let mut extra = Map::new();
        extra.insert("b".into(), json!({}));
        assert!(is_registered(&known, &extra, "a"));
        assert!(is_registered(&known, &extra, "b"));
        assert!(!is_registered(&known, &extra, "c"));
    }
}
