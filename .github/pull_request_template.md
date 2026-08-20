<!--
WRITE THIS DESCRIPTION IN ASD-STE100 (Simplified Technical English). HARD RULE.

  One idea per sentence · the same word for the same thing (plugin, marketplace,
  Agent Skill, MCP server, manifest, inventory — no synonyms in one PR) · active
  voice, imperative in steps · no filler (basically, simply, just, in order to) ·
  exact technical names (known_marketplaces.json, enabledPlugins, ~/.agents/skills/).

Produce this body in a FORKED CHAT, not in the conversation that wrote the code
— that conversation already understands the change and writes for itself. Fork
it, hand the fork the diff summary + the mermaid text + the linked issues, let it
write the description SO A COMMON REVIEWER UNDERSTANDS THE CHANGE and post it,
then drop the fork and continue the main chat.

The description does NOT have to be short. Give the reviewer the context, the
mechanism, and the risk. STE100 makes the text clear; it does not cap length.
Cut filler, not information.

Card: docs/guides/PR_DESCRIPTION_STANDARD.md
-->

## Labels (REQUIRED)
<!--
Every PR carries four labels. Set them with `gh pr edit <n> --add-label "..."`.
A PR without all four is not ready for review.
-->
- [ ] Type: `bug` | `enhancement` | `documentation`
- [ ] Priority: `priority:critical` | `priority:high` | `priority:medium`
- [ ] Size: `t-shirt:small` | `t-shirt:medium` | `t-shirt:big`
- [ ] Area: `area:cli` | `area:dx` | `area:docs` (more than one is allowed)

## Summary
- 

## How It Works (diagram — REQUIRED)
<!--
Every PR MUST include a mermaid diagram of how the change works: data flow,
sequence of calls, or architecture of the pieces touched. GitHub renders
```mermaid blocks natively as images. Pick the type that fits —
sequenceDiagram (call flows), flowchart (branching logic). Scope it to the
pieces this PR touches.

MERMAID THAT BREAKS GITHUB SILENTLY — no `;` inside note or message text, no
HTML (<br/>) in a participant alias, no bare `%%` line (use `%% ---`). GitHub
renders unparseable mermaid as a plain code block with no error, and no CI job
ever reads this description. Open the PR page and look at the diagram.
-->

```mermaid
flowchart LR
    A[Replace] --> B[this] --> C[diagram]
```

## Linked Issues / ADRs
- Resolves: <!-- e.g., #123 -->
- Part of: <!-- e.g., #456 -->

## Testing Evidence
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Ran the built binary against a sandboxed `CLAUDE_HOME` (state-changing commands only)

```
<insert command output>
```

<!--
zskills writes to the user's real ~/.claude and ~/.agents. If this PR changes a
command that writes state, exercise it with CLAUDE_HOME and AGENTS_HOME pointed
at a temporary directory, and paste the before/after of the files it touched.
-->

## Additional Notes for Reviewers
- 
