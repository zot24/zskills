use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;

pub fn run(specs: Vec<String>, enable: bool) -> Result<()> {
    if specs.is_empty() {
        anyhow::bail!("specify at least one plugin name");
    }
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    let settings_path = crate::paths::settings_json()?;
    let inventory_path = crate::paths::installed_plugins_json()?;
    let mut settings = crate::settings::load(&settings_path)?;
    let inventory = crate::inventory::load(&inventory_path)?;
    let mut failed = 0usize;
    let mut any = false;

    for spec in &specs {
        let ep = crate::settings::enabled_plugins(&settings)
            .cloned()
            .unwrap_or_default();
        let plugs = crate::inventory::plugins(&inventory)
            .cloned()
            .unwrap_or_default();
        let qualified =
            match crate::commands::remove::resolve_installed_plugin(spec, &ep, &plugs, &known) {
                Ok(q) => q,
                Err(e) => {
                    eprintln!("{} {e}", "✗".red());
                    failed += 1;
                    continue;
                }
            };
        let ep = crate::settings::enabled_plugins_mut(&mut settings);
        if enable {
            ep.insert(qualified.clone(), Value::Bool(true));
            println!("{} enabled plugin {}", "✓".green(), qualified);
        } else {
            ep.insert(qualified.clone(), Value::Bool(false));
            println!("{} disabled plugin {}", "•".yellow(), qualified);
        }
        any = true;
    }

    if any {
        crate::settings::save(&settings_path, &settings)?;
    }
    anyhow::ensure!(failed == 0, "{failed} plugin enable/disable(s) failed");
    Ok(())
}
