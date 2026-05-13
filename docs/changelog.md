# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases from this point forward are managed by [release-please](https://github.com/googleapis/release-please) based on [Conventional Commits](https://www.conventionalcommits.org/).

## [0.5.0](https://github.com/zot24/zskills/compare/v0.4.0...v0.5.0) (2026-05-13)


### Features

* tarball update for non-git marketplaces ([db6e370](https://github.com/zot24/zskills/commit/db6e370cf2a511f0fa1318d8f61a7b2502dcbe83))
* v0.5 — upgrade command, npm sources, grouped list ([fb37468](https://github.com/zot24/zskills/commit/fb37468a68a03b43cbc13c1f9e574a10d8ef9273))


### Bug Fixes

* claims field + quiet git output + skip non-git marketplaces ([b424711](https://github.com/zot24/zskills/commit/b42471127db361b1ac210a2edae073f49c2cece7))

## [0.4.0](https://github.com/zot24/zskills/compare/v0.3.0...v0.4.0) (2026-05-12)


### Features

* initial v0.1 — package manager for Claude Code skills ([c03fcea](https://github.com/zot24/zskills/commit/c03fceaac89c10dc4fd8c7ca2b2c1eb50f5190b2))
* v0.2 — Agent Skills support (~/.claude/skills/) ([fcd7773](https://github.com/zot24/zskills/commit/fcd7773901de7ddeea4006f07ac56ad41ecc3b3b))
* v0.3 — migrate-skill, migrate-all, optional source ([d4144d9](https://github.com/zot24/zskills/commit/d4144d9b7931f955b1586be43a685a62c77abae5))


### Bug Fixes

* **manifest:** use XDG ~/.config across platforms, not platform default ([25e9b10](https://github.com/zot24/zskills/commit/25e9b10b94389e3fa0b14d5c5304f6197e2f1621))


### Documentation

* release-please + CHANGELOG + docs/ folder ([bdb002c](https://github.com/zot24/zskills/commit/bdb002c500dc9c1e04dc60b9cc1d5ba2e40fba33))

## [0.3.0] - 2026-05-12

### Features

- **migrate-skill**: promote ONE agent skill across every project under a tree. Hashes each project's copy to detect content divergence, picks the first as canonical, copies to user scope, optionally removes from all projects, appends a `[[agent_skills]]` entry to the manifest.
- **migrate-all**: interactive sweep over a tree. Groups by skill name, sorts by occurrence count, prompts per skill (promote? source? remove from projects?). `--threshold N` filters; `-y/--yes` accepts defaults.
- **Optional source** on `[[agent_skills]]` entries. A `name`-only entry declares a local-only skill: tracked in inventory but not refreshed from a remote by `sync`.
- **Manifest writes preserve formatting**: append uses `toml_edit::DocumentMut` so existing comments/structure in `skills.toml` survive round-trip.

### Internal

- Added `dialoguer` for interactive prompts.
- 13/13 integration tests passing, including new coverage for `migrate-skill`.

## [0.2.0] - 2026-05-12

### Features

- **Agent Skills support** (raw `SKILL.md` format under `~/.claude/skills/`). New `[[agent_skills]]` manifest section with `source` (owner/repo or git URL) and optional `name`.
- Source repos cached at `$XDG_CACHE_HOME/zskills/agent-skills/<owner>-<repo>/`.
- Own inventory at `~/.claude/skills/.zskills.json` (since Claude Code's `installed_plugins.json` doesn't cover Agent Skills).
- **`sync`** applies both `[[skills]]` and `[[agent_skills]]` in a single pass.
- **`list`** shows plugins AND agent skills; flags untracked agent skills.
- **`doctor`** detects orphans across all three states (settings, inventory, disk).
- **`scan`** walks `.claude/skills/<name>/SKILL.md` directories at project scope (default depth bumped 4 → 6).
- **`migrate`** also promotes `.claude/skills/` directories to user scope.

## [0.1.0] - 2026-05-12

Initial release — package manager for Claude Code plugins.

### Features

- **Commands**: `list`, `install`, `remove`, `purge`, `enable`, `disable`, `sync`, `update`, `doctor`, `scan`, `migrate`, `marketplace add|remove|list|update`.
- **Atomic JSON round-trip** preserves all unknown fields in `~/.claude/settings.json` (hooks, permissions, env, etc.).
- **Multi-marketplace** support with `name@marketplace` qualification matching Claude Code's syntax.
- **Declarative `skills.toml`** manifest auto-discovered from CWD or `~/.config/zskills/`.
- **Scan + migrate** for promoting project-scope skills to user scope.
- Git shelled out (no `libgit2` bundling); rustls TLS; single static binary.
- 8 integration tests using `assert_cmd` + `tempfile`-isolated `CLAUDE_HOME`.

[0.3.0]: https://github.com/zot24/zskills/releases/tag/v0.3.0
[0.2.0]: https://github.com/zot24/zskills/releases/tag/v0.2.0
[0.1.0]: https://github.com/zot24/zskills/releases/tag/v0.1.0
