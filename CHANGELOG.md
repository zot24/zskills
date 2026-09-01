# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases from this point forward are managed by [release-please](https://github.com/googleapis/release-please) based on [Conventional Commits](https://www.conventionalcommits.org/).

## [1.4.0](https://github.com/zot24/zskills/compare/v1.3.0...v1.4.0) (2026-09-01)


### Features

* one [[agent_skills]] stanza names many skills from one source ([#79](https://github.com/zot24/zskills/issues/79)) ([cfe7d88](https://github.com/zot24/zskills/commit/cfe7d88f09aaf891fd57af3b75815121897f1656))
* scan --mcp and migrate project MCP servers ([#76](https://github.com/zot24/zskills/issues/76)) ([11f72be](https://github.com/zot24/zskills/commit/11f72beafca9f6ad578b701552ff7e3908278cae))


### Bug Fixes

* doctor walks harness skill roots ([#74](https://github.com/zot24/zskills/issues/74)) ([a7d624b](https://github.com/zot24/zskills/commit/a7d624b46aa68ffed8883ee53cf9ced137716b2f))
* drop [[skills]] row on plugin remove and purge ([#72](https://github.com/zot24/zskills/issues/72)) ([31dc6ef](https://github.com/zot24/zskills/commit/31dc6ef637bc9e3260f6e401a30db7b86c14a703))
* name marketplaces from the manifest, not the repo basename ([#78](https://github.com/zot24/zskills/issues/78)) ([58a920d](https://github.com/zot24/zskills/commit/58a920dbc886added7868f1ffa705aec5770128a))
* plan npm agent-skill rows so sync dry-run matches apply ([#75](https://github.com/zot24/zskills/issues/75)) ([5fe0b5f](https://github.com/zot24/zskills/commit/5fe0b5f7966abdd4a612e4f17afdfcf8eb827aa4))
* show source kind on every list Agent Skill line ([#73](https://github.com/zot24/zskills/issues/73)) ([99f6182](https://github.com/zot24/zskills/commit/99f61822f83c84b2bc98aaa0a66312f41c25cd19))


### Documentation

* record Pi skill listing and upstream wiki-manager description limit ([#70](https://github.com/zot24/zskills/issues/70)) ([2a82ed8](https://github.com/zot24/zskills/commit/2a82ed8f4b1a64d08aa0adce2d214dfb90cc490f))

## [1.3.0](https://github.com/zot24/zskills/compare/v1.2.0...v1.3.0) (2026-08-30)


### Features

* copy Agent Skills from a marketplace clone via marketplace and path ([d9c065e](https://github.com/zot24/zskills/commit/d9c065e12afc5a4a7ac7671fb8e47a009db817de))
* doctor warns when hub wiki-manager is Claude-flavored for Pi or Grok ([8baa6d8](https://github.com/zot24/zskills/commit/8baa6d81f14b7e8916dc0e2ffce00e28a986f8cd))
* hint Agent Skill trees under plugins/*/skills after marketplace add ([41e83c7](https://github.com/zot24/zskills/commit/41e83c7ce91c62dfb6f97a8cebf2627a58179834))
* select an Agent Skill tree with path inside a source clone ([c48f505](https://github.com/zot24/zskills/commit/c48f50520d4cf703fe9ff86b8d5632f71d55ecb9))
* skill install --path for non-conventional Agent Skill roots ([b743ac8](https://github.com/zot24/zskills/commit/b743ac8f39d571de3ebefa0df152bb8e38304fb1))


### Bug Fixes

* declare marketplace source in skills.toml so sync can clone on a fresh machine ([675ad99](https://github.com/zot24/zskills/commit/675ad998a768b133eff83e3b8f78ae4ac4ea618e))
* **doctor:** guard the shipped SKILL.md against verbs removed in 1.0 ([#52](https://github.com/zot24/zskills/issues/52)) ([10528b8](https://github.com/zot24/zskills/commit/10528b8eefd483d6b03e604ac1a64eb8bec3b7dd))
* print plugins and next install command after marketplace add ([#56](https://github.com/zot24/zskills/issues/56)) ([2906957](https://github.com/zot24/zskills/commit/2906957746f5938904767cda1d252f50db148a51))


### Documentation

* declare llm-wiki plugin and OpenCode Agent Skills for Claude, Pi, and Grok ([ed4197f](https://github.com/zot24/zskills/commit/ed4197f6a3b8233ea9c596db5e9f097066fd0f06))
* update command surface and harness rules for 1.2.0 ([#48](https://github.com/zot24/zskills/issues/48)) ([470abd2](https://github.com/zot24/zskills/commit/470abd2f458e6bcae3139a6d199fd3390e96ae91))

## [1.2.0](https://github.com/zot24/zskills/compare/v1.1.0...v1.2.0) (2026-08-25)


### Features

* register the Agent Skill hub in Pi settings ([#45](https://github.com/zot24/zskills/issues/45)) ([b99398a](https://github.com/zot24/zskills/commit/b99398a607d695f1709dd517e276488de7567456))
* symlink hub skills into per-harness roots ([#46](https://github.com/zot24/zskills/issues/46)) ([e92139e](https://github.com/zot24/zskills/commit/e92139ee19bf92a6a3961db536c02710555b28fa))

## [1.1.0](https://github.com/zot24/zskills/compare/v1.0.0...v1.1.0) (2026-08-24)


### Features

* name which harnesses can see a plugin or Agent Skill ([#35](https://github.com/zot24/zskills/issues/35)) ([daad031](https://github.com/zot24/zskills/commit/daad031939808103e3382555d3c0f364ade241db)), closes [#34](https://github.com/zot24/zskills/issues/34)

## [1.0.0](https://github.com/zot24/zskills/compare/v0.9.0...v1.0.0) (2026-08-24)


### ⚠ BREAKING CHANGES

* typed plugin, skill, and mcp command groups ([#32](https://github.com/zot24/zskills/issues/32))

### Features

* typed plugin, skill, and mcp command groups ([#32](https://github.com/zot24/zskills/issues/32)) ([5562c42](https://github.com/zot24/zskills/commit/5562c42d375184e63ad1197e5726be6a46cd864e))

## [0.9.0](https://github.com/zot24/zskills/compare/v0.8.0...v0.9.0) (2026-08-22)


### Features

* pin a marketplace so update and upgrade cannot float it ([#28](https://github.com/zot24/zskills/issues/28)) ([b604aca](https://github.com/zot24/zskills/commit/b604aca4546d93206da24035b2619cbe3dadc037))


### Bug Fixes

* discover Agent Skills under .agents/skills/, not only skills/ ([#31](https://github.com/zot24/zskills/issues/31)) ([37334fb](https://github.com/zot24/zskills/commit/37334fb3921678ed2f222a51803350170732bfa9))
* stop reporting owned skills as unmanaged, and stop faking inventory ([#30](https://github.com/zot24/zskills/issues/30)) ([bfdf5ca](https://github.com/zot24/zskills/commit/bfdf5cae05b599698d9a684ca4dfa5e9bd1c6fa1))

## [0.8.0](https://github.com/zot24/zskills/compare/v0.7.0...v0.8.0) (2026-08-20)


### Features

* install from repo + cross-client skill path + sync --adopt ([#21](https://github.com/zot24/zskills/issues/21)) ([64274b2](https://github.com/zot24/zskills/commit/64274b2ee8be7a3363d66e10f98adeb859616ef7))
* sparse Agent Skill installs from git repos + install --skill flag ([#24](https://github.com/zot24/zskills/issues/24)) ([9623646](https://github.com/zot24/zskills/commit/96236466cd5de7bf9f2607451520de9a6f2c007e))
* zskills install &lt;owner/repo&gt; — direct install from a git repo ([#18](https://github.com/zot24/zskills/issues/18)) ([2756dc0](https://github.com/zot24/zskills/commit/2756dc037aa815bab80fc62ca57555a70ed71067))


### Bug Fixes

* sparse installs work on marketplace repos and nested skill layouts ([#25](https://github.com/zot24/zskills/issues/25)) ([7dcd9a5](https://github.com/zot24/zskills/commit/7dcd9a566eac1ccad13cf3b657c5c810ad937fa7))


### Documentation

* adopt the house PR standard — template, STE100 card, labels ([#27](https://github.com/zot24/zskills/issues/27)) ([80a6b59](https://github.com/zot24/zskills/commit/80a6b599d3a50a349cb0c894129bc38ee5b2e186))
* ship Agent Skill + MCP-aware docs for v0.7.0 ([#16](https://github.com/zot24/zskills/issues/16)) ([66ad32f](https://github.com/zot24/zskills/commit/66ad32f478692d13d6dd2eb6f7842984d33aad2b))
* **theme:** terminal aesthetic + readability tune + ← zot24.com link ([#23](https://github.com/zot24/zskills/issues/23)) ([a34aeae](https://github.com/zot24/zskills/commit/a34aeae682d760d36f029ef3d53e1265c5fe963d))
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
