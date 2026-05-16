# zskills

[![CI](https://github.com/zot24/zskills/actions/workflows/ci.yml/badge.svg)](https://github.com/zot24/zskills/actions/workflows/ci.yml)
[![Release Please](https://github.com/zot24/zskills/actions/workflows/release-please.yml/badge.svg)](https://github.com/zot24/zskills/actions/workflows/release-please.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A declarative package manager for agentic coding CLIs — skills, plugins, and MCP servers managed from a single TOML manifest. Written in Rust.

Think `brew bundle` for your AI coding setup: `skills.toml` declares intent, the runtime's config (e.g. Claude Code's `~/.claude/settings.json`, `installed_plugins.json`, MCP server entries) gets reconciled atomically. Works with any marketplace tap and any GitHub repo that exposes a skill under `skills/<name>/SKILL.md`.

**Supported runtimes:**

| Runtime | Status | What's managed |
|---|---|---|
| [Claude Code](https://claude.com/claude-code) | ✅ supported | plugins (via marketplaces), Agent Skills (`~/.claude/skills/`), MCP servers (all five known scopes) |
| Grok-based CLIs (e.g. [`grok-cli`](https://github.com/superagent-ai/grok-cli)) | planned | skills (`~/.agents/skills/`), MCP servers |
| [Codex](https://github.com/openai/codex) | planned | skills, MCP servers |
| xAI's official CLI | planned | once it ships |

The data model in `src/mcp.rs` is intentionally runtime-agnostic — adding a new runtime is a new loader, not a new package manager.

## Documentation

- **[Commands reference](docs/commands.md)** — every subcommand, every flag, with defaults and notes
- **[Use cases](docs/use-cases.md)** — 10 common workflows (bootstrap a machine, centralize duplicates, etc.)
- **[Architecture](docs/architecture.md)** — the three-state model (intent / inventory / activation) and how reconciliation works
- **[Troubleshooting](docs/troubleshooting.md)** — diagnostic recipes for common errors
- **[Changelog](CHANGELOG.md)**

## Install

```bash
cargo install --git https://github.com/zot24/zskills
```

Requires `git` on `$PATH`.

### Companion Agent Skill

This repo also ships an Agent Skill that teaches Claude how to use zskills. Add it to your `skills.toml`:

```toml
[[agent_skills]]
source = "zot24/zskills"
```

Then `zskills sync`. Claude will reach for zskills with confidence for install / search / sync / doctor / MCP management.

## Why

Existing tooling is per-runtime, per-language, and per-primitive: there's a JavaScript shim for Claude skills, a separate flow for MCP servers, no way to track ownership across machines, no declarative reproducibility. zskills is a single static binary that:

- Manages **skills**, **plugins**, and **MCP servers** from one manifest.
- Preserves every unknown field in your settings JSON (hooks, permissions, env, anything the runtime adds later) — atomic round-trips, never clobbers.
- Reconciles intent (manifest) ↔ inventory (what's actually installed) ↔ activation (what's enabled) in one pass.
- Treats secrets carefully: only `${VAR}` references and key names ever land in zskills's data structures or output, never values.
- Is designed for multiple agentic CLI runtimes — Claude Code today, more as their primitives stabilize.

## Commands

```
zskills list [-v]                       # what's installed; agent skills grouped by source
zskills install <name>                  # add to enabledPlugins
zskills install -i                      # browse all marketplace plugins, fuzzy-pick one
zskills remove  <name>                  # disable + drop inventory entry (keep bytes — apt style)
zskills remove  -i                      # multi-select from enabled plugins to remove
zskills purge   <name>                  # also delete bytes
zskills enable  <name>                  # flip on without (un)installing
zskills disable <name>
zskills sync [--file f.toml]            # apply declarative manifest (headline command)
zskills update [<name>...]              # refresh marketplace caches (plugins only)
zskills upgrade [<name>...]             # ONE command: refresh marketplaces + reinstall all agent skills
zskills doctor [--fix]                  # reconcile disk ↔ inventory ↔ settings
zskills scan [path]                     # find project-scope skills across a tree
zskills migrate <project>               # promote one project's skills to user scope
zskills migrate-skill <name>            # promote ONE skill across every project in a tree
zskills migrate-all <dir>               # interactive: walk a tree, prompt per skill
zskills search <query> [-i]             # -i picks a result and installs it
zskills marketplace add|remove|list|update
```

`<name>` accepts unqualified (`servarr`) when unambiguous, or `name@marketplace` (`servarr@zot24-skills`) to disambiguate.

`-i` / `--interactive` is available on `install`, `remove`, and `search`. If `fzf` is on `$PATH` it gets used automatically; otherwise zskills falls back to a built-in fuzzy picker. Disable fzf detection with `ZSKILLS_NO_FZF=1`.

## Declarative manifest (skills.toml)

```toml
# Claude Code plugins (marketplace-based, controlled via enabledPlugins)
[[skills]]
name = "umbrel-app"
marketplace = "zot24-skills"

[[skills]]
name = "github"
marketplace = "claude-plugins-official"

[[skills]]
name = "cloudflare"
marketplace = "cloudflare"

# Agent Skills (older raw-SKILL.md format, installed to ~/.claude/skills/)
[[agent_skills]]
source = "jakubkrehel/make-interfaces-feel-better"

# Install only one specific skill out of a multi-skill repo:
[[agent_skills]]
source = "owner/multi-skill-repo"
name = "specific-skill"

# npm-distributed agent skills (npm install -g <pkg> + post-install)
[[agent_skills]]
npm = "get-shit-done-cc"

# Packages that need a custom installer command:
[[agent_skills]]
npm = "some-tool"
install_cmd = "npx some-tool setup"
```

```bash
zskills sync               # apply: enables anything in the manifest, disables anything not
zskills sync --dry-run     # preview
```

`zskills sync` is idempotent. Run it anywhere — same machine, new machine — and the result matches the manifest. Plugins flip via `enabledPlugins`. Agent Skills get `git clone`d and copied into `~/.claude/skills/<name>/`. Run it on every fresh checkout.

## Scanning project-scope skills

If you've enabled skills inside `.claude/settings.json` or dropped Agent Skills into `.claude/skills/<name>/` across many repos, `zskills` can find them all:

```bash
zskills scan ~/Desktop/code
```

Then promote a project's skills to user scope (so they're available everywhere):

```bash
zskills migrate ~/Desktop/code/some-project              # add to user; leave project alone
zskills migrate ~/Desktop/code/some-project --remove-from-project
zskills migrate ~/Desktop/code/some-project --dry-run
```

Both plugin enables (`enabledPlugins`) and Agent Skill directories under `.claude/skills/` get promoted.

## Promoting duplicated skills across a tree

If the same skill exists in many projects (common with authored-in-place agent skills), `migrate-skill` and `migrate-all` move it to user scope and optionally clean every project.

```bash
# Promote one skill found in many projects
zskills migrate-skill performance-tracking-skill --root ~/Desktop/code --dry-run
zskills migrate-skill performance-tracking-skill --root ~/Desktop/code --remove-from-all
zskills migrate-skill performance-tracking-skill --root ~/Desktop/code --source owner/repo

# Interactive sweep: walk the whole tree and prompt per duplicated skill
zskills migrate-all ~/Desktop/code --threshold 3       # only skills in ≥3 projects
zskills migrate-all ~/Desktop/code --threshold 2 -y    # non-interactive (no source, keep project copies)
```

`migrate-skill` hashes each project's copy of the named skill and warns if content has diverged before picking the first as canonical. Both commands append `[[agent_skills]]` entries to your `skills.toml` so the migration is reproducible — and accept `--source owner/repo` (or prompt for it interactively) so skills with an upstream repo get tracked and refreshed by future `sync` runs. Skills without a known upstream get a `name`-only manifest entry; they're tracked in inventory but `sync` won't fetch them.

## Design

- **`~/.claude/settings.json` is authoritative for what runs.** `installed_plugins.json` is the inventory. Three states matter: on-disk-and-enabled, on-disk-and-disabled, on-disk-and-orphaned. `doctor` reconciles them.
- **Atomic JSON writes.** Tempfile + rename. Preserves every unknown field — `hooks`, `permissions`, `env`, anything Claude Code adds — round-trip safe.
- **`name@marketplace` qualification.** Same syntax Claude Code uses. No invention.
- **Git is shelled out.** Reuses your credential helpers; no `libgit2` to bundle.
- **No async network unless needed.** Marketplace caches live on disk; `git pull` does the work.

## Optional features

`zskills` ships vanilla by default — the binary only talks to local marketplace caches you already trust. Optional capabilities are gated behind cargo features so they aren't even compiled in unless you ask for them.

| Feature | What it adds | How to enable |
|---|---|---|
| `skills-sh` | Federated `search` + `install` against the [skills.sh](https://www.skills.sh) remote index. Registers a `remote-index` source type, dispatches to its REST API, requires `ZSKILLS_SKILLS_SH_API_KEY` at runtime. | `cargo install --git https://github.com/zot24/zskills --features skills-sh` |

Without the feature, `zskills marketplace add skills.sh` returns *"unrecognized marketplace source"* — no dormant code paths, no env-var detection, nothing.

## Status

v0.6 — `search <query>` across registered marketplaces, `marketplace add-recommended` seeder for trusted defaults, optional `skills-sh` federation behind a cargo feature.

v0.3 — adds `migrate-skill` (promote ONE skill across all projects in a tree) and `migrate-all` (interactive sweep with per-skill prompts for source + cleanup). Agent skill entries now support optional `source` (local-only entries are valid). Manifest writes preserve existing comments via `toml_edit`.

v0.2 — declarative `sync` for both plugins and Agent Skills, scan/migrate over both, atomic settings.json round-trip, doctor reconciliation across all three states (settings, inventory, disk).

Lockfile semantics, `info`, and full version pinning still to come.

## Roadmap: third-party marketplace drivers

The `skills-sh` cargo feature is a holding pattern for *"one known remote index, ship it as opt-in code."* It's the right shape today; it stops being the right shape if two or three other remote indexes want to plug in.

When that happens, the planned move is a **subprocess plugin protocol** rather than more cargo features:

- Drivers ship as separate binaries on `$PATH`, named `zskills-driver-<name>` (git-style extension pattern).
- Marketplaces with `source.source = "remote-index"` get matched to a driver by URL host (or an explicit `driver` field on the entry).
- zskills exec's the driver with a JSON request on stdin (`{"method": "search", "params": {"query": "stripe", "limit": 25}}`) and reads a JSON response from stdout. Methods: `search`, `resolve` (slug → install coordinates), and optionally `audit`.
- Drivers can be written in any language, version independently, and ship under their own trust models.

What this buys: third parties can publish drivers without forking zskills, and the core binary doesn't grow with each new index. What it costs: a stable wire protocol commitment, subprocess overhead, and a public surface to support long-term. **It's worth it once there are at least two or three confirmed driver consumers** — building it for one half-confirmed consumer (skills.sh, gated behind API keys) is premature abstraction. Until then, `--features skills-sh` is the right ergonomic.

If you maintain a remote index that would want to plug in, [open an issue](https://github.com/zot24/zskills/issues) so we can size demand.

## Sister project

The [`zot24/skills`](https://github.com/zot24/skills) marketplace is what this tool was originally built to manage.

## License

MIT
