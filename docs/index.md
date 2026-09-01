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
[defaults]
harnesses = ["claude", "pi", "hermes", "kimi", "grok", "codex"]
mcp_harnesses = ["claude", "pi", "hermes", "kimi", "grok", "codex"]

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

# Many skills from one repo: `skills` is the plural of `name`, so one stanza
# carries one `source`. An entry declares `name` or `skills`, not both.
[[agent_skills]]
source = "mattpocock/skills"
skills = ["prototype", "research", "tdd", "wayfinder"]

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
zskills skill upgrade                   # refresh Agent Skills from origin
zskills list                            # see what's installed (prints which harnesses can see each name)
zskills doctor                          # reconcile disk ↔ inventory ↔ settings
```

## Commands

```text
zskills list [-v]
zskills plugin install|remove|purge|enable|disable
zskills skill install|remove|upgrade
zskills mcp add|remove
zskills sync [--file f.toml] [--prune]
zskills doctor [--fix]
zskills scan|migrate|migrate-skill|migrate-all|search
zskills marketplace add|remove|list|update|add-recommended
```

Bare `install`/`remove`/`purge`/`enable`/`disable`/`update`/`upgrade` exit 2.

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
