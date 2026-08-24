mod agent_skill;
mod claude_cli;
mod cli;
mod commands;
mod error;
mod git;
mod harness;
mod interactive;
mod inventory;
mod lockfile;
mod manifest;
mod marketplace;
mod mcp;
mod paths;
mod reconcile;
mod repo_scanner;
mod settings;
#[cfg(feature = "skills-sh")]
mod skills_sh;
mod timestamp;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match cli.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            if e.downcast_ref::<error::RemovedVerb>().is_some() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
