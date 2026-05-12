# Architecture

How zskills models the three sources of truth in your Claude Code install and reconciles between them.

## The three states

| State | Lives at | Authoritative for |
|---|---|---|
| **Intent** | `skills.toml` | What you *want* installed and enabled |
| **Inventory** | `~/.claude/plugins/installed_plugins.json` + `~/.claude/skills/.zskills.json` | What *exists* on disk |
| **Activation** | `~/.claude/settings.json` → `enabledPlugins` | What's currently *running* in a Claude Code session |

The first is what zskills writes from. The second and third are what Claude Code reads. zskills' job is to keep all three consistent.

## Two ecosystems

Claude Code has two parallel skill systems that we manage from one manifest:

### Claude Code plugins
- Distributed via **marketplaces** (Git repos with a `.claude-plugin/marketplace.json`)
- Installed under `~/.claude/plugins/cache/<marketplace>/<name>/<version>/`
- Activation toggle in `settings.json` → `enabledPlugins`
- Inventory: `~/.claude/plugins/installed_plugins.json`
- Qualified name: `<plugin>@<marketplace>` (matches Claude Code's syntax)

### Agent Skills (the older raw-`SKILL.md` format)
- No marketplace — direct from any Git repo with `skills/<name>/SKILL.md`
- Installed under `~/.claude/skills/<name>/`
- No "enabled" flag — files-on-disk *is* the activation
- Inventory: `~/.claude/skills/.zskills.json` (we own this; Claude Code doesn't write it)

Both kinds are declared in `skills.toml`:

```toml
[[skills]]                          # plugin
name = "umbrel-app"
marketplace = "zot24-skills"

[[agent_skills]]                    # agent skill from a repo
source = "owner/repo"

[[agent_skills]]                    # local-only agent skill
name = "my-internal-tool"           # name required; source omitted = no remote refresh
```

## Atomic JSON writes

Claude Code's `settings.json` and `installed_plugins.json` carry *more* than the keys zskills cares about: hooks, permissions, env vars, MCP servers, etc. Losing those on round-trip would be catastrophic. So every write goes through:

1. Read full document as `serde_json::Map<String, Value>` (preserves unknown keys).
2. Mutate only the keys zskills owns (`enabledPlugins`, `extraKnownMarketplaces`, `plugins.*`).
3. Serialize the entire map back to a temp file in the same directory.
4. `std::fs::rename` to the target path (atomic on Unix).

Same idea for `skills.toml` writes: we use `toml_edit::DocumentMut` so existing comments, blank lines, and table ordering survive. An append-only writer (`manifest::append_agent_skill`) checks for duplicates by exact `(source, name)` match before inserting.

## How `sync` reconciles

```
            ┌─────────────────┐
            │   skills.toml   │  ← you edit this
            └────────┬────────┘
                     │
                     ▼
            ┌─────────────────┐
            │     Manifest    │  parsed model
            └────────┬────────┘
                     │
        ┌────────────┴────────────┐
        │ resolve name@marketplace │
        │ via known_marketplaces  │
        └────────┬────────────────┘
                 ▼
        ┌────────────────┐    ┌─────────────────────┐
        │ desired_plugins│    │ desired_agent_skills│
        └───────┬────────┘    └──────────┬──────────┘
                │                        │
                ▼                        ▼
       ┌────────────────┐       ┌────────────────┐
       │ current_plugins│       │ current_agent  │  (from inventory + disk)
       │ from settings  │       │ skills         │
       └────────┬───────┘       └────────┬───────┘
                │                        │
                ▼                        ▼
              DIFF                     DIFF
                │                        │
       ┌────────┴───────┐       ┌────────┴────────┐
       │ enable / disable│       │ install / remove│
       │ via settings.json│      │ via git + copy  │
       └─────────────────┘       └─────────────────┘
```

`sync` is a single atomic apply: it computes the full plan first, prints it, and (unless `--dry-run`) applies the diff. The settings.json write happens once at the end, not per-skill.

## Doctor's three reconciliations

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  settings    │◀──▶│   inventory  │◀──▶│     disk     │
│ enabledPlugins│    │  json files  │    │  ~/.claude/  │
└──────────────┘    └──────────────┘    └──────────────┘
       │                    │                   │
       └────────────────────┴───────────────────┘
                            │
                         doctor
```

Three failure modes:

1. **Settings says enabled, inventory says nothing** — broken reference. Claude Code's startup install will fix this on next launch, OR `doctor --fix` removes the flag.
2. **Inventory says installed, marketplace gone** — orphan from a `marketplace remove`. `doctor` reports; `purge` cleans.
3. **Agent skill inventory entry, no bytes on disk** — someone `rm -rf`'d the skill manually. `doctor --fix` drops the inventory entry; `sync` would reinstall from manifest.

Doctor never deletes plugin bytes. That's `purge`'s job — a deliberate, explicit operation.

## Why git is shelled out

`zskills` runs `git clone --depth 1` and `git pull --ff-only` via `std::process::Command` instead of linking `libgit2`. Reasons:

- Reuses your existing credential helpers (SSH keys, gh CLI, OS keychain) for free.
- Smaller binary (no libgit2 dependency).
- Sparse-checkout and partial-clone work correctly without bespoke handling.
- Errors come back as plain git output, which is what users already know how to read.

The trade-off is one process spawn per fetch, which is fine: marketplace updates and agent-skill installs are rare events.

## Path resolution

```
~/.claude/                              ($CLAUDE_HOME)
├── settings.json                       activation
├── plugins/
│   ├── installed_plugins.json          plugin inventory
│   ├── known_marketplaces.json         tap registry
│   ├── marketplaces/<name>/            tap clones
│   └── cache/<mp>/<name>/<v>/          plugin bytes
└── skills/
    ├── .zskills.json                   agent skill inventory
    └── <name>/SKILL.md                 agent skill bytes

$XDG_CACHE_HOME/zskills/agent-skills/<owner>-<repo>/  source clones
$XDG_CONFIG_HOME/zskills/skills.toml                  manifest (fallback)
./skills.toml                                         manifest (project-scope wins)
```

`CLAUDE_HOME` env var overrides `~/.claude`. `XDG_CACHE_HOME` and `XDG_CONFIG_HOME` are respected. On macOS, zskills uses `~/.config/zskills/` for the manifest by default (matching cargo/starship/atuin) rather than the platform's `~/Library/Application Support/` location.
