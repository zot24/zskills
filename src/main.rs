mod agent_skill;
mod cli;
mod commands;
mod error;
mod git;
mod inventory;
mod lockfile;
mod manifest;
mod marketplace;
mod mcp;
mod paths;
mod reconcile;
mod settings;
#[cfg(feature = "skills-sh")]
mod skills_sh;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    cli.run()
}
