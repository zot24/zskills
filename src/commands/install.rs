//! `install <name>[@marketplace]...` — adds to enabledPlugins.
//!
//! v1 strategy: we don't fetch bytes ourselves — we delegate the actual download to
//! Claude Code by flipping enabledPlugins. Claude Code's startup install path will
//! materialize the bytes from the marketplace tap on next launch (or via /plugin install).
//!
//! Future v1.1: shell out to `claude plugin install` for synchronous install when present.

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;

pub fn run(specs: Vec<String>) -> Result<()> {
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    if known.is_empty() {
        println!(
            "{}",
            "No marketplaces registered. Run `zskills marketplace add <owner/repo>` first.".yellow()
        );
        return Ok(());
    }

    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;

    let mut count = 0;
    for spec in &specs {
        match crate::marketplace::resolve_spec(spec, &known) {
            Ok(qualified) => {
                let ep = crate::settings::enabled_plugins_mut(&mut settings);
                ep.insert(qualified.clone(), Value::Bool(true));
                println!("{} {}", "+".green(), qualified);
                count += 1;
            }
            Err(e) => {
                eprintln!("{} {}: {}", "✗".red(), spec, e);
            }
        }
    }

    crate::settings::save(&settings_path, &settings)?;
    println!(
        "\nWrote {} entry/entries to {}.\nRestart Claude Code (or run `/plugin marketplace update` and `/plugin install ...`) to fetch the bytes.",
        count,
        settings_path.display()
    );
    Ok(())
}
