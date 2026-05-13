# Troubleshooting

## "no skills.toml found"

```
Error: no skills.toml found (looked in ./ and ~/.config/zskills/)
```

zskills looks for the manifest in this order:
1. `$PWD/skills.toml` (if you're inside a project that vendors a manifest)
2. `$XDG_CONFIG_HOME/zskills/skills.toml`
3. `~/.config/zskills/skills.toml`
4. The platform default from `dirs::config_dir()` (`~/Library/Application Support/zskills/skills.toml` on macOS)

Fix: create the file at `~/.config/zskills/skills.toml`, or pass `--file <path>` explicitly.

## "skill X is ambiguous — qualify with @marketplace"

Two registered marketplaces both expose a skill with the same name. zskills can't pick one. Qualify it:

```bash
zskills install firecrawl@zot24-skills        # instead of just firecrawl
```

## "enabled but NOT installed (broken)"

`doctor` is flagging an `enabledPlugins` entry that has no corresponding inventory record. Three legitimate causes:

1. **You just ran `sync` and haven't restarted Claude Code yet.** This is the normal post-sync state. Restart Claude Code; it'll fetch the bytes on startup. Doctor will go clean.
2. **The plugin was removed from its marketplace upstream.** Pick a replacement or `zskills disable <name>` to silence the warning.
3. **The marketplace tap was unregistered (`marketplace remove`).** `zskills doctor --fix` will drop the orphan reference from `enabledPlugins`.

## Sync deleted agent skills I didn't expect to lose

A v0.5 incident: running `zskills sync` inside a repo that ships its own `skills.toml` (like the zot24/skills marketplace) destructively re-applied *that* manifest against your user-scope state. v0.5.1+ defaults prevent this:

1. **`./skills.toml` no longer auto-loads.** Sync without `--file` uses only `~/.config/zskills/skills.toml`. If `./skills.toml` exists, sync prints a yellow warning pointing at it.
2. **`sync` never deletes bytes by default.** Removal of agent skills no longer in the manifest requires `--prune`. Without it, they're reported as `skip` and left intact.

If you lost a skill before these defaults, check whether the skill was committed in any project under your tree:

```bash
find ~/Desktop/code -path '*/.claude/skills/<name>/SKILL.md' 2>/dev/null
# Or via git history (any project that had it)
for p in ~/Desktop/code/*; do
  [ -d "$p/.git" ] || continue
  sha=$(git -C "$p" log --all --oneline -- ".claude/skills/<name>/SKILL.md" | head -1 | awk '{print $1}')
  [ -n "$sha" ] && echo "$p: $sha"
done
```

Then restore via `git checkout <sha> -- .claude/skills/<name>/` in the relevant project and `cp -R` to `~/.claude/skills/`.

## Sync wants to disable plugins I want to keep

If your `skills.toml` doesn't list a plugin, `sync` will flip it off because the manifest is *declarative* — it represents your complete intent. Two ways to handle:

**A)** Add the plugin to your manifest so sync stops touching it:

```toml
[[skills]]
name = "rust-analyzer-lsp"
marketplace = "claude-plugins-official"
```

**B)** Edit `~/.claude/settings.json` to remove the unwanted `enabledPlugins` entry, so it no longer needs an `enable=false` flip on every sync.

(A `sync --no-prune` flag is on the roadmap for users who prefer additive-only behavior; for now, declare everything you want.)

## `migrate-skill` says "content differs across projects"

Different projects have edited their copy of the same-named skill, so the bytes are no longer identical. zskills hashes each project's `SKILL.md` tree and groups by hash:

```
! content differs across projects — using the first as canonical:
  [4e483861]  7 project(s)
    /path/to/project-a
    /path/to/project-b
    ...
  [5f9d37fb]  1 project(s)
    /path/to/project-z
```

The first project (alphabetical) wins as canonical. If you want a *different* project's version to win, either:

- Re-run `migrate-skill` from inside that project first, OR
- Manually copy the desired version to `~/.claude/skills/<name>/` before `migrate-skill` (which will detect it as "already at user scope" and overwrite with canonical only if you proceed — so cancel first).

A `--canonical <project-path>` flag is reasonable for v0.4 if this becomes common pain.

## npm agent skill says "no new skills discovered"

Some npm packages place their skill files via a separate setup CLI (e.g., `npx <pkg> install`), not via npm's own postinstall hook. If `npm install -g <pkg>` alone doesn't write to `~/.claude/skills/`, the diff-before-after returns empty and zskills sees nothing to claim.

Two fixes:

1. Add a `claims` glob so zskills retroactively claims pre-existing directories that match:
   ```toml
   [[agent_skills]]
   npm = "get-shit-done-cc"
   claims = ["gsd-*"]
   ```
2. Set `install_cmd` to whatever the package's actual installer is:
   ```toml
   [[agent_skills]]
   npm = "some-tool"
   install_cmd = "npx some-tool install"
   ```

If you're not sure where a package writes its skills, run it once manually, then check `~/.claude/skills/` and pick a `claims` pattern that covers them.

## `sync` clones repeatedly / is slow on Agent Skills

The first sync clones every `[[agent_skills]] source` repo into `~/.cache/zskills/agent-skills/`. Subsequent syncs do `git pull --ff-only` against the cache — fast. If you're seeing repeated full clones, check that the cache directory exists and is writable:

```bash
ls -la ~/.cache/zskills/agent-skills/
```

You can wipe and rebuild the cache safely; it's reproducible:

```bash
rm -rf ~/.cache/zskills
zskills sync
```

## Plugin bytes seem stale / not reflecting upstream

Marketplace caches need an explicit refresh:

```bash
zskills marketplace update              # all marketplaces
zskills marketplace update zot24-skills # one
```

Restart Claude Code afterward so it picks up the new versions.

## Manifest entries vanished after I edited skills.toml manually

zskills writes via `toml_edit` and never deletes user content. If entries are gone, check:

- Did you save the file?
- Are you editing the right one? `zskills sync --dry-run` prints the manifest path at the top.
- Did `git checkout` or another tool overwrite it?

## "agent skill in inventory, missing on disk"

You deleted `~/.claude/skills/<name>/` manually. Two ways to recover:

```bash
# Re-fetch from upstream (if there's a source in the manifest)
zskills sync

# Or just drop the inventory entry
zskills doctor --fix
```

## "unrecognized marketplace source: skills.sh"

You ran `zskills marketplace add skills.sh` against a default build. The skills.sh driver is gated behind the `skills-sh` cargo feature and not compiled into vanilla binaries. Reinstall with the feature on:

```bash
cargo install --git https://github.com/zot24/zskills --features skills-sh --force
```

Then set `ZSKILLS_SKILLS_SH_API_KEY` (get one from [skills.sh/account](https://www.skills.sh/account)) and retry the `marketplace add`. Without the env var, `search` will skip skills.sh with a one-line hint and `install` will not fall through to it.

## "skills.sh rejected the API key in ZSKILLS_SKILLS_SH_API_KEY (HTTP 401)"

The key is wrong, revoked, or expired. Generate a fresh one at [skills.sh/account](https://www.skills.sh/account), update your shell rc, and re-source. The whole skills.sh API is gated — there's no unauthenticated fallback today.

## Cargo install fails with "edition2024 not stabilized"

You're on Rust < 1.85. zskills requires Rust 1.85 or newer (transitive `idna_adapter` dep). Update:

```bash
rustup update stable
cargo install --git https://github.com/zot24/zskills --force
```

## How do I uninstall zskills entirely?

```bash
cargo uninstall zskills
rm -rf ~/.cache/zskills
# Manifest stays — it's just a config file. Delete if you don't want it:
rm ~/.config/zskills/skills.toml

# Agent skill inventory stays too — Claude Code itself doesn't read it, but if
# you reinstall zskills later it'll resume from this state. Delete if you want
# a clean slate:
rm ~/.claude/skills/.zskills.json
```

Plugins and Agent Skills installed via zskills remain in `~/.claude/` — they're managed by Claude Code's own machinery, not zskills. Uninstall those via Claude Code's `/plugin uninstall` or by deleting the relevant directories.
