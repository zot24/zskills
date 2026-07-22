# Implementation report: sparse Agent Skill installs

Branch: `feat/sparse-install` (off `main` @ 64274b2). Design in
`DESIGN-sparse-install.md` (written before implementation).

## What changed

**Sparse materialization (`src/agent_skill.rs`).** The root cause of the
motivating bug (`zskills install ogulcancelik/herdr` landing the whole Rust
project in `~/.agents/skills/`) was `skills_in_cache()`'s root-level fallback:
it returned the cache clone's root as the skill's source dir, and the installer
copied that verbatim — `src/`, `vendor/`, `.git/` and all. The clone-to-cache
architecture was already right (and `upgrade` depends on it), so only the
copy-out changed:

- Root-level skills now go through `install_root_skill_sparse()`, which
  materializes only: `SKILL.md`; the conventional dirs `references/`,
  `assets/`, `scripts/` when present; and relative paths referenced from
  SKILL.md (markdown link targets plus path-like inline-code spans). The
  parser (`referenced_relative_paths`) is liberal on purpose — every candidate
  is filtered through "does it exist in the clone?" — but rejects URLs,
  absolute paths, anchors, whitespace, and `..` escapes, and skips fenced code
  blocks.
- `copy_dir_recursive` now skips `.git/` in every layout.
- `skills/<name>/SKILL.md` installs are unchanged (already minimal).
- No inventory schema change: `Entry.source` already carries the origin repo,
  so `upgrade` keeps working exactly as before and re-materializes sparsely.

**`--skill <name>` flag (`src/cli.rs`, `src/commands/install.rs`).**
Non-interactive single-skill selection from a multi-skill repo. Conflicts with
`--all` (clap-level), bypasses the >5-skill size policy since the selection is
explicit, errors with the list of available names when the skill isn't found,
and bails if used without any repo spec.

**Doctor migration path (`src/commands/doctor.rs`).** A legacy full-repo
install reliably contains a `.git/` directory (it was a verbatim clone copy).
`zskills doctor` now flags such installs — "full-repo install … run
`zskills upgrade <name>` or `zskills doctor --fix` to re-install slim" — and
`doctor --fix` re-runs the install from the recorded inventory source, which
replaces the dir with the sparse copy. Both are explicit user actions; nothing
touches existing installs otherwise. `zskills upgrade` converts transparently
for manifest-managed skills, since it re-runs the same install path.

**Docs.** `docs/commands.md` documents the flag, the updated count-behavior
table, and sparse-install semantics.

**Deliberate non-change:** root-level skill naming stays cache-dir-derived
(`<owner>-<repo>`); switching to the frontmatter `name:` would break `upgrade`
for existing manifests/inventories (rationale in the design doc).

## Commits

| SHA | Message |
|---|---|
| c00e43e | docs: design proposal for sparse agent-skill installs |
| 7a650f4 | feat(agent-skill): sparse materialization for root-level skills |
| 4028009 | feat(install): `--skill <name>` to select one skill from a multi-skill repo |
| 15ab432 | chore: fix map-values clippy lint in list.rs (pre-existing, newer clippy) |
| f93bf2b | feat(doctor): flag legacy full-repo skill installs; `--fix` re-installs slim |
| 1206a24 | test: e2e coverage for sparse installs, `--skill`, and doctor slim-down |
| 6af1a80 | docs: document `--skill` flag and sparse root-level installs |

## Tests

Matching the repo's existing styles (unit tests in-module; e2e in
`tests/cli.rs` with `git init` + `file://` upstreams and a sandboxed fake
home). New coverage:

- Unit (7): referenced-path extraction — links, code spans, URL/absolute/`..`
  rejection, anchor + `./` stripping, fenced-block skipping, bare-word
  rejection; `sparse_root_paths` composition.
- E2E (6): root-level install is sparse (referenced + conventional paths in,
  `src/`/`vendor/`/`Cargo.toml`/`.git` out); `--skill` selects one skill /
  errors on unknown names with the available list / conflicts with `--all` /
  bypasses the large-collection abort; `doctor` flags a simulated legacy
  full-repo install and `--fix` slims it.
- Harness improvement: `XDG_CACHE_HOME` is now sandboxed in the test helper,
  so repo-install tests no longer write clone caches into the real `~/.cache`.

Results (matching CI: `cargo fmt --all --check`, `cargo clippy --all-targets
--all-features -- -D warnings`, `cargo test`):

- fmt: clean
- clippy: clean (`-D warnings`)
- tests: **34 unit + 49 e2e, all passing, 0 failed**
