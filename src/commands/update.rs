use anyhow::Result;
use owo_colors::OwoColorize;

pub fn run(_skills: Vec<String>) -> Result<()> {
    // Refresh every marketplace tap; Claude Code itself handles version negotiation.
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    for mp_name in known.keys() {
        let repo = crate::paths::marketplaces_dir()?.join(mp_name);
        if repo.exists() {
            print!("Updating {} ... ", mp_name);
            match crate::git::pull(&repo) {
                Ok(()) => println!("{}", "ok".green()),
                Err(e) => println!("{} ({})", "fail".red(), e),
            }
        }
    }
    println!(
        "\nMarketplaces refreshed. Restart Claude Code to pull latest skill bytes."
    );
    Ok(())
}
