# zskills

A package manager for [Claude Code](https://claude.com/claude-code) skills. Declarative install, multi-marketplace, written in Rust. Manages both **Claude Code plugins** (via marketplaces, `settings.json` → `enabledPlugins`) and **Agent Skills** (the older raw-`SKILL.md` format under `~/.claude/skills/`) from a single manifest.

Think `brew bundle` for Claude Code: a `skills.toml` declares intent, your `~/.claude/settings.json` and `installed_plugins.json` get reconciled atomically. Works with any marketplace tap (`zot24-skills`, `claude-plugins-official`, `cloudflare`, custom) and any GitHub repo that exposes an Agent Skill under `skills/<name>/SKILL.md`. npm-distributed skill bundles (like `get-shit-done-cc`) are supported via an `npm = "<pkg>"` declaration.

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
```

Then:

```bash
zskills marketplace add zot24/skills    # register the marketplace
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
zskills marketplace add|remove|list|update
```

Full reference: [Commands](./commands.md). Workflows and recipes: [Use cases](./use-cases.md). How it works internally: [Architecture](./architecture.md). Stuck? [Troubleshooting](./troubleshooting.md).

## Why

Existing skill managers are JavaScript shims with per-skill Node cold-start, no atomic write semantics, no notion of upgrade-from-origin for marketplaces shipped as tarballs, and no way to take ownership of skill bundles installed via other tooling. `zskills` is a single static binary that wraps Claude Code's existing plugin substrate, atomically — preserving every unknown field in your `settings.json` (hooks, permissions, env, MCP servers), tracking ownership via inventory tags + glob claims, and reconciling all three sources of truth in one pass.

## Source

[github.com/zot24/zskills](https://github.com/zot24/zskills) · MIT license · v0.5+
