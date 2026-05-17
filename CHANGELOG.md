# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases from this point forward are managed by [release-please](https://github.com/googleapis/release-please) based on [Conventional Commits](https://www.conventionalcommits.org/).

## [0.8.0](https://github.com/zot24/zskills/compare/v0.7.0...v0.8.0) (2026-05-17)


### Features

* install from repo + cross-client skill path + sync --adopt ([#21](https://github.com/zot24/zskills/issues/21)) ([64274b2](https://github.com/zot24/zskills/commit/64274b2ee8be7a3363d66e10f98adeb859616ef7))
* zskills install &lt;owner/repo&gt; — direct install from a git repo ([#18](https://github.com/zot24/zskills/issues/18)) ([2756dc0](https://github.com/zot24/zskills/commit/2756dc037aa815bab80fc62ca57555a70ed71067))


### Documentation

* ship Agent Skill + MCP-aware docs for v0.7.0 ([#16](https://github.com/zot24/zskills/issues/16)) ([66ad32f](https://github.com/zot24/zskills/commit/66ad32f478692d13d6dd2eb6f7842984d33aad2b))
* update for ~/.agents/skills/ path + sync --adopt + quieter npm ([#22](https://github.com/zot24/zskills/issues/22)) ([38a5c58](https://github.com/zot24/zskills/commit/38a5c584c7d7c8234057e91066c71e6499c66d54))

## [0.7.0](https://github.com/zot24/zskills/compare/v0.6.0...v0.7.0) (2026-05-16)


### Features

* [[mcps]] manifest support + sync reconciliation ([#13](https://github.com/zot24/zskills/issues/13)) ([882bfe5](https://github.com/zot24/zskills/commit/882bfe5a5ff5de42632a7dabfe5b375534e49871))
* add -i interactive mode to install, search, remove ([#8](https://github.com/zot24/zskills/issues/8)) ([33ae160](https://github.com/zot24/zskills/commit/33ae1601381b63d632725b16ac33d3accc3fe650))
* doctor statically validates MCP servers ([#11](https://github.com/zot24/zskills/issues/11)) ([8295670](https://github.com/zot24/zskills/commit/8295670341d34342b91e77e851da96f5f9a5d1c6))
* prefer fzf for interactive pickers, fall back to dialoguer ([#9](https://github.com/zot24/zskills/issues/9)) ([1e13340](https://github.com/zot24/zskills/commit/1e133407e22c532a294501e53eba7aef2c301a51))
* zskills list --paths shows on-disk location for each entry ([#15](https://github.com/zot24/zskills/issues/15)) ([d1dcc04](https://github.com/zot24/zskills/commit/d1dcc0458871bf32b973a238d2ab30913baef479))
* zskills list aggregates MCP servers across all scopes ([#10](https://github.com/zot24/zskills/issues/10)) ([b13ca45](https://github.com/zot24/zskills/commit/b13ca45fb49a04572a311c1aeecb577e3312f7c8))


### Documentation

* reposition as multi-runtime, not Claude-only ([#12](https://github.com/zot24/zskills/issues/12)) ([b8429ac](https://github.com/zot24/zskills/commit/b8429ac985fdc7a39ddc158d8e392a4b1300dcd2))
* **site:** add OG/Twitter card image for social previews ([#6](https://github.com/zot24/zskills/issues/6)) ([631a426](https://github.com/zot24/zskills/commit/631a426cfcff8306de759298a4bea22060316e2c))

## [0.6.0](https://github.com/zot24/zskills/compare/v0.5.0...v0.6.0) (2026-05-13)


### Features

* add search command and optional skills.sh driver ([#4](https://github.com/zot24/zskills/issues/4)) ([f65573c](https://github.com/zot24/zskills/commit/f65573cc69cfa0f35b309e89809d912a1db7d39e))


### Bug Fixes

* **list:** cleaner group header — bare name + arrow source kind ([ea68a63](https://github.com/zot24/zskills/commit/ea68a63bc10ca87e8d1d66aa95d8a074caf1727c))
* **sync:** honor npm/claims ownership; skip already-present source entries ([be3e187](https://github.com/zot24/zskills/commit/be3e1874658a0fa0e7280261e211cc85f854f56f))
* **sync:** prevent data loss via safer defaults ([b97c721](https://github.com/zot24/zskills/commit/b97c721911a26fcc7cedbfd5e93374ff0d0af6b9))


### Documentation

* add mdBook static site + GitHub Pages deploy ([4a12893](https://github.com/zot24/zskills/commit/4a128938d6336791c88be738678137d2f45ddb4d))
* cover v0.5/v0.5.1 features in depth ([cd802ef](https://github.com/zot24/zskills/commit/cd802ef21cb4a4977fabd64ccc94a0b709919e23))
* document v0.6 search command and skills-sh optional feature ([#5](https://github.com/zot24/zskills/issues/5)) ([793ec61](https://github.com/zot24/zskills/commit/793ec61d3e214a7735b0b7e37f10e6826d67c601))
* **site:** add CNAME for zskills.zot24.com ([a82cf7e](https://github.com/zot24/zskills/commit/a82cf7ef0a5726465dac36051116a19c7953fd65))
* **site:** mirror .md files + add llms.txt and llms-full.txt ([4e0be13](https://github.com/zot24/zskills/commit/4e0be135ab5bbde5e3e9275a627a528ad9958ec7))

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
