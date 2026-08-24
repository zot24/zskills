use anyhow::Result;
use owo_colors::OwoColorize;

#[allow(dead_code)] // folded into `marketplace update`; kept for the all-tap refresh print
pub fn run(_skills: Vec<String>) -> Result<()> {
    // Refresh every marketplace; Claude Code itself handles version negotiation.
    let known = crate::marketplace::load_known(&crate::paths::known_marketplaces_json()?)?;
    let pins = crate::marketplace::load_pins()?;
    let mut pinned = 0usize;
    for mp_name in known.keys() {
        let repo = crate::paths::marketplaces_dir()?.join(mp_name);
        if !repo.exists() {
            continue;
        }
        print!("Updating {} ... ", mp_name);
        match crate::marketplace::refresh(mp_name, &repo, pins.get(mp_name).map(String::as_str)) {
            Ok(outcome) => {
                if matches!(outcome, crate::marketplace::Refresh::Pinned { .. }) {
                    pinned += 1;
                }
                println!("{}", crate::marketplace::refresh_label(&outcome).green());
            }
            Err(e) => println!("{} ({:#})", "fail".red(), e),
        }
    }
    println!("\nMarketplaces refreshed. Restart Claude Code to pull latest skill bytes.");
    if pinned > 0 {
        println!(
            "{}",
            format!(
                "{} marketplace(s) held at their pin in skills.toml — `zskills marketplace list` shows which.",
                pinned
            )
            .dimmed()
        );
    }
    Ok(())
}
