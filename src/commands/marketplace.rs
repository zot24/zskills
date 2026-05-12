use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::{json, Map, Value};

use crate::cli::MarketplaceCmd;

pub fn run(cmd: MarketplaceCmd) -> Result<()> {
    match cmd {
        MarketplaceCmd::Add { source } => add(source),
        MarketplaceCmd::Remove { name } => remove(name),
        MarketplaceCmd::List { json: as_json } => list(as_json),
        MarketplaceCmd::Update { name } => update(name),
    }
}

fn add(source: String) -> Result<()> {
    let (name, repo_url) = parse_source(&source)?;
    let path = crate::paths::known_marketplaces_json()?;
    let mut known = crate::marketplace::load_known(&path)?;

    let install_location = crate::paths::marketplaces_dir()?.join(&name);
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

    let mut entry = Map::new();
    let github_form = source.split('/').collect::<Vec<_>>();
    let src_obj = if github_form.len() == 2 && !source.starts_with("http") {
        json!({ "source": "github", "repo": source })
    } else {
        json!({ "source": "git", "url": repo_url })
    };
    entry.insert("source".into(), src_obj);
    entry.insert(
        "installLocation".into(),
        Value::String(install_location.to_string_lossy().to_string()),
    );
    entry.insert("autoUpdate".into(), Value::Bool(true));
    known.insert(name.clone(), Value::Object(entry));

    crate::marketplace::save_known(&path, &known)?;

    // Mirror in settings.json -> extraKnownMarketplaces
    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    let ekm = crate::settings::extra_marketplaces_mut(&mut settings);
    ekm.insert(
        name.clone(),
        json!({ "source": if source.contains('/') && !source.contains("://") {
            json!({ "source": "github", "repo": source })
        } else {
            json!({ "source": "git", "url": repo_url })
        }}),
    );
    crate::settings::save(&settings_path, &settings)?;

    println!("{} added marketplace {}", "✓".green(), name);
    Ok(())
}

fn parse_source(source: &str) -> Result<(String, String)> {
    if source.contains("://") {
        // git URL
        let name = source
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or(source)
            .to_string();
        Ok((name, source.to_string()))
    } else if source.contains('/') && !source.starts_with('/') {
        // owner/repo
        let name = source.split('/').next_back().unwrap_or(source).to_string();
        let url = format!("https://github.com/{}.git", source);
        Ok((name, url))
    } else {
        anyhow::bail!(
            "unrecognized marketplace source: {} (expected owner/repo or git URL)",
            source
        )
    }
}

fn remove(name: String) -> Result<()> {
    let path = crate::paths::known_marketplaces_json()?;
    let mut known = crate::marketplace::load_known(&path)?;
    known.remove(&name);
    crate::marketplace::save_known(&path, &known)?;

    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    crate::settings::extra_marketplaces_mut(&mut settings).remove(&name);
    crate::settings::save(&settings_path, &settings)?;

    let dir = crate::paths::marketplaces_dir()?.join(&name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).ok();
    }
    println!("{} removed marketplace {}", "-".yellow(), name);
    Ok(())
}

fn list(as_json: bool) -> Result<()> {
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&known)?);
        return Ok(());
    }
    if known.is_empty() {
        println!("(no marketplaces registered)");
        return Ok(());
    }
    for (name, entry) in &known {
        let count = crate::marketplace::load_manifest(&crate::paths::marketplace_manifest(name)?)
            .map(|m| m.plugins.len())
            .unwrap_or(0);
        let auto = entry
            .get("autoUpdate")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        println!(
            "  {}  {} plugin(s){}",
            name.bold(),
            count,
            if auto {
                "  [autoUpdate]".dimmed().to_string()
            } else {
                String::new()
            }
        );
    }
    Ok(())
}

fn update(name: Option<String>) -> Result<()> {
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    let targets: Vec<String> = match name {
        Some(n) => vec![n],
        None => known.keys().cloned().collect(),
    };
    for n in &targets {
        let repo = crate::paths::marketplaces_dir()?.join(n);
        if repo.exists() {
            print!("Updating {} ... ", n);
            match crate::git::pull(&repo) {
                Ok(()) => println!("{}", "ok".green()),
                Err(e) => println!("{} ({})", "fail".red(), e),
            }
        }
    }
    Ok(())
}
