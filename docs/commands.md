# Commands reference

Full reference for every `zskills` subcommand. Flags are shown with their defaults. Run `zskills <cmd> --help` for the up-to-date help.

## Conventions

- `<name>` accepts the unqualified skill name (e.g. `servarr`) when unambiguous, or `name@marketplace` (e.g. `servarr@zot24-skills`) when multiple marketplaces declare the same skill. This matches Claude Code's own syntax.
- Most commands print colored output. Set `NO_COLOR=1` to disable, or pipe through `cat` if you need plain text.
- `CLAUDE_HOME=/custom/path` overrides `~/.claude` for testing.
- `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` are respected for manifest/cache locations.

## `list`

What's currently installed, with each item's enabled/disabled/orphaned status. Covers both Claude Code plugins and Agent Skills.

```
zskills list [--json]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | off | Emit a machine-readable JSON document for scripting |

The non-JSON output groups results into four plugin buckets (active, installed-but-disabled, enabled-but-not-installed, installed-from-missing-marketplace) plus two Agent Skill buckets (managed by zskills, on-disk-but-untracked).

## `install`

Flip a plugin's `enabledPlugins` entry on. Claude Code fetches bytes on next start (or run `/plugin install <name>` inside Claude Code to materialize them immediately).

```
zskills install <name>...
```

Accepts multiple names. Resolves unqualified names against your registered marketplaces; errors on ambiguity.

## `remove` / `purge`

`remove` is apt-style: disable in `enabledPlugins` and drop the inventory entry, but leave bytes on disk so re-enabling is instant. `purge` does the same plus deletes the bytes from `~/.claude/plugins/cache/`.

```
zskills remove <name>...
zskills purge  <name>...
```

## `enable` / `disable`

Flip a plugin's `enabledPlugins` flag without (un)installing.

```
zskills enable  <name>...
zskills disable <name>...
```

`enable` is a no-op if the plugin isn't installed (Claude Code will install it on next start). `disable` keeps bytes and inventory; use `purge` to wipe completely.

## `sync` (headline command)

Apply a declarative `skills.toml` manifest. Diffs intent against current state, then atomically writes the necessary settings.json and inventory changes.

```
zskills sync [--file <path>] [--dry-run]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--file <path>` | auto | Path to `skills.toml`. Default: `./skills.toml` → `$XDG_CONFIG_HOME/zskills/skills.toml` → `~/.config/zskills/skills.toml` |
| `--dry-run` | off | Print the plan; do not write |

What sync does:
1. For each `[[skills]]` entry: resolve `name@marketplace`, write to `enabledPlugins`. Entries currently enabled but not in the manifest get flipped off.
2. For each `[[agent_skills]]` entry: if `source` is present, clone/pull the source repo into the cache, copy `skills/<name>/` to `~/.claude/skills/`, record in inventory. If `source` is absent (local-only), register the existing on-disk skill in inventory without fetching anything.

Sync is idempotent. Run it on every fresh machine to reproduce your global state from a single file.

## `update`

Refresh every registered marketplace's git cache (or just the named one).

```
zskills update [<marketplace-name>]
```

Runs `git pull --ff-only` against each marketplace clone. Claude Code reads the cache on next start to detect new versions.

## `doctor`

Reconcile disk ↔ inventory ↔ settings. Reports drift in three categories:

1. Plugins enabled in `enabledPlugins` but not present in `installed_plugins.json` (broken references — Claude Code will fetch on next start).
2. Plugins in inventory whose marketplace tap is no longer registered.
3. Agent skills tracked in inventory but missing from `~/.claude/skills/` on disk.

```
zskills doctor [--fix]
```

With `--fix`, dangling references are removed (settings.json entries pointing nowhere, orphaned inventory entries). `--fix` never deletes installed bytes — that's what `purge` is for.

## `scan`

Walk a directory tree looking for project-scope skills.

```
zskills scan [<path>] [--depth N] [--json]
```

| Flag | Default | Description |
|------|---------|-------------|
| `<path>` | `.` | Tree to walk |
| `--depth N` | 6 | Maximum directory recursion depth |
| `--json` | off | Machine-readable output |

Detects two patterns:
- `.claude/settings.json` or `.claude/settings.local.json` with `enabledPlugins` / `extraKnownMarketplaces` (project-scope plugin enables)
- `.claude/skills/<name>/SKILL.md` (project-scope Agent Skills)

The default depth of 6 catches both patterns from a `~/Desktop/code`-style parent directory (Agent Skills are at depth 5 from the project root).

## `migrate`

Promote ONE project's enabled plugins and project-scope Agent Skills to user scope.

```
zskills migrate <project> [--remove-from-project] [--dry-run]
```

Reads `<project>/.claude/settings.json` (or `settings.local.json`) and `<project>/.claude/skills/<name>/`. Writes promoted plugin enables into `~/.claude/settings.json`'s `enabledPlugins` and copies Agent Skill directories into `~/.claude/skills/`.

`--remove-from-project` clears `enabledPlugins`, `extraKnownMarketplaces`, and `.claude/skills/` from the project after a successful promote.

## `migrate-skill`

Promote ONE Agent Skill that appears in many projects across the tree, in a single operation.

```
zskills migrate-skill <name> [--root <dir>] [--source <ref>]
                              [--remove-from-all] [--dry-run]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--root <dir>` | `.` | Tree to search |
| `--source <ref>` | none (local-only) | If set, install from upstream (`owner/repo` or git URL) instead of copying canonical |
| `--remove-from-all` | off | Delete the skill's `.claude/skills/<name>/` from every matched project |
| `--dry-run` | off | Print the plan; do not write |

For each matched project, the skill's directory is hashed and compared. If content diverges, the first project (alphabetical) wins as canonical and a warning is printed showing which projects have which hash. The promoted skill gets a `[[agent_skills]]` entry appended to your `skills.toml` so the migration is reproducible.

## `migrate-all`

Interactive sweep across a tree.

```
zskills migrate-all <dir> [--threshold N] [--yes] [--dry-run]
```

| Flag | Default | Description |
|------|---------|-------------|
| `<dir>` | required | Tree to walk |
| `--threshold N` | 2 | Only consider skills appearing in ≥N projects |
| `--yes`, `-y` | off | Skip prompts; accept defaults (no source, keep project copies) |
| `--dry-run` | off | Print planned action per skill; do not write |

For each duplicated skill above the threshold, prompts:
1. "Promote '<name>' to user scope? [Y/n]"
2. "Upstream source [owner/repo, URL, or blank for local-only]:"
3. "Remove project copies from N project(s)? [y/N]"

Calls `migrate-skill` under the hood for each accepted prompt.

## `marketplace`

Tap management — register, list, refresh, and remove Claude Code marketplaces.

```
zskills marketplace add <owner/repo | git-url>
zskills marketplace remove <name>
zskills marketplace list [--json]
zskills marketplace update [<name>]
```

`add` clones the marketplace repo into `~/.claude/plugins/marketplaces/<name>/` and writes both `known_marketplaces.json` and `settings.json`'s `extraKnownMarketplaces`. Mirrors what `/plugin marketplace add` does inside Claude Code.
