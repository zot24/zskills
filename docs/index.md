# zskills

A declarative package manager for agentic coding CLIs — skills, plugins, and MCP servers from a single TOML manifest. Written in Rust.

Think `brew bundle` for your AI coding setup: `skills.toml` declares intent, the runtime's on-disk config (e.g. Claude Code's `~/.claude/settings.json`, `installed_plugins.json`, and MCP server entries) gets reconciled atomically. Works with any marketplace tap, any GitHub repo that exposes a skill under `skills/<name>/SKILL.md`, and npm-distributed skill bundles via `npm = "<pkg>"`.

## Supported runtimes

| Runtime | Status | What's managed |
|---|---|---|
| [Claude Code](https://claude.com/claude-code) | ✅ supported | plugins (via marketplaces), Agent Skills (`~/.claude/skills/`), MCP servers (all five known scopes) |
| Grok-based CLIs (e.g. [`grok-cli`](https://github.com/superagent-ai/grok-cli)) | planned | skills (`~/.agents/skills/`), MCP servers |
| [Codex](https://github.com/openai/codex) | planned | skills, MCP servers |
| xAI's official CLI | planned | once it ships |

The data model is runtime-agnostic; new runtimes are new loaders, not a new tool.

## Install

```bash
cargo install --git https://github.com/zot24/zskills
```

Requires `git` and (for npm-sourced skills) `npm` on `$PATH`.

## Quick start

Create `~/.config/zskills/skills.toml`:

```toml
# Claude Code plugins (marketplace-based)
[[skills]]
name = "umbrel-app"
marketplace = "zot24-skills"

[[skills]]
name = "cloudflare"
marketplace = "cloudflare"

# Agent Skills from a GitHub repo
[[agent_skills]]
source = "jakubkrehel/make-interfaces-feel-better"

# Agent Skills from an npm package (with glob ownership)
[[agent_skills]]
npm = "get-shit-done-cc"
claims = ["gsd-*"]

# MCP servers (v0.7+)
[[mcps]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
scope = "user"

[[mcps]]
name = "linear"
url = "https://mcp.linear.app/mcp"
scope = "user"
```

Then:

```bash
zskills marketplace add-recommended     # seed trusted defaults (Anthropic-official marketplace)
zskills marketplace add zot24/skills    # register additional taps as needed
zskills search <query>                  # find skills across registered marketplaces
zskills sync                            # apply the manifest
zskills upgrade                         # refresh everything from origin
zskills list                            # see what's installed
zskills doctor                          # reconcile disk ↔ inventory ↔ settings
```

## Commands

```text
zskills list [-v]                       # what's installed; agent skills grouped by source
zskills install <name>                  # add a plugin to enabledPlugins
zskills remove  <name>                  # disable + drop inventory (keep bytes)
zskills purge   <name>                  # also delete bytes
zskills enable  <name> / disable <name> # flip the flag only
zskills sync [--file f.toml] [--prune]  # apply declarative manifest
zskills upgrade [<name>...]             # ONE command: refresh everything
zskills update [<name>...]              # refresh marketplace caches (plugins only)
zskills doctor [--fix]                  # reconcile disk ↔ inventory ↔ settings
zskills scan [path]                     # find project-scope skills across a tree
zskills migrate <project>               # promote project skills to user scope
zskills migrate-skill <name>            # promote ONE skill across every project
zskills migrate-all <dir>               # interactive sweep
zskills search <query>                  # keyword search across registered marketplaces
zskills marketplace add|remove|list|update
zskills marketplace add-recommended     # seed trusted defaults (anthropics/claude-plugins-official)
```

Optional capabilities live behind cargo features so the default binary stays minimal — see [Commands → Optional features](./commands.md#optional-features) for the `skills-sh` remote-index driver.

Full reference: [Commands](./commands.md). Workflows and recipes: [Use cases](./use-cases.md). How it works internally: [Architecture](./architecture.md). Stuck? [Troubleshooting](./troubleshooting.md).

## Why

Existing tooling is fragmented across runtimes, primitives, and languages: a JS shim for Claude skills, a separate flow for MCP servers, no shared manifest, no atomic write semantics, no way to take ownership of bundles installed via other tooling. `zskills` is a single static binary that:

- Manages **skills**, **plugins**, and **MCP servers** from one declarative manifest.
- Preserves every unknown field in your settings JSON (hooks, permissions, env, anything the runtime adds later) — atomic round-trips, never clobbers.
- Tracks ownership via inventory tags + glob claims so you can take over skill bundles installed by other tools.
- Reconciles intent ↔ inventory ↔ activation in one pass via `sync`.
- Treats secrets carefully: only `${VAR}` references and key names ever land in zskills's data structures, never values.
- Is built for multiple runtimes — Claude Code today, more planned as their primitives stabilize.

## Source

[github.com/zot24/zskills](https://github.com/zot24/zskills) · MIT license · v0.6+
