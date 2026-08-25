# Closeout: sparse-install (PR #24)

Date: 2026-07-21. PR #24 squash-merged to `main` as `9623646`; all CI checks
(clippy, rustfmt, tests on macOS + Ubuntu) green. Local `feat/sparse-install`
branch deleted (remote deleted at merge).

## Binary

- Reinstalled from merged `main` via `cargo install --path . --force` →
  `~/.cargo/bin/zskills`.
- Reports **0.7.0** — correct: release-please owns versioning
  (`.release-please-manifest.json` = 0.7.0, release-type `rust`). No hand-bump;
  the `feat:` commit will produce a 0.8.0 release PR. The new sparse-install
  code is in the installed binary regardless of the version string.

## Live doctor run (real `~/.agents/skills/`)

`zskills doctor` found:

1. **`ogulcancelik-herdr` — full-repo install** (the motivating case): whole
   source tree with embedded `.git/`, `src/`, `vendor/`, `Cargo.lock`,
   `website/`, `flake.nix`, … **48 MB**.
2. `mercator` — tracked in inventory but bytes already missing on disk
   (pre-existing stale entry, unrelated to this work).

No other full-repo installs were flagged — `ogulcancelik-herdr` was the only
one.

## Fix applied (`zskills doctor --fix`)

- `ogulcancelik-herdr` re-installed slim from `ogulcancelik/herdr`:
  **48 MB → 1.4 MB**. Verified after: `SKILL.md` intact; `src/`, `vendor/`,
  `.git/`, `Cargo.lock`, `website/` all gone. Remaining contents: `SKILL.md`,
  `assets/`, `scripts/` (the conventional-dir rule; herdr keeps project dev
  scripts at the repo root, so ~1.4 MB of those ride along — known heuristic
  trade-off, documented in the PR).
- `mercator` stale inventory entry dropped.
- A follow-up `zskills doctor` reports: *All good — disk, inventory, and
  settings are in sync.*

## Left open

- **Release**: wait for/merge the release-please PR to cut v0.8.0 and publish;
  then `cargo install zskills` users get sparse installs.
- **Heuristic refinement (optional)**: only include conventional dirs
  (`scripts/`, `assets/`, `references/`) when SKILL.md references them, to
  avoid dragging in project-level dirs like herdr's — at the risk of dropping
  files skills rely on silently.
- **Naming (deliberate non-change)**: root-level skills still install under
  the cache-derived name (`ogulcancelik-herdr`, not frontmatter `herdr`);
  switching would break `upgrade` for existing manifests.
- Old local topic branches (`feat/install-from-repo`, `docs/zot24-home-link`,
  …) remain; prune at leisure per the usual worktree/branch hygiene.
