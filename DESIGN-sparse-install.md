# Design: sparse Agent Skill installs from git repos

## Problem

`zskills install <owner>/<repo>` clones the repo into
`$XDG_CACHE_HOME/zskills/agent-skills/<owner>-<repo>/` and copies the skill's
source directory into `~/.agents/skills/<name>/`. For the `skills/<name>/SKILL.md`
layout that copy is already minimal (the skill subdirectory *is* the skill).
But for a repo with a **root-level SKILL.md** (e.g. `ogulcancelik/herdr`, a full
Rust project that also ships a skill), `skills_in_cache()` returns the cache
root as the skill's source dir, so the install copies the *entire* repo —
`src/`, `vendor/`, `Cargo.lock`, `website/`, `.git/`, megabytes of bytes the
skill never uses.

## Approach: keep clone-to-cache, fix materialization

The cache clone already gives us everything git sparse-checkout would, and
`upgrade` already depends on it (`ensure_cache` → `git pull` → re-copy). So we
keep the clone-to-cache design and change only *what gets copied out*:

- **`skills/<name>/` layout** — unchanged: copy the whole skill subdirectory
  (that directory is the skill's own tree by convention).
- **Root-level SKILL.md** — sparse copy. Materialize only:
  1. `SKILL.md` itself;
  2. conventional skill dirs if present: `references/`, `assets/`, `scripts/`;
  3. any *relative paths referenced from SKILL.md* (markdown links
     `[x](path)` and inline-code spans `` `path` ``) that actually exist in
     the repo. The parser is deliberately liberal because every candidate is
     filtered through "does this path exist in the clone?" — a mention of
     `src/main.rs` copies that one file, never the whole `src/` tree.
     Paths with `..` components, absolute paths, URLs, and anchors are rejected.
- **`.git/` is never copied** in any layout (the recursive copy now skips it).

No inventory schema change is needed: `Entry.source` already records the origin
repo, and `upgrade` refreshes from the cache clone exactly as before — the next
upgrade of a root-level skill simply re-materializes sparsely.

## `--skill <name>` flag

`zskills install <owner>/<repo> --skill <name>` selects a single skill from a
multi-skill repo non-interactively (the same name shown by the survey /
manifest `name` field). It conflicts with `--all` (clap `conflicts_with`), and
errors if the named skill isn't in the repo. It also bypasses the >5-skill
size-policy prompt, since the selection is explicit. If given alongside
marketplace-plugin specs, those specs are rejected with a hint (the flag is
meaningful only for repo installs).

## Migration / compat

- Existing full-repo installs keep working untouched — nothing rewrites
  `~/.agents/skills/` outside explicit actions.
- **Detection**: a legacy full-repo install reliably contains a `.git/`
  directory (it was a verbatim copy of the clone). `zskills doctor` flags such
  dirs: *"full-repo install; run `zskills upgrade <name>` or re-install to
  slim"*.
- **Conversion**: `zskills upgrade` (and `doctor --fix`, both explicit user
  actions) re-run the install path, which deletes the dest dir and re-copies
  sparsely — the transparent slim-down. `doctor --fix` only re-installs
  entries whose inventory `source` is a repo spec it can re-fetch.

## Non-changes (deliberate)

- Root-level skill naming stays cache-dir-derived (`<owner>-<repo>`). Using
  the frontmatter `name:` would be nicer, but it would break `upgrade` for
  existing manifests/inventories that recorded the old name. Separate change
  if ever.
- No git sparse-checkout: it saves clone bytes but complicates `upgrade`
  and buys little once the copy-out is sparse. `--depth 1` already bounds
  history.

## Tests

- Unit (in `agent_skill.rs`): referenced-path extraction (links, code spans,
  rejection of URLs/absolute/`..`), sparse path-set computation.
- E2E (in `tests/cli.rs`, existing `git init` + `file://` style):
  - root-level skill installs SKILL.md + referenced file + conventional dirs,
    and does **not** install `src/`, `Cargo.toml`, `.git/`;
  - `--skill` selects one skill from a multi-skill repo; unknown name errors;
    conflicts with `--all`; bypasses the large-collection abort;
  - `doctor` flags a simulated legacy full-repo install and `--fix` slims it.
