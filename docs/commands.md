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
zskills list [--json] [-v] [--paths]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | off | Emit a machine-readable JSON document for scripting |
| `-v`, `--verbose` | off | Expand grouped agent skills (show every skill name in each source group) |
| `--paths` | off | Show the on-disk location of each entry: plugin install path under `~/.claude/plugins/cache/...`, agent skill directory under `~/.claude/skills/...`, or the settings file an MCP server is declared in |

The non-JSON output groups results into four plugin buckets (active, installed-but-disabled, enabled-but-not-installed, installed-from-missing-marketplace) plus two Agent Skill buckets (managed by zskills, on-disk-but-untracked), plus a final **MCP Servers** section that aggregates every server visible to Claude Code from all of:

- `~/.claude.json` and `~/.claude/settings.json` — user scope
- `<cwd>/.mcp.json` and `<cwd>/.claude/settings.json` — project scope
- `<cwd>/.claude.local/settings.json` — local (gitignored) scope
- `/Library/Application Support/ClaudeCode/managed-settings.json` (macOS) / `/etc/claude-code/managed-settings.json` (Linux) — managed (org-deployed) scope
- An attribution column reads each enabled plugin's `plugin.json` / sibling `.mcp.json` and marks entries with `★ plugin:<name>` when the server is bundled by a plugin.

Servers are grouped by scope (managed → local → project → user) and only the *keys* of `env` / `headers` are surfaced (no values), so the output never leaks secrets even when `${VAR}` refs aren't used. Set `ZSKILLS_MANAGED_SETTINGS=<path>` to override the managed-settings probe (useful in CI).

## `install`

Three accepted spec shapes:

```
zskills install <name>                       # plugin from a registered marketplace
zskills install <name>@<marketplace>         # qualified plugin
zskills install <owner>/<repo>               # NEW: Agent Skill(s) directly from a git repo
zskills install <git-url>                    # NEW: same, for arbitrary git URLs (https://, git@, file://)
zskills install -i                           # interactive picker over marketplace plugins
```

**Plugin path (`<name>` / `<name>@<marketplace>`).** Resolves against registered marketplaces, flips `enabledPlugins` in `~/.claude/settings.json`. Claude Code materializes bytes on next launch.

**Repo path (`<owner>/<repo>` or git URL).** Clones the repo via `git clone --depth 1` into `~/.cache/zskills/agent-skills/<owner>-<repo>/`, surveys the tree, and:
- **Agent Skills** (any directory under `skills/<name>/SKILL.md`) — installed to `~/.claude/skills/<name>/`. Inventory tagged with the source.
- **Marketplace** (`.claude-plugin/marketplace.json` at repo root) — prints a redirect: `use zskills marketplace add <owner/repo>` and installs nothing from this repo.
- **MCP servers** (`.mcp.json` or `mcpServers` in `plugin.json`) — surfaced as a hint; not auto-installed (use `[[mcps]]` in `skills.toml` + `zskills sync`).

| Flag | Default | Description |
|------|---------|-------------|
| `-i`, `--interactive` | off | For plugin specs: without any `<name>`, browse all marketplace plugins with a fuzzy picker. For repo specs: always opens a multi-select picker over the Agent Skills found in the repo. |
| `--all` | off | For repo specs only: when the repo contains more than 5 Agent Skills, confirm "yes, install every one." Without `--all`, large collections abort with a sample summary so they don't silently flood `~/.claude/skills/`. Ignored for repos with ≤5 skills (those install everything by default). |

**Repo-path count behavior**:

| Skills in repo | Default (no flags) | With `-i` | With `--all` |
|---|---|---|---|
| 0 | error | error | error |
| 1 | install it | install it | install it |
| 2–5 | install all | picker | install all |
| > 5 | **abort + summary + next-step hint** | picker | install all |

## `remove` / `purge`

`remove` is apt-style: disable in `enabledPlugins` and drop the inventory entry, but leave bytes on disk so re-enabling is instant. `purge` does the same plus deletes the bytes from `~/.claude/plugins/cache/`.

```
zskills remove <name>...
zskills remove -i
zskills purge  <name>...
```

| Flag | Default | Description |
|------|---------|-------------|
| `-i`, `--interactive` | off | (`remove` only) When passed without any `<name>`, browse enabled plugins with a multi-select picker and remove the selection. |

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
zskills sync [--file <path>] [--dry-run] [--prune]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--file <path>` | `$XDG_CONFIG_HOME/zskills/skills.toml` (then `~/.config/zskills/skills.toml`) | Path to `skills.toml`. **`./skills.toml` is NOT auto-loaded** — pass `--file ./skills.toml` to use a project-local manifest. (This caused destructive surprises in v0.5; the v0.5.1 default is safer.) |
| `--dry-run` | off | Print the plan; do not write |
| `--prune` | off | Allow destructive removals. Without `--prune`, agent skills present on disk but absent from the manifest are reported as `skip` and left untouched. With `--prune`, their bytes are deleted from `~/.claude/skills/`. |

What sync does:
1. For each `[[skills]]` entry: resolve `name@marketplace`, write to `enabledPlugins`. Entries currently enabled but not in the manifest get flipped off.
2. For each `[[agent_skills]]` entry: if `source` is present, clone/pull and copy `skills/<name>/` to `~/.claude/skills/`. If `npm` is present, run `npm install -g <pkg>` (or `install_cmd`), then claim all matching `claims` globs. If neither is present (just `name`), register the existing on-disk skill in inventory without fetching anything.
3. Agent skills tracked in inventory but missing from the manifest are reported. With `--prune` they're deleted; without, they're skipped.

Sync is idempotent. Run it on every fresh machine to reproduce your global state from a single file.

If a `./skills.toml` exists in CWD when you run `sync` without `--file`, zskills prints a yellow warning telling you it's being ignored — pass `--file ./skills.toml` if that's actually what you wanted.

## `upgrade`

The one command for refreshing everything zskills manages — marketplaces, git agent skills, and npm agent skills.

```
zskills upgrade [<name>...]
```

| Source kind | What `upgrade` does |
|---|---|
| Plugins (marketplace-based) | For each registered marketplace tap, `git pull --ff-only` if it's a git working tree; otherwise fetch the GitHub archive tarball from the source recorded in `known_marketplaces.json` and atomically swap the tree. Claude Code picks up new plugin versions on next start. |
| Git agent skills (`source = "owner/repo"`) | `git pull` the cached source clone + re-copy bytes |
| npm agent skills (`npm = "pkg"`) | Run `npm install -g <pkg>` (or `install_cmd`), then re-apply the `claims` glob to retag inventory |

Pass specific names to upgrade just those; empty = upgrade everything. The `name` filter matches against the manifest's `npm`, `source`, or `name` fields.

## MCP servers manifest schema

Add `[[mcps]]` tables to `skills.toml` to declaratively manage MCP servers. `sync` writes them to the appropriate runtime config file based on `scope`:

```toml
# Stdio: a process the runtime spawns directly.
[[mcps]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
scope = "user"  # default; also "project" or "local"

# HTTP: a remote server reachable over HTTP.
[[mcps]]
name = "linear"
url = "https://mcp.linear.app/mcp"
transport = "http"  # optional; inferred from `url`
scope = "user"

# Stdio + mcp-remote proxy pattern (real-world): args reference ${VAR} too.
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

| Field | Purpose |
|---|---|
| `name` (required) | MCP server name. Becomes the key under `mcpServers`. |
| `command` | Stdio: the executable to run. Required for stdio transports. |
| `args` | Stdio: list of args. `${VAR}` refs are allowed and recommended for credentials. |
| `env` | Stdio: env-var map. Values should use `${VAR}` refs. |
| `url` | HTTP/SSE: server URL. |
| `headers` | HTTP/SSE: header map. Same `${VAR}` policy as `env`. |
| `transport` | `"stdio"` \| `"http"` \| `"sse"`. Optional; inferred from `command` or `url`. SSE is the deprecated spec form — prefer http. |
| `scope` | `"user"` (default) \| `"project"` \| `"local"`. `"managed"` is not accepted (read-only). |

**Write targets per scope:**

| Scope | File | Notes |
|---|---|---|
| `user` | `~/.claude.json` | The file `claude mcp add --scope user` writes to. Wrapped JSON (`{"mcpServers": {...}}`). |
| `project` | `<cwd>/.mcp.json` | Spec-recommended team-shared file. Created wrapped; if it already exists with the legacy flat schema, the existing shape is preserved. |
| `local` | `<cwd>/.claude.local/settings.json` | Gitignored personal+creds file. Wrapped. |

**Sync semantics.**

- `sync` installs every MCP from the manifest and overwrites overlapping entries (manifest is source of truth).
- Without `--prune`, MCP entries currently in the runtime config but absent from the manifest are reported as `skip` — same safety pattern as `[[agent_skills]]`.
- With `--prune`, those entries are removed from their settings file.
- **Plugin-injected MCPs are never pruned.** If a server name matches one declared by an enabled plugin, it's owned by that plugin and zskills won't touch it.
- **Managed scope is never written.** Entries appear in `list`/`doctor` but `sync` ignores them.

**Secret handling.** Values in `env`/`headers` should be `${VAR}` references; the actual secret stays in your shell. Literal values are not blocked but they land in the JSON verbatim — which means they'd go straight into git-committed files at `project` scope. The recommendation is documentation, not enforcement.

## Agent skills manifest schema

The `[[agent_skills]]` table supports four orthogonal fields, used in combination:

```toml
# Git-sourced single skill
[[agent_skills]]
source = "jakubkrehel/make-interfaces-feel-better"

# Git-sourced multi-skill repo, install just one
[[agent_skills]]
source = "owner/multi-skill-repo"
name = "specific-skill"

# npm-distributed package
[[agent_skills]]
npm = "get-shit-done-cc"
claims = ["gsd-*"]              # glob patterns; every match in ~/.claude/skills/ is owned by this entry

# npm with custom installer command
[[agent_skills]]
npm = "some-tool"
install_cmd = "npx some-tool setup"

# Local-only (just tracked, never refreshed from a remote)
[[agent_skills]]
name = "my-internal-tool"
```

| Field | Purpose |
|---|---|
| `source` | Git source: `owner/repo` (GitHub) or full git URL. `sync`/`upgrade` clone/pull and copy. |
| `npm` | npm package name. `sync`/`upgrade` runs `npm install -g <pkg>`. |
| `install_cmd` | Custom installer command — overrides the default `npm install -g`. Used for packages with their own setup CLI. |
| `name` | Optional. For source entries, pick a single skill out of a multi-skill repo. For local-only entries, required — names the on-disk skill to track. |
| `claims` | Glob patterns (e.g., `["gsd-*"]`) matched against `~/.claude/skills/`. After install, every match is tagged with this entry's source. Used for npm packages whose installer touches pre-existing directories — so the diff-after-install discovers nothing, but `claims` retroactively claims ownership. |

## `update`

Refresh every registered marketplace's git cache (or just the named one).

```
zskills update [<marketplace-name>]
```

Runs `git pull --ff-only` against each marketplace clone. Claude Code reads the cache on next start to detect new versions.

## `doctor`

Reconcile disk ↔ inventory ↔ settings, and statically validate every configured MCP server. Reports drift in four categories:

1. Plugins enabled in `enabledPlugins` but not present in `installed_plugins.json` (broken references — Claude Code will fetch on next start).
2. Plugins in inventory whose marketplace tap is no longer registered.
3. Agent skills tracked in inventory but missing from `~/.claude/skills/` on disk.
4. **MCP server issues** — static checks against every server returned by `list`:
   - **stdio**: `command` must resolve on `$PATH` (uses `which`-style lookup).
   - **any transport**: every `${VAR}` reference in `env` (stdio) or `headers` (http/sse) must be set in the user's environment.
   - **sse transport**: flagged as deprecated; the spec recommends migrating to `http`.

```
zskills doctor [--fix]
```

With `--fix`, dangling plugin/inventory references are removed. **`--fix` is a no-op for MCP issues** — none of them are auto-fixable (we won't install a missing binary or invent an env var), so doctor's job there is purely to surface what's broken. `--fix` never deletes installed bytes — that's what `purge` is for.

Doctor never spawns or talks to an MCP server. Running-state diagnosis (whether a server is actually connected, last error, latency) is Claude Code's job — replicating it here would risk divergent diagnoses. If you need that, run the server directly or check Claude Code's own logs.

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
zskills marketplace add-recommended
zskills marketplace remove <name>
zskills marketplace list [--json]
zskills marketplace update [<name>]
```

`add` clones the marketplace repo into `~/.claude/plugins/marketplaces/<name>/` and writes both `known_marketplaces.json` and `settings.json`'s `extraKnownMarketplaces`. Mirrors what `/plugin marketplace add` does inside Claude Code.

`add-recommended` seeds the trusted defaults (currently just `anthropics/claude-plugins-official`). Idempotent — safe to re-run; existing marketplaces are left as-is.

`add skills.sh` is recognized only when zskills was built with `--features skills-sh`. It registers skills.sh as a `remote-index` source type (no git clone) and is dispatched by `search` and `install` via the HTTP API. See [Optional features](#optional-features) below.

## `search`

Keyword search across every registered marketplace. Substring-matches `<query>` against `name + description` in each marketplace's cached `marketplace.json`. Purely local — no network calls.

```
zskills search <query> [--limit <n>] [--json] [-i]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--limit <n>` | 25 | Maximum results per marketplace |
| `--json` | off | Emit results as a JSON array for scripting |
| `-i`, `--interactive` | off | After printing results, open a picker; selecting one installs it. |

With the `skills-sh` cargo feature compiled in AND `ZSKILLS_SKILLS_SH_API_KEY` set, `search` also federates to the skills.sh remote index and tags those results `[skill]`. Without the env var, the registered remote-index is skipped with a one-line hint and local search continues uninterrupted.

## Optional features

`zskills` ships vanilla by default. Optional capabilities are gated behind cargo features so they aren't even compiled into the binary unless you ask for them.

| Feature | What it adds | How to enable |
|---|---|---|
| `skills-sh` | Federated `search` + `install` against the [skills.sh](https://www.skills.sh) remote index. Registers a new `remote-index` source type. Runtime activation requires `ZSKILLS_SKILLS_SH_API_KEY`. | `cargo install --git https://github.com/zot24/zskills --features skills-sh` |

Without the feature, `zskills marketplace add skills.sh` returns *"unrecognized marketplace source"* — there's no dormant code, no env-var detection, nothing. The compiled binary is byte-identical to a feature-free build except for what you explicitly asked for.

### fzf integration (auto-detected)

`install -i`, `search -i`, and `remove -i` automatically use `fzf` when it's on `$PATH`, which gets you full fuzzy filtering and the familiar fzf keybindings. When fzf is not installed, zskills falls back to the built-in dialoguer picker (`FuzzySelect` for single-select, `MultiSelect` for multi). Set `ZSKILLS_NO_FZF=1` to force the dialoguer path even when fzf is installed.

### `install` fallback (skills.sh feature only)

When `skills-sh` is built in and a remote index is registered with a valid key, `install <name>` will fall through to skills.sh if the spec doesn't resolve in any local plugin marketplace. It performs an exact-slug match against the skills.sh search API and, on hit, routes through the existing Agent Skill install path (`git clone source/repo` → drop `SKILL.md` into `~/.claude/skills/<name>/`). No `enabledPlugins` flip — agent skills don't use that gate.
