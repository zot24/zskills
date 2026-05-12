mod cli;
mod commands;
mod error;
mod git;
mod inventory;
mod lockfile;
mod manifest;
mod marketplace;
mod paths;
mod reconcile;
mod settings;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    cli.run()
}
