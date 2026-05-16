# Use cases

Pragmatic recipes for common workflows. Each section assumes `zskills` is installed and on `PATH`.

## 1. Bootstrap a fresh machine

```bash
cargo install --git https://github.com/zot24/zskills

# Copy your manifest from the old machine (or your dotfiles repo)
mkdir -p ~/.config/zskills
scp old-machine:~/.config/zskills/skills.toml ~/.config/zskills/

# Seed the trusted defaults, then add any additional marketplaces named in the manifest
zskills marketplace add-recommended       # anthropics/claude-plugins-official
zskills marketplace add zot24/skills
zskills marketplace add cloudflare/skills

# Apply
zskills sync
```

After `sync`, restart Claude Code so it fetches the bytes for the flagged plugins. `zskills doctor` should report clean.

## 2. Add a new skill globally

Two options. Use whichever matches how you think.

**Imperative** — flip on now, document later:

```bash
zskills install firecrawl@zot24-skills
```

**Declarative** — edit the manifest, run sync:

```toml
# ~/.config/zskills/skills.toml
[[skills]]
name = "firecrawl"
marketplace = "zot24-skills"
```

```bash
zskills sync
```

The declarative path is reproducible across machines; the imperative path is what you reach for in a one-off shell.

## 3. Mirror an Agent Skill from a GitHub repo

Two paths — pick whichever fits.

**Imperative**, one-shot:

```bash
zskills install jakubkrehel/make-interfaces-feel-better
```

zskills clones the repo, surveys it, and installs every `skills/<name>/SKILL.md` it finds. For repos with ≤5 skills, all install by default. For one skill, it installs silently. For multi-skill repos (e.g. a collection), see ["large collections"](#large-collections-from-a-repo) below.

The same command accepts full git URLs too:

```bash
zskills install https://github.com/jakubkrehel/make-interfaces-feel-better.git
zskills install git@github.com:jakubkrehel/make-interfaces-feel-better.git
```

**Declarative**, reproducible across machines:

```toml
[[agent_skills]]
source = "jakubkrehel/make-interfaces-feel-better"
```

```bash
zskills sync
```

`sync` clones (or pulls) the repo into `$XDG_CACHE_HOME/zskills/agent-skills/`, then copies every directory under `skills/<name>/SKILL.md` into `~/.claude/skills/<name>/`. To pin just one skill out of a multi-skill repo, add `name = "specific-skill"`.

### Large collections from a repo

When a repo exposes more than 5 skills (a curated collection, say), the imperative install path **won't silently flood `~/.claude/skills/`**:

```
$ zskills install owner/big-skill-collection
owner/big-skill-collection contains 47 Agent Skills — zskills won't install all of them by default.

Options:
  zskills install owner/big-skill-collection -i      interactive picker
  zskills install owner/big-skill-collection --all   install all 47 skills

Sample (5 of 47): skill-a, skill-b, skill-c, skill-d, skill-e, …
```

`-i` opens a multi-select picker (fzf when on `$PATH`, else dialoguer's `MultiSelect`); `--all` is the explicit-consent escape hatch.

### Marketplace repos

If the repo is actually a Claude Code marketplace (has `.claude-plugin/marketplace.json`), zskills redirects:

```
$ zskills install anthropics/claude-plugins-official
This repo is a plugin marketplace. To register and install plugins from it:
  zskills marketplace add anthropics/claude-plugins-official
  zskills install <plugin>@<marketplace>
```

That's the canonical path for plugins — they go through marketplace registration, not direct install from the marketplace's repo.

## 4. Centralize duplicate skills scattered across projects

You have the same `performance-tracking-skill` directory under `.claude/skills/` in 8 projects, and you want it at user scope so every project gets it for free.

Inspect first:

```bash
zskills scan ~/Desktop/code | grep -A1 'Skill → projects'
```

Promote one specifically (preview, then apply):

```bash
zskills migrate-skill performance-tracking-skill --root ~/Desktop/code --dry-run
zskills migrate-skill performance-tracking-skill --root ~/Desktop/code --remove-from-all
```

`migrate-skill` hashes each project's copy; if content has diverged it warns and picks the first as canonical so you can stop and inspect manually. The skill ends up at `~/.claude/skills/performance-tracking-skill/`, an entry is appended to your `skills.toml`, and (with `--remove-from-all`) every project's copy is deleted.

## 5. Interactive sweep across many projects

For a one-shot cleanup of dozens of duplicated skills:

```bash
zskills migrate-all ~/Desktop/code --threshold 3
```

Walks the tree, groups by skill name, only considers skills in ≥3 projects, then prompts per skill. For each accepted prompt it asks for an upstream source (or blank for local-only) and whether to clean project copies.

For batch-promotion without interactivity (accepts defaults: no source, keep project copies):

```bash
zskills migrate-all ~/Desktop/code --threshold 5 -y
```

## 6. Track a project-scope skill you can't move yet

Sometimes a skill is genuinely project-specific (e.g., the project's own ops runbook). You don't want it at user scope, but you do want it tracked. Currently the manifest is user-scope-only. Two reasonable patterns:

**Pattern A: Keep the project copy in version control.** Check `.claude/skills/<name>/` into the project's git repo. Don't add it to `~/.config/zskills/skills.toml`. The project carries its own skill; teammates get it on `git clone`.

**Pattern B: Put a project-scope `skills.toml` in the project root.** `zskills sync` (run from inside the project) auto-discovers `./skills.toml` before falling back to `~/.config/zskills/`. So a project can carry its own intent for what should be globally enabled when working on it.

## 7. Adopt a multi-skill npm package (e.g., `get-shit-done-cc`)

Some skill bundles ship as npm packages whose post-install hook writes many skill directories under `~/.claude/skills/`. zskills owns them via a `npm` + `claims` declaration:

```toml
[[agent_skills]]
npm = "get-shit-done-cc"
claims = ["gsd-*"]               # any name matching this glob is owned by this entry
```

After `zskills sync`, all matching `~/.claude/skills/gsd-*/` directories are tagged `source: "npm:get-shit-done-cc"` in inventory. `zskills list` groups them under one line:

```
✓ get-shit-done-cc (66 skills)  ← npm
    gsd-add-tests, gsd-ai-integration-phase, … [-v to list all 66]
```

`zskills upgrade` will run `npm update -g get-shit-done-cc` and re-claim, keeping the bundle current.

If the npm package needs a custom setup command (some packages have a separate CLI to actually place files), use `install_cmd`:

```toml
[[agent_skills]]
npm = "some-tool"
install_cmd = "npx some-tool install"
claims = ["sometool-*"]
```

## 8. One command, refresh everything

```bash
zskills upgrade
```

That single command:

- `git pull` (or tarball fetch) every marketplace tap, so Claude Code sees the latest plugin versions
- `git pull` every git-sourced agent skill and re-copy bytes
- `npm update -g` every npm-sourced agent skill (and re-claim via `claims` globs)

Pass names to limit scope: `zskills upgrade get-shit-done-cc zot24-skills`.

## 9. Diagnose drift

```bash
zskills doctor
```

If something's amiss:

- **"enabled but NOT installed (broken)"** — Claude Code knows about the plugin via `enabledPlugins`, but the bytes aren't on disk. Restart Claude Code (it'll install on startup), or run `/plugin install <name>@<mp>` inside Claude Code. If you don't want it anymore, `zskills doctor --fix` removes the flag.
- **"installed from a marketplace that's no longer registered"** — you `marketplace remove`-d a tap but plugins from it are still in the inventory. Run `zskills purge <name>` to clean.
- **"agent skill tracked in inventory but missing on disk"** — someone deleted `~/.claude/skills/<name>/` manually. `zskills doctor --fix` removes the stale inventory entry, or re-run `sync` to reinstall.

## 10. Reproduce someone else's setup

Ask them for their `skills.toml`. Drop it in `~/.config/zskills/skills.toml`. Register any marketplaces they use (`zskills marketplace add owner/repo`). Run `zskills sync`. Done.

## 11. Promote project skills + remove the project's `.claude/skills/`

```bash
zskills migrate ~/Desktop/code/some-project --remove-from-project
```

Moves both `enabledPlugins` entries from the project's `.claude/settings.json` (or `settings.local.json`) AND every directory under `.claude/skills/` into user scope, then clears the project copies. Useful when you've decided "these are clearly global tools — they shouldn't be vendored per-project."

## 12. Vendor a global skill INTO a project (rare; manual)

zskills doesn't push this direction yet (user-scope → project-scope). If you need a particular project to pin an exact version of a skill that diverges from the global one, copy `~/.claude/skills/<name>/` into the project's `.claude/skills/<name>/` and commit it. Claude Code resolves project scope before user scope, so the project's pinned copy wins.

## 13. See every MCP server configured on this machine

Claude Code can read MCP servers from up to six files (per scope). `zskills list` aggregates the lot and attributes plugin-injected entries:

```bash
zskills list                # everything; MCPs are the last section
zskills list --paths        # also show which file each entry was loaded from
zskills list --json | jq '.mcp_servers'
```

Output looks like:

```
MCP Servers
  user (3)
    github          stdio  npx -y @modelcontextprotocol/server-github  ★ plugin:github  (1 env)
    honcho          http   https://mcp.honcho.dev                                       (3 headers)
    linear-server   http   https://mcp.linear.app/mcp
  project (1)
    postgres        stdio  docker run --rm postgres-mcp                                 (2 envs)
```

Only `env` / `header` *keys* are surfaced — values are never read into memory, so the output is safe even if a secret got pasted in literally instead of as a `${VAR}` ref.

## 14. Declare MCP servers in the manifest

```toml
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

```bash
zskills sync                # writes both into ~/.claude.json atomically
```

Restart Claude Code (or use its `/mcp` prompt) so it picks up the new servers.

## 15. Centralize scattered MCP configs

You've added MCP servers ad-hoc with `claude mcp add` from different projects; now they're spread across `~/.claude.json`, project `.mcp.json` files, and one rogue `.claude.local/settings.json`. To consolidate at user scope:

1. **See what's where:**

   ```bash
   zskills list --paths
   ```

2. **Manually transcribe each into `~/.config/zskills/skills.toml`** as `[[mcps]]` entries. Use `${VAR}` refs for any credentials — never paste literal tokens.

3. **Sync with prune** to write at user scope AND delete the duplicates from other scopes:

   ```bash
   zskills sync --prune
   ```

`--prune` removes MCP entries currently in settings files but absent from the manifest. **Plugin-injected MCPs are never pruned** — zskills detects them by name match against every enabled plugin's `plugin.json` / sibling `.mcp.json` and leaves them alone. **Managed scope is never written** — IT-deployed entries are read-only.

A `dump-mcps` helper to skip the manual transcription step is on the roadmap (see [issue #14](https://github.com/zot24/zskills/issues/14)).

## 16. Validate MCPs before launching Claude Code

```bash
zskills doctor
```

Three static checks per MCP server, no process spawning:

- **stdio**: `command` must resolve on `$PATH` (e.g. `npx` is installed, the package would be reachable).
- **any transport**: every `${VAR}` referenced in `env` / `headers` / `args` must be defined in your shell environment.
- **sse transport**: flagged as deprecated; switch to `transport = "http"`.

`--fix` is a no-op for MCP findings — none of them are auto-fixable (zskills won't install a missing binary or invent an env var). The contribution is surfacing the problem so you know before Claude Code complains.

Doctor never spawns or talks to a server. Runtime health (connection, latency, last error) is Claude Code's job; replicating it here would risk divergent diagnoses.

## 17. Find a skill before installing it

You remember there's a Stripe integration somewhere but you don't know which marketplace ships it:

```bash
zskills search stripe                # substring-matches name + description across taps
zskills search stripe --limit 5      # tighter output
zskills search "data analytics"      # quoted multi-word queries work
zskills search stripe --json | jq    # JSON for scripting
```

Search reads each marketplace's cached `marketplace.json` — purely local, no network. If you also have the `skills-sh` cargo feature compiled in and `ZSKILLS_SKILLS_SH_API_KEY` set, results from the skills.sh remote index are tagged `[skill]` and merged in:

```bash
cargo install --git https://github.com/zot24/zskills --features skills-sh --force
export ZSKILLS_SKILLS_SH_API_KEY=sk_live_...
zskills marketplace add skills.sh
zskills search next-js                # now federates to skills.sh
```

Once you've found the name, `zskills install <name>` flips it on (or appends `[[skills]]` to `skills.toml` for the declarative path).
