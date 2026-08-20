# PR Description Standard (ASD-STE100)

One card. Read it before you write a pull request description.

The house standard is **ASD-STE100 (Simplified Technical English)**. It is a hard rule, not a
suggestion. The rule covers the description text and the prose inside the mermaid diagram in that
description.

This card is the zskills copy of the house standard. The writing rules and the forked-chat rule are
identical across repositories. Only the test commands and the diagram storage rule differ, because
zskills is a Rust CLI.

The portable version of this standard is the `pr-standard` Agent Skill in
[`zot24/skills`](https://github.com/zot24/skills). That repository is the skill home. An agent that
needs the standard in another repository installs it from there:

```bash
zskills install zot24/skills --skill pr-standard
```

Do not add a copy of the skill to this repository.

Related: [`../../CLAUDE.md`](../../CLAUDE.md) · [`../../.github/pull_request_template.md`](../../.github/pull_request_template.md)

---

## 1. The four labels

Every PR carries four labels. A PR without all four is not ready for review.

| Group | Values |
|---|---|
| Type | `bug`, `enhancement`, `documentation` |
| Priority | `priority:critical`, `priority:high`, `priority:medium` |
| Size | `t-shirt:small`, `t-shirt:medium`, `t-shirt:big` |
| Area | `area:cli`, `area:dx`, `area:docs` |

More than one area label is allowed. Set the labels with `gh pr edit`:

```bash
gh pr edit 26 --add-label "bug,priority:high,t-shirt:big,area:cli"
```

`area:cli` covers command behaviour and the state zskills writes. `area:dx` covers the build, the
tests, and the tooling. `area:docs` covers this card, the template, and `docs/`.

---

## 2. The six writing rules

These six rules are the whole standard. Do not add more.

| Rule | Do this | Not this |
|---|---|---|
| One idea per sentence | "`doctor` reads `known_marketplaces.json`. It reports an entry without `lastUpdated`." | "`doctor` reads `known_marketplaces.json` and reports an entry without `lastUpdated`, which also covers the empty-string case." |
| Same word for the same thing | `marketplace` everywhere | `marketplace`, `tap`, `registry` in one PR |
| Active voice | "`install` writes `enabledPlugins`." | "`enabledPlugins` is written by `install`." |
| Imperative in steps | "Run `cargo test`." | "You will want to run `cargo test`." |
| No filler | "The entry needs a string timestamp." | "Basically the entry just needs a string timestamp in order to work." |
| Exact technical names | `known_marketplaces.json`, `enabledPlugins` | "the marketplace file", "the enabled map" |

Banned filler words: **basically, simply, just, actually, in order to, obviously, of course**.

Agreed nouns — use these, and no synonym for them: `plugin`, `marketplace`, `Agent Skill`,
`MCP server`, `manifest`, `inventory`, `scope`.

`plugin` and `Agent Skill` are different things. A plugin comes from a marketplace and lives in
`~/.claude/plugins/`. An Agent Skill is a `SKILL.md` directory and lives in `~/.agents/skills/`.
Do not call one the other.

---

## 3. Write the description in a forked chat

Do not draft the description in the same conversation that wrote the code. That conversation already
understands the change. It writes for itself, and a reviewer cannot follow the result.

1. Finish the code on the main chat.
2. Fork the conversation. Give the fork three things: the diff summary, the mermaid text, and the
   linked issues. Tell it to apply this card.
3. The fork writes the description **so that a common reviewer understands the change**, then puts
   it on the PR (`gh pr create` or `gh pr edit`).
4. Drop the fork.
5. Continue the main chat.

A fork that inherits the full coding conversation does not satisfy this rule. The writer must start
from the diff, not from the memory of writing it.

**The description does not have to be short.** Give the reviewer the context, the mechanism, and the
risk. STE100 makes the text clear. It does not cap the length. Cut filler, not information.

This applies to humans and to agents. Any in-repo agent or skill that opens a PR follows the same
five steps.

---

## 4. Which diagram the PR needs

Every PR carries a mermaid diagram of how the change works.

| Change type | Required mermaid |
|---|---|
| Command flow, or a call into another process | `sequenceDiagram` |
| Branching logic, or a classification | `flowchart` |

Scope the diagram to the pieces the PR touches. Do not redraw the whole binary.

zskills has no `docs/diagrams/` directory today, so the mermaid block in the PR body is the whole
requirement. If the repository later adds `docs/diagrams/`, commit each diagram as a `.mmd` source
there as well and reference it under the block.

---

## 5. Three mermaid patterns that break GitHub

GitHub renders unparseable mermaid as a plain code block. It prints no error, so nobody notices.

1. **`;` inside note or message text.** Mermaid reads `;` as a statement terminator, so
   `stamps the field; keeps the tap` ends the statement early.
2. **HTML in a participant alias.** `participant Doctor as doctor<br/>(--fix)` does not render.
   Write the alias on one line with no tag.
3. **A bare `%%` line.** The comment stripper needs one character after `%%`, so a bare `%%` glues
   onto the next line. Use `%% ---` as a separator. This breaks flowcharts, not sequence diagrams.

No CI job reads the PR body. The description is machine-unchecked. Open the PR page and look at the
diagram.

---

## 6. Testing evidence

Paste real output. These are the commands for this repository:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

zskills writes to the user's real `~/.claude` and `~/.agents`. When a PR changes a command that
writes state, exercise the built binary against a temporary home and paste the before/after:

```bash
export CLAUDE_HOME=/tmp/zs-sandbox/.claude AGENTS_HOME=/tmp/zs-sandbox/.agents
./target/release/zskills doctor
```

---

## 7. Scope

- **New PRs follow this card.** Old open PRs are not rewritten for the standard alone.
- If you edit an old PR's body for another reason, bring it up to the standard while you are there.
