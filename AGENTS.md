# AGENTS.md — zskills

Rules for any agent working in this repository: Claude Code, Grok, Codex, or a human following the
same checklist. `CLAUDE.md` points here; this file is the source of truth.

zskills is a Rust CLI that manages plugins, Agent Skills, and MCP servers for agentic coding CLIs.
It writes to the user's real `~/.claude/` and `~/.agents/`. Treat every state-changing command as
something that can damage a working machine.

---

## 1. Vocabulary

Use these words, and no synonym for them:

| Term | Means |
|---|---|
| `plugin` | Distributed by a marketplace. Lives in `~/.claude/plugins/`. Enabled in `settings.json` → `enabledPlugins`. |
| `marketplace` | A registered source of plugins. Recorded in `~/.claude/plugins/known_marketplaces.json`. Never call it a tap or a registry in a PR. |
| `Agent Skill` | A `SKILL.md` directory. Lives in `~/.agents/skills/`. Not a plugin. |
| `MCP server` | Declared in `~/.claude.json`, `<project>/.mcp.json`, or a settings file. |
| `manifest` | The user's `skills.toml`. |
| `inventory` | What is recorded as installed: `installed_plugins.json` for plugins, `.zskills.json` for Agent Skills. |
| `scope` | `user`, `project`, or `local`. |

A plugin and an Agent Skill are different things. Do not call one the other.

---

## 2. Pull requests — HARD RULES

**Every PR fills [`.github/pull_request_template.md`](.github/pull_request_template.md).** Do not
pass a one-paragraph `-b` to `gh pr create`. Write the body to a file and pass `--body-file`:

```bash
gh pr create --title "..." --body-file /tmp/pr-body.md
```

The template has six sections and all six are filled: Labels, Summary, How It Works, Linked Issues,
Testing Evidence, Additional Notes.

**Every PR carries four labels.** Type (`bug` | `enhancement` | `documentation`), `priority:*`,
`t-shirt:*`, and at least one `area:*` (`area:cli`, `area:dx`, `area:docs`). A PR without all four
is not ready for review.

```bash
gh pr edit <n> --add-label "bug,priority:high,t-shirt:medium,area:cli"
```

**Every PR description is written in ASD-STE100 (Simplified Technical English).** The full card is
[`docs/guides/PR_DESCRIPTION_STANDARD.md`](docs/guides/PR_DESCRIPTION_STANDARD.md). It is the house
standard, and it is the same in every repository that uses it. The portable Agent Skill that carries
it across repositories is `pr-standard` in [`zot24/skills`](https://github.com/zot24/skills) — that
repository is the skill home. Do not add a copy of it here. Six rules, and no more:

- One idea per sentence. Keep sentences short.
- The same word for the same thing. See the vocabulary table above. No synonyms inside one PR.
- Active voice. Imperative in steps.
- No filler: *basically, simply, just, actually, in order to, obviously, of course*.
- Technical names stay exact: `known_marketplaces.json`, `enabledPlugins`, `~/.agents/skills/`.
- **Write the body in a forked chat.** Do not draft it in the conversation that wrote the code —
  that conversation knows the change already, so it writes for itself instead of for a reviewer.
  Finish the code on the main chat. Fork it. Give the fork the diff summary, the mermaid text, and
  the linked issues. The fork writes the description **so that a common reviewer understands the
  change**, checks the mermaid renders on GitHub, and posts it. Then drop the fork. A fork that
  inherits the whole coding conversation does not satisfy this rule: the writer must start from the
  diff. **Length is whatever the reviewer needs** — the target is comprehension, not brevity.

**Every PR description carries a mermaid diagram** of how the change works: call flow, or the
branching logic the change adds. `sequenceDiagram` for a flow, `flowchart` for a classification.
GitHub renders ```mermaid blocks natively.

**A broken diagram fails silently.** GitHub renders unparseable mermaid as a plain code block with
no error, and **no CI job reads the PR body**. Three traps: a `;` inside note or message text, HTML
such as `<br/>` in a participant alias, and a bare `%%` line. Open the PR page and look at it.

zskills has no `docs/diagrams/` directory. The mermaid block in the body is the whole requirement.
If that directory is added later, commit the `.mmd` source alongside and reference it under the
block.

---

## 3. Testing evidence

Paste real output into the Testing Evidence section. Run all three:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`RUSTFLAGS: "-D warnings"` is set in CI, so a warning fails the build.

**Never exercise a state-changing command against your real home.** Point `CLAUDE_HOME` and
`AGENTS_HOME` at a temporary directory and paste the before/after of the files the command touched:

```bash
export CLAUDE_HOME=/tmp/zs-sandbox/.claude AGENTS_HOME=/tmp/zs-sandbox/.agents
./target/release/zskills doctor
```

The test suite sets `ZSKILLS_NO_CLAUDE_CLI=1` so no test reaches the developer's real `claude`
binary. Keep it that way: `claude` reads `CLAUDE_CONFIG_DIR`, not `CLAUDE_HOME`.

---

## 4. Issues

Open an issue before non-trivial work. Use the same label groups as a PR: priority, size, and area.

---

## 5. Release

`release-please` owns versioning. Do not hand-edit `Cargo.toml` versions or
`.release-please-manifest.json`. Write Conventional Commit subjects (`fix:`, `feat:`, `docs:`) —
they drive the changelog and the version bump.
