use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "zskills",
    version,
    about = "Package manager for Claude Code skills",
    long_about = "Declarative install, enable, and reconciliation across Claude Code marketplaces.\n\
                  Treats skills.toml as intent and ~/.claude/settings.json + installed_plugins.json as state."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List installed skills with their enabled/disabled/orphaned status
    List {
        /// Output as JSON for scripting
        #[arg(long)]
        json: bool,
    },

    /// Install + enable one or more skills (format: name or name@marketplace)
    Install {
        /// Skills to install
        #[arg(required = true)]
        skills: Vec<String>,
    },

    /// Disable + drop from inventory (keeps bytes on disk)
    Remove {
        #[arg(required = true)]
        skills: Vec<String>,
    },

    /// Like remove, but also deletes the bytes from ~/.claude/plugins
    Purge {
        #[arg(required = true)]
        skills: Vec<String>,
    },

    /// Flip enabledPlugins on (skill must already be installed)
    Enable {
        #[arg(required = true)]
        skills: Vec<String>,
    },

    /// Flip enabledPlugins off (skill stays installed)
    Disable {
        #[arg(required = true)]
        skills: Vec<String>,
    },

    /// Update one or more skills (or all) to latest from their marketplace
    Update {
        /// Specific skills to update; empty = all
        skills: Vec<String>,
    },

    /// Apply a declarative skills.toml manifest to the current scope
    Sync {
        /// Path to skills.toml (default: ./skills.toml then ~/.config/zskills/skills.toml)
        #[arg(long)]
        file: Option<PathBuf>,

        /// Show what would change without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Reconcile disk ↔ inventory ↔ settings; report orphans + mismatches
    Doctor {
        /// Attempt to fix issues automatically
        #[arg(long)]
        fix: bool,
    },

    /// Scan a directory tree for project-scope skill installations
    Scan {
        /// Root directory to walk (default: current directory)
        path: Option<PathBuf>,

        /// Maximum directory depth
        #[arg(long, default_value_t = 4)]
        depth: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Promote project-scope skills to user scope; optionally remove from project
    Migrate {
        /// Project directory to migrate from
        path: PathBuf,

        /// Remove the migrated entries from the project's .claude/settings.json
        #[arg(long)]
        remove_from_project: bool,

        /// Show what would change without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Marketplace (tap) management
    #[command(subcommand)]
    Marketplace(MarketplaceCmd),
}

#[derive(Subcommand)]
pub enum MarketplaceCmd {
    /// Add a marketplace tap (owner/repo or full git URL)
    Add { source: String },

    /// Remove a marketplace tap
    Remove { name: String },

    /// List all known marketplaces
    List {
        #[arg(long)]
        json: bool,
    },

    /// Refresh marketplace caches (git pull)
    Update {
        /// Specific marketplace to update; empty = all
        name: Option<String>,
    },
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            Command::List { json } => crate::commands::list::run(json),
            Command::Install { skills } => crate::commands::install::run(skills),
            Command::Remove { skills } => crate::commands::remove::run(skills, false),
            Command::Purge { skills } => crate::commands::remove::run(skills, true),
            Command::Enable { skills } => crate::commands::enable::run(skills, true),
            Command::Disable { skills } => crate::commands::enable::run(skills, false),
            Command::Update { skills } => crate::commands::update::run(skills),
            Command::Sync { file, dry_run } => crate::commands::sync::run(file, dry_run),
            Command::Doctor { fix } => crate::commands::doctor::run(fix),
            Command::Scan { path, depth, json } => crate::commands::scan::run(path, depth, json),
            Command::Migrate {
                path,
                remove_from_project,
                dry_run,
            } => crate::commands::migrate::run(path, remove_from_project, dry_run),
            Command::Marketplace(cmd) => crate::commands::marketplace::run(cmd),
        }
    }
}
