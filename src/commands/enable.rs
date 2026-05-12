use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;

pub fn run(specs: Vec<String>, enable: bool) -> Result<()> {
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    let settings_path = crate::paths::settings_json()?;
    let mut settings = crate::settings::load(&settings_path)?;

    for spec in &specs {
        let qualified = crate::marketplace::resolve_spec(spec, &known)
            .unwrap_or_else(|_| spec.to_string());
        let ep = crate::settings::enabled_plugins_mut(&mut settings);
        if enable {
            ep.insert(qualified.clone(), Value::Bool(true));
            println!("{} enabled {}", "✓".green(), qualified);
        } else {
            ep.insert(qualified.clone(), Value::Bool(false));
            println!("{} disabled {}", "•".yellow(), qualified);
        }
    }

    crate::settings::save(&settings_path, &settings)?;
    Ok(())
}
