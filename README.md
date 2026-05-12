# zskills

A package manager for Claude Code skills. Declarative install, multi-marketplace, written in Rust.

Think `brew bundle` for Claude Code: a `skills.toml` declares intent, your `~/.claude/settings.json` and `installed_plugins.json` get reconciled atomically. Works with any marketplace tap (`zot24-skills`, `claude-plugins-official`, `cloudflare`, custom).

## Install

```bash
cargo install --git https://github.com/zot24/zskills
```

Requires `git` on `$PATH`.

## Why

Existing options are JavaScript shims that pay Node cold-start per skill (`bunx skills add` loops are slow), don't preserve unknown fields when editing `settings.json`, and have no notion of a lockfile or declarative manifest. `zskills` is a single static binary that wraps Claude Code's existing plugin substrate, atomically.

## Commands

```
zskills list                  # what's installed; with status
zskills install <name>        # add to enabledPlugins
zskills remove  <name>        # disable + drop inventory entry (keep bytes — apt style)
zskills purge   <name>        # also delete bytes
zskills enable  <name>        # flip on without (un)installing
zskills disable <name>
zskills sync [--file f.toml]  # apply declarative manifest (headline command)
zskills update [<name>...]    # refresh marketplace caches
zskills doctor [--fix]        # reconcile disk ↔ inventory ↔ settings
zskills scan [path]           # find project-scope skills across a tree
zskills migrate <project>     # promote project-scope to user scope
zskills marketplace add|remove|list|update
```

`<name>` accepts unqualified (`servarr`) when unambiguous, or `name@marketplace` (`servarr@zot24-skills`) to disambiguate.

## Declarative manifest (skills.toml)

```toml
[[skills]]
name = "umbrel-app"
marketplace = "zot24-skills"

[[skills]]
name = "github"
marketplace = "claude-plugins-official"

[[skills]]
name = "cloudflare"
marketplace = "cloudflare"
```

```bash
zskills sync               # apply: enables anything in the manifest, disables anything not
zskills sync --dry-run     # preview
```

`zskills sync` is idempotent. Run it anywhere — same machine, new machine — and the result matches the manifest. Run it on every fresh checkout.

## Scanning project-scope skills

If you've enabled skills inside `.claude/settings.json` across many repos, `zskills` can find them all:

```bash
zskills scan ~/Desktop/code
```

Then promote a project's skills to user scope (so they're available everywhere):

```bash
zskills migrate ~/Desktop/code/some-project              # add to user; leave project alone
zskills migrate ~/Desktop/code/some-project --remove-from-project
zskills migrate ~/Desktop/code/some-project --dry-run
```

## Design

- **`~/.claude/settings.json` is authoritative for what runs.** `installed_plugins.json` is the inventory. Three states matter: on-disk-and-enabled, on-disk-and-disabled, on-disk-and-orphaned. `doctor` reconciles them.
- **Atomic JSON writes.** Tempfile + rename. Preserves every unknown field — `hooks`, `permissions`, `env`, anything Claude Code adds — round-trip safe.
- **`name@marketplace` qualification.** Same syntax Claude Code uses. No invention.
- **Git is shelled out.** Reuses your credential helpers; no `libgit2` to bundle.
- **No async network unless needed.** Marketplace caches live on disk; `git pull` does the work.

## Status

v0.1 — covers daily install/enable/disable/scan/migrate flows. Lockfile semantics, `info`/`search`, and full version pinning land in v0.2.

## Sister project

The [`zot24/skills`](https://github.com/zot24/skills) marketplace is what this tool was originally built to manage.

## License

MIT
