use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "zskills",
    version,
    about = "Package manager for plugins, Agent Skills, and MCP servers",
    long_about = "Declarative install and reconciliation across Claude Code marketplaces.\n\
                  Treats skills.toml as intent and ~/.claude/settings.json + installed_plugins.json as state.\n\
                  Typed groups: plugin, skill, mcp."
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

        /// Expand grouped agent skills (show every skill name in each source group)
        #[arg(long, short = 'v')]
        verbose: bool,

        /// Show the on-disk location of each entry (plugin install path, agent skill
        /// directory, or the settings file an MCP server is declared in)
        #[arg(long)]
        paths: bool,
    },

    /// Marketplace plugins (`enabledPlugins` + `installed_plugins.json`)
    #[command(subcommand)]
    Plugin(PluginCmd),

    /// Agent Skills in ~/.agents/skills/
    #[command(name = "skill", subcommand)]
    AgentSkill(AgentSkillCmd),

    /// MCP servers in skills.toml and the runtime mcpServers map
    #[command(subcommand)]
    Mcp(McpCmd),

    /// Removed in 1.0. Prints the typed replacement and exits 2.
    #[command(hide = true, disable_help_flag = true)]
    Install {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Removed in 1.0.
    #[command(hide = true, disable_help_flag = true)]
    Remove {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Removed in 1.0.
    #[command(hide = true, disable_help_flag = true)]
    Purge {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Removed in 1.0.
    #[command(hide = true, disable_help_flag = true)]
    Enable {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Removed in 1.0.
    #[command(hide = true, disable_help_flag = true)]
    Disable {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Removed in 1.0. Use `marketplace update`.
    #[command(hide = true, disable_help_flag = true)]
    Update {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
    /// Removed in 1.0. Use `skill upgrade` / `marketplace update`.
    #[command(hide = true, disable_help_flag = true)]
    Upgrade {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },

    /// Apply a declarative skills.toml manifest to the current scope
    Sync {
        /// Path to skills.toml. Default: ~/.config/zskills/skills.toml. (`./skills.toml`
        /// is ignored unless passed explicitly — it caused data loss in v0.5.)
        #[arg(long)]
        file: Option<PathBuf>,

        /// Show what would change without writing
        #[arg(long)]
        dry_run: bool,

        /// Allow destructive removals (deleting agent skill bytes for entries no longer
        /// in the manifest). Without this, sync only enables/disables — it never deletes.
        #[arg(long)]
        prune: bool,

        /// Adopt orphans into the manifest instead of skipping/pruning them. Every
        /// installed agent skill, enabled plugin, and configured MCP that isn't yet
        /// listed gets appended to the manifest. Inverse of `--prune`.
        #[arg(long, conflicts_with = "prune")]
        adopt: bool,

        /// Allow applying a manifest that declares zero entries of a kind while
        /// state still holds many (the silent mass-disable guard).
        #[arg(long)]
        force: bool,
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

        /// Maximum directory depth (needs ≥5 to find .claude/skills/<name>/SKILL.md inside a project)
        #[arg(long, default_value_t = 6)]
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

    /// Promote ONE agent skill across every project that has it.
    MigrateSkill {
        /// Skill name (matches the directory under .claude/skills/<name>/)
        name: String,

        /// Tree to search; default: current directory
        #[arg(long)]
        root: Option<PathBuf>,

        /// Upstream source for the manifest entry (owner/repo or git URL). Omit for local-only.
        #[arg(long)]
        source: Option<String>,

        /// Remove the skill from every project's .claude/skills/ after promotion
        #[arg(long)]
        remove_from_all: bool,

        #[arg(long)]
        dry_run: bool,
    },

    /// Interactive sweep: walk a tree and prompt to promote each duplicated agent skill.
    MigrateAll {
        /// Tree to walk
        dir: PathBuf,

        /// Only consider skills appearing in at least this many projects
        #[arg(long, default_value_t = 2)]
        threshold: usize,

        /// Skip prompts and accept defaults (no source, no project removal)
        #[arg(long, short = 'y')]
        yes: bool,

        #[arg(long)]
        dry_run: bool,
    },

    /// Marketplace (tap) management
    #[command(subcommand)]
    Marketplace(MarketplaceCmd),

    /// Search registered marketplaces by keyword (substring-match on name + description)
    Search {
        /// Query string
        query: String,

        /// Max results to return per marketplace
        #[arg(long, default_value_t = 25)]
        limit: u32,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// After showing results, pick one interactively and install it
        #[arg(short = 'i', long)]
        interactive: bool,
    },
}

#[derive(Subcommand)]
pub enum PluginCmd {
    /// Install + enable a marketplace plugin (name or name@marketplace)
    Install {
        #[arg(short = 'i', long)]
        interactive: bool,
        /// One-shot harness override (comma-separated): claude,pi,hermes,kimi,grok,codex.
        /// Omitted: `[defaults].harnesses`, or Claude only when `[defaults]` is missing.
        #[arg(
            long = "harness",
            value_delimiter = ',',
            value_enum,
            value_name = "HARNESS"
        )]
        harness: Vec<crate::harness::Harness>,
        /// Hermes skills category. Default: software-development.
        #[arg(long, default_value = "software-development", value_name = "CATEGORY")]
        category: String,
        skills: Vec<String>,
    },
    /// Drop enabledPlugins + inventory; keep bytes
    Remove {
        #[arg(short = 'i', long)]
        interactive: bool,
        skills: Vec<String>,
    },
    /// Like remove, and delete recorded installPath bytes
    Purge {
        #[arg(required = true)]
        skills: Vec<String>,
    },
    /// Flip enabledPlugins on (plugin must already be installed)
    Enable {
        #[arg(required = true)]
        skills: Vec<String>,
    },
    /// Flip enabledPlugins off (plugin stays installed)
    Disable {
        #[arg(required = true)]
        skills: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum AgentSkillCmd {
    /// Install Agent Skills from owner/repo or a git URL
    Install {
        #[arg(short = 'i', long, help = "Pick skills from the source interactively")]
        interactive: bool,
        #[arg(long, help = "Install every skill the source provides (>5 needs this)")]
        all: bool,
        #[arg(
            long,
            conflicts_with = "all",
            value_name = "NAME",
            help = "Install only this one skill out of the source"
        )]
        skill: Option<String>,
        #[arg(
            long,
            value_name = "REL",
            help = "Relative path inside the clone to a directory of Agent Skills (skips the marketplace redirect)"
        )]
        path: Option<String>,
        /// One-shot harness override (comma-separated). Omitted: `[defaults].harnesses`.
        #[arg(
            long = "harness",
            value_delimiter = ',',
            value_enum,
            value_name = "HARNESS"
        )]
        harness: Vec<crate::harness::Harness>,
        /// Hermes skills category. Default: software-development.
        #[arg(long, default_value = "software-development", value_name = "CATEGORY")]
        category: String,
        #[arg(
            value_name = "SOURCE",
            help = "owner/repo, https://, git@ or file:// URL"
        )]
        skills: Vec<String>,
    },
    /// Delete bytes + inventory for an Agent Skill
    Remove {
        #[arg(long)]
        force: bool,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// Refresh git/npm Agent Skills (and marketplace caches)
    Upgrade { names: Vec<String> },
    /// Ensure the Agent Skill hub path is listed once in Pi's settings.json
    RegisterPiHub,
    /// Hidden. `skill migrate` is not a verb. Point at `migrate-skill`.
    #[command(hide = true, disable_help_flag = true)]
    Migrate {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum McpCmd {
    /// Declare an MCP server in skills.toml and the runtime map
    Add {
        name: String,
        #[arg(long)]
        transport: Option<String>,
        #[arg(long)]
        command: Option<String>,
        #[arg(long = "args", value_name = "ARG")]
        args: Vec<String>,
        #[arg(long = "env", value_name = "KEY=VALUE", value_parser = crate::commands::mcp::parse_kv)]
        env: Vec<(String, String)>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long = "header", value_name = "KEY=VALUE", value_parser = crate::commands::mcp::parse_kv)]
        header: Vec<(String, String)>,
        #[arg(long, default_value = "user")]
        scope: String,
        #[arg(long)]
        file: Option<PathBuf>,
        /// One-shot MCP harness override. Omitted: `[defaults].mcp_harnesses`.
        #[arg(
            long = "harness",
            value_delimiter = ',',
            value_enum,
            value_name = "HARNESS"
        )]
        harness: Vec<crate::harness::Harness>,
    },
    /// Remove one MCP server from skills.toml and the runtime map
    Remove {
        name: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum MarketplaceCmd {
    /// Add a marketplace tap (owner/repo or full git URL)
    Add { source: String },

    /// Seed the recommended trusted marketplaces (anthropics/claude-plugins-official)
    AddRecommended,

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
            Command::List {
                json,
                verbose,
                paths,
            } => crate::commands::list::run(json, verbose, paths),
            Command::Plugin(cmd) => match cmd {
                PluginCmd::Install {
                    skills,
                    interactive,
                    harness,
                    category,
                } => {
                    if skills
                        .iter()
                        .any(|s| crate::commands::install::is_repo_spec(s))
                    {
                        anyhow::bail!(
                            "plugin install takes name or name@marketplace; use `zskills skill install` for owner/repo"
                        );
                    }
                    crate::commands::install::run(
                        skills,
                        interactive,
                        false,
                        None,
                        None,
                        harness,
                        category,
                    )
                }
                PluginCmd::Remove {
                    skills,
                    interactive,
                } => crate::commands::remove::run(skills, interactive, false),
                PluginCmd::Purge { skills } => crate::commands::remove::run(skills, false, true),
                PluginCmd::Enable { skills } => crate::commands::enable::run(skills, true),
                PluginCmd::Disable { skills } => crate::commands::enable::run(skills, false),
            },
            Command::AgentSkill(cmd) => match cmd {
                AgentSkillCmd::Install {
                    skills,
                    interactive,
                    all,
                    skill,
                    path,
                    harness,
                    category,
                } => crate::commands::agent_skills::install(
                    skills,
                    interactive,
                    all,
                    skill,
                    path,
                    harness,
                    category,
                ),
                AgentSkillCmd::Remove { names, force, file } => {
                    crate::commands::agent_skills::remove(names, force, file)
                }
                AgentSkillCmd::Upgrade { names } => crate::commands::agent_skills::upgrade(names),
                AgentSkillCmd::RegisterPiHub => crate::commands::agent_skills::register_pi_hub(),
                AgentSkillCmd::Migrate { rest } => {
                    crate::commands::stub::run("skill-migrate", &rest)
                }
            },
            Command::Mcp(cmd) => match cmd {
                McpCmd::Add {
                    name,
                    transport,
                    command,
                    args,
                    env,
                    url,
                    header,
                    scope,
                    file,
                    harness,
                } => crate::commands::mcp::add(
                    name,
                    transport,
                    command,
                    args,
                    env,
                    url,
                    header,
                    Some(scope),
                    file,
                    harness,
                ),
                McpCmd::Remove { name, scope, file } => {
                    crate::commands::mcp::remove(name, scope, file)
                }
            },
            Command::Install { rest } => crate::commands::stub::run("install", &rest),
            Command::Remove { rest } => crate::commands::stub::run("remove", &rest),
            Command::Purge { rest } => crate::commands::stub::run("purge", &rest),
            Command::Enable { rest } => crate::commands::stub::run("enable", &rest),
            Command::Disable { rest } => crate::commands::stub::run("disable", &rest),
            Command::Update { rest } => crate::commands::stub::run("update", &rest),
            Command::Upgrade { rest } => crate::commands::stub::run("upgrade", &rest),
            Command::Sync {
                file,
                dry_run,
                prune,
                adopt,
                force,
            } => crate::commands::sync::run(file, dry_run, prune, adopt, force),
            Command::Doctor { fix } => crate::commands::doctor::run(fix),
            Command::Scan { path, depth, json } => crate::commands::scan::run(path, depth, json),
            Command::Migrate {
                path,
                remove_from_project,
                dry_run,
            } => crate::commands::migrate::run(path, remove_from_project, dry_run),
            Command::MigrateSkill {
                name,
                root,
                source,
                remove_from_all,
                dry_run,
            } => crate::commands::migrate_skill::run(name, root, source, remove_from_all, dry_run),
            Command::MigrateAll {
                dir,
                threshold,
                yes,
                dry_run,
            } => crate::commands::migrate_all::run(dir, threshold, yes, dry_run),
            Command::Marketplace(cmd) => crate::commands::marketplace::run(cmd),
            Command::Search {
                query,
                limit,
                json,
                interactive,
            } => crate::commands::search::run(query, limit, json, interactive),
        }
    }
}
