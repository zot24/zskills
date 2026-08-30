---
name: zskills
description: Declarative package manager for agentic coding CLIs — manages skills, plugins, and MCP servers across user/project/local scopes via a single skills.toml manifest. Use when installing, removing, listing, syncing, or troubleshooting Claude Code plugins, Agent Skills, or MCP servers; when reconciling drift between settings.json and on-disk state; when bootstrapping a fresh machine; or when consolidating scattered configs from many projects into one global place.
allowed-tools: Bash, Read, Edit, Write
---

# zskills

A single Rust binary that manages three primitives for Claude Code (and, by design, future agentic CLIs like grok-cli / Codex):

- **Plugins** — marketplace-distributed, controlled via `~/.claude/settings.json` → `enabledPlugins`
- **Agent Skills** — raw `SKILL.md` directories in the shared hub `~/.agents/skills/<name>/`, symlinked into each harness root (`~/.claude/skills/`, `~/.codex/skills/`, `~/.hermes/skills/<category>/`); Pi and Grok scan the hub directly
- **MCP servers** — declared in `~/.claude.json`, `<project>/.mcp.json`, `<project>/.claude/settings.json`, `<project>/.claude.local/settings.json`, and (read-only) `/Library/Application Support/ClaudeCode/managed-settings.json`

Everything reconciles atomically from one `skills.toml` manifest. Preserves every unknown JSON field — hooks, permissions, env, anything Claude Code adds.

## When to use

- **Installing things**: `zskills plugin install <name>` / `zskills skill install <owner>/<repo>` (add `-i` for a picker) is faster and safer than editing `settings.json` by hand.
- **Bootstrapping a machine**: drop your `skills.toml` in `~/.config/zskills/`, run `zskills sync`. Reproducible.
- **MCP visibility**: `zskills list` aggregates every MCP across all 6 known sources with plugin attribution — no other tool surfaces plugin-injected servers separately.
- **MCP validation**: `zskills doctor` statically checks command-on-PATH, unset `${VAR}` refs, deprecated SSE transport — without spawning anything.
- **Consolidating scattered configs**: project-scope skills/MCPs duplicated across many repos can be promoted to user scope (skills today; MCPs is a roadmap item, see issue #14).
- **Diagnosing drift**: `zskills doctor` reports broken references, orphans, dangling inventory entries. `--fix` cleans (without deleting bytes).

## When NOT to use

- Don't reach for zskills to *configure* a single MCP one-off — `claude mcp add` is fine for that, and zskills's value is the manifest-driven reproducibility, not CRUD parity.
- Don't use zskills to manage Claude Code itself (it doesn't install/upgrade Claude Code, only skills/plugins/MCPs).

## Commands

```bash
# Listing
zskills list                       # what's installed across plugins, Agent Skills, MCPs
zskills list --paths               # also show on-disk location of every entry
zskills list -v                    # expand grouped npm-bundle agent skills
zskills list --json                # machine-readable for scripting

# Search (marketplaces)
zskills search <query>             # substring-match across registered marketplaces
zskills search <query> -i          # also opens an interactive picker; selection installs

# Plugins (marketplace-distributed)  — group: `zskills plugin`
zskills plugin install <name>      # name@marketplace if ambiguous
zskills plugin install <name>@<mp> # explicitly qualified
zskills plugin install -i          # fuzzy-pick from all marketplaces
zskills plugin remove <name>       # apt-style: disable + drop inventory, keep bytes
zskills plugin remove -i           # multi-select picker over enabled plugins
zskills plugin purge <name>        # also delete bytes
zskills plugin enable <name>       # flip on (must already be installed)
zskills plugin disable <name>      # flip off, bytes stay

# Agent Skills from a git repo — group: `zskills skill`
zskills skill install zot24/zskills                        # owner/repo — clone, survey, install
zskills skill install https://github.com/owner/repo.git    # full git URL works too
zskills skill install owner/big-collection -i              # multi-select picker for many skills
zskills skill install owner/big-collection --all           # confirm "install all" when >5 skills
zskills skill install owner/repo --skill <name>            # just one out of the source
zskills skill remove <name>        # delete bytes + inventory
zskills skill upgrade [<name>...]  # refresh git/npm Agent Skills + marketplace caches
zskills skill register-pi-hub      # list the hub path once in Pi's settings.json

# MCP servers — group: `zskills mcp`
zskills mcp add <name> ...         # declare in skills.toml + the runtime map
zskills mcp remove <name>

# Declarative manifest (the headline command)
zskills sync                       # apply ~/.config/zskills/skills.toml
zskills sync --dry-run             # preview
zskills sync --prune               # destructive removals: delete agent skill bytes and
                                   # MCP entries not in manifest
zskills sync --file ./skills.toml  # project-local manifest (NOT auto-loaded)
zskills sync --adopt               # inverse of --prune: append orphans to the manifest
zskills sync --force               # override the silent mass-disable guard

# Diagnostics
zskills doctor                     # report plugin / inventory / MCP issues
zskills doctor --fix               # clean dangling references (never deletes bytes)

# Project-scope discovery + promotion
zskills scan [path]                # find project-scope skills across a tree
zskills migrate <project>          # promote one project's skills to user scope
zskills migrate <project> --remove-from-project
zskills migrate-skill <name>       # promote ONE skill across every project
zskills migrate-all <dir>          # interactive sweep, prompt per duplicated skill

# Marketplace registry
zskills marketplace add <owner/repo>
zskills marketplace add-recommended  # seed Anthropic-official defaults
zskills marketplace list
zskills marketplace update [name]    # git pull / tarball fetch
zskills marketplace remove <name>
```

## Manifest schema (skills.toml)

```toml
# ──── Plugins (marketplace-distributed) ────
[[skills]]
name = "umbrel-app"
marketplace = "zot24-skills"

[[skills]]
name = "github"
marketplace = "claude-plugins-official"

# ──── Agent Skills (raw SKILL.md format) ────
# Git repo with skills/<name>/SKILL.md
[[agent_skills]]
source = "jakubkrehel/make-interfaces-feel-better"

# Pick ONE skill out of a multi-skill repo
[[agent_skills]]
source = "owner/multi-skill-repo"
name = "specific-skill"

# Agent Skills from a path inside a marketplace clone (zskills 1.3.0+)
[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
name = "wiki-manager"
harnesses = ["pi", "grok"]

# npm package with glob ownership over installed directories
[[agent_skills]]
npm = "get-shit-done-cc"
claims = ["gsd-*"]

# Local-only entry (just tracked, never refreshed)
[[agent_skills]]
name = "my-internal-tool"

# ──── MCP servers ────
# Stdio (process spawn)
[[mcps]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
scope = "user"   # default; also "project" or "local"

# HTTP / remote MCP
[[mcps]]
name = "linear"
url = "https://mcp.linear.app/mcp"
transport = "http"   # optional; inferred from `url`
scope = "user"

# Stdio + mcp-remote proxy (args reference ${VAR} for credentials)
[[mcps]]
name = "honcho"
command = "npx"
args = [
  "mcp-remote",
  "https://mcp.honcho.dev",
  "--header", "Authorization:${HONCHO_AUTH}",
  "--header", "X-Honcho-User-Name:${USER_NAME}",
]
env = { HONCHO_AUTH = "${HONCHO_AUTH}", USER_NAME = "${USER}" }
scope = "user"
```

### Harness visibility — the trap

An install is not automatically visible to every agent CLI. **The two primitives have opposite
defaults**, and `zskills list` prints the resolved set after each name (`[claude · pi · grok | skipped kimi]`).

| Primitive | Default harnesses | Why |
|---|---|---|
| `[[skills]]` (plugin) | **`claude` only** | `enabledPlugins` is a Claude Code file; no other harness reads it |
| `[[agent_skills]]` | **every harness whose home dir exists** (minus unsupported) | the hub is harness-neutral |

So moving a skill from `[[agent_skills]]` to a plugin **silently drops it out of Pi, Grok, Codex and
Hermes**. To fan a plugin out, name the harnesses explicitly — its nested `skills/<name>/` trees are
then *copied* into the hub (copied, not linked: plugin cache paths are version-stamped and a link
would break on upgrade):

```toml
[[skills]]
name = "mattpocock-skills"
marketplace = "skills"
harnesses = ["claude", "pi", "grok", "codex", "hermes"]
```

Set it once for everything with `[defaults]`, override per entry, or one-shot with `--harness`:

```toml
[defaults]
harnesses = ["claude", "pi", "grok", "codex", "hermes"]
mcp_harnesses = ["claude"]
```

Support as of 1.2.0: `claude`, `pi`, `grok`, `codex`, `hermes` all take skills; **`kimi` is
unsupported** (no cited skills directory under `~/.kimi-code/` — zskills refuses to invent one).
MCP servers are Claude-only. Pi reads the hub only once its absolute path is in
`~/.pi/agent/settings.json` `skills: []` — `zskills skill register-pi-hub` puts it there. Hermes
files skills by category, defaulting to `software-development` (`--category` overrides).

Some source repos ship both a plugin and Agent Skills. `nvk/llm-wiki` is one. Do not fan plugin `wiki` to Pi or Grok. That copies the Claude nested skill. Claude talks about `/wiki:*`. Pi does not have `/wiki:*`. Declare the plugin with `harnesses = ["claude"]`. Declare the OpenCode Agent Skills from `plugins/llm-wiki-opencode/skills` with `harnesses = ["pi", "grok"]`. Do not rely on `[defaults].harnesses` for those rows.

Requires zskills 1.3.0 or later. 1.3.0 is the series after 1.2.0 that copies Agent Skills from a marketplace clone via `marketplace` and `path`. A 1.2 binary ignores unknown keys. It treats `marketplace` and `path` as absent. The `[[agent_skills]]` rows then become local-only. `sync` prints `✓ applied.` and copies nothing from the OpenCode tree.

```toml
[[marketplaces]]
name = "llm-wiki"
repo = "nvk/llm-wiki"

[[skills]]
name = "wiki"
marketplace = "llm-wiki"
harnesses = ["claude"]

[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
name = "wiki-manager"
harnesses = ["pi", "grok"]

[[agent_skills]]
marketplace = "llm-wiki"
path = "plugins/llm-wiki-opencode/skills"
name = "wiki-query"
harnesses = ["pi", "grok"]
```

Set `harnesses` on both rows. Do not omit them. A plugin row that omits `harnesses` inherits `[defaults].harnesses`. An Agent Skill row that omits `harnesses` inherits every harness whose home exists.

| Harness | What it consumes |
|---|---|
| Claude Code | Plugin `wiki@llm-wiki`. Slash commands `/wiki:*`. Nested Claude `wiki-manager`. `bin/llm-wiki` in the plugin cache. |
| Pi | Hub Agent Skills `wiki-manager` and `wiki-query` at `~/.agents/skills/`. Invoke with `/skill:wiki-manager`. |
| Grok | The same hub Agent Skills. Grok scans `~/.agents/skills/`. Slash `/wiki-manager` and `/wiki-query`. |

zskills does not install `scripts/pi-wiki-query`. That is a launcher. It is not an Agent Skill. zskills does not provide Grok slash `/wiki:*`. zskills does not write `~/.grok/config.toml`. It does not install a Grok plugin.

Do not run `zskills skill install nvk/llm-wiki`. That command sees `.claude-plugin/marketplace.json` and redirects to `marketplace add`. Write the `[[agent_skills]]` rows. Apply with `zskills sync`.

### Secret handling

Values in `env` / `headers` should be `${VAR}` references — the manifest is reproducible and shareable, so credentials stay in your shell environment. `${VAR}` is preserved verbatim on write; never resolved.

### Scope routing (where sync writes MCPs)

| scope | File |
|---|---|
| `user` (default) | `~/.claude.json` |
| `project` | `<cwd>/.mcp.json` |
| `local` | `<cwd>/.claude.local/settings.json` |

`managed` is read-only — not writable from manifest.

## Common workflows

### Bootstrap a fresh machine
```bash
cargo install --git https://github.com/zot24/zskills
mkdir -p ~/.config/zskills && cp <somewhere>/skills.toml ~/.config/zskills/
zskills marketplace add-recommended
zskills sync
```

### Install one thing fast
```bash
# Plugin from a registered marketplace
zskills plugin install firecrawl@zot24-skills

# Agent Skill directly from any git repo (no manifest edit required)
zskills skill install zot24/zskills

# Browse marketplace plugins
zskills plugin install -i
```

### Repo-install size policy
When `zskills skill install <owner>/<repo>` discovers many skills, it does not silently flood the hub:

| Skills in repo | Default behavior |
|---|---|
| 1 | install it |
| 2–5 | install all (silent) |
| > 5 | abort + print sample + suggest `-i` or `--all` |

Pass `-i` for a picker or `--all` for explicit consent on large collections.

### Marketplace repos vs skill repos
If the repo is a Claude Code marketplace (has `.claude-plugin/marketplace.json`), `zskills skill install <owner>/<repo>` redirects to `zskills marketplace add <owner/repo>` instead. That's the canonical path for plugins.

### See what's where
```bash
zskills list             # everything
zskills list --paths     # also show on-disk locations
```

### Diagnose problems
```bash
zskills doctor           # surface drift + MCP issues (no spawning)
zskills doctor --fix     # clean dangling settings/inventory references
```

### Centralize MCPs across scopes
Today the flow is two steps:
1. `zskills list` to see all current MCPs across `~/.claude.json`, `<proj>/.mcp.json`, etc.
2. Add `[[mcps]]` entries to `skills.toml`, then `zskills sync --prune` to write them at user scope and remove the duplicates from other scopes. **Plugin-injected MCPs are protected** — sync never prunes those.

A `dump-mcps` helper to automate step 2 is on the roadmap (see issue #14).

## Mental model

zskills tracks three states per primitive:

| State | Lives at | Authoritative for |
|---|---|---|
| **Intent** | `skills.toml` | What you *want* installed |
| **Inventory** | `installed_plugins.json` / `.zskills.json` | What *exists* on disk |
| **Activation** | `settings.json` → `enabledPlugins`, `~/.claude.json` → `mcpServers` | What's *running* |

`sync` reconciles intent → activation by writing settings files and (for agent skills) cloning/pulling repos. `doctor` cross-checks all three for drift.

## Safety guarantees

- **Atomic JSON writes**: tempfile + rename. Preserves every unknown top-level key — hooks, permissions, env, plugin/MCP entries zskills doesn't know about.
- **`./skills.toml` NOT auto-loaded** since v0.5.1 (it caused destructive surprises). Pass `--file ./skills.toml` explicitly.
- **No removal without `--prune`** for agent skills and MCPs. The default is additive.
- **`doctor` never deletes bytes**. Use `purge` for that — deliberate, explicit.
- **Plugin-injected MCPs never pruned**. zskills knows they're owned by their plugin.
- **Managed-scope MCPs never written**. IT-deployed, read-only.

## Troubleshooting quick reference

| Symptom | Likely fix |
|---|---|
| `no skills.toml found` | Create `~/.config/zskills/skills.toml` or pass `--file <path>` |
| `skill X is ambiguous` | Qualify with `name@marketplace` |
| `enabled but NOT installed` | Restart Claude Code (will install on next boot) or `doctor --fix` |
| `agent skill in inventory, missing on disk` | `sync` to re-fetch, or `doctor --fix` to drop inventory entry |
| MCP `doctor` says `command not found` | Install the binary, or fix the path in the manifest |
| MCP `doctor` says `env var X referenced but not set` | Export `X` in your shell |
| MCP `doctor` says `sse: deprecated` | Migrate the entry to `transport = "http"` |
| Sync wants to disable plugins you want to keep | Add them to `skills.toml` so manifest matches intent |

Full reference: <https://zskills.zot24.com> · Source: <https://github.com/zot24/zskills>
