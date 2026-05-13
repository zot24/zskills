# Use cases

Pragmatic recipes for common workflows. Each section assumes `zskills` is installed and on `PATH`.

## 1. Bootstrap a fresh machine

```bash
cargo install --git https://github.com/zot24/zskills

# Copy your manifest from the old machine (or your dotfiles repo)
mkdir -p ~/.config/zskills
scp old-machine:~/.config/zskills/skills.toml ~/.config/zskills/

# Add the marketplaces named in the manifest
zskills marketplace add zot24/skills
zskills marketplace add cloudflare/skills
# (claude-plugins-official is registered automatically by Claude Code)

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

```toml
[[agent_skills]]
source = "jakubkrehel/make-interfaces-feel-better"
```

```bash
zskills sync
```

`sync` clones (or pulls) `github.com/jakubkrehel/make-interfaces-feel-better.git` into `$XDG_CACHE_HOME/zskills/agent-skills/`, then copies every directory under `skills/<name>/SKILL.md` into `~/.claude/skills/<name>/`. To pin just one skill out of a multi-skill repo, add `name = "specific-skill"`.

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
