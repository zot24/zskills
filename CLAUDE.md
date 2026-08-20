# CLAUDE.md

See [`AGENTS.md`](AGENTS.md). It is the source of truth for every agent working in this
repository, Claude Code included.

The rules that bind hardest:

- **Every PR fills [`.github/pull_request_template.md`](.github/pull_request_template.md).** Do not
  pass a one-paragraph `-b` to `gh pr create`. Use `--body-file`.
- **Every PR carries four labels**: type (`bug` | `enhancement` | `documentation`), `priority:*`,
  `t-shirt:*`, and at least one `area:*`.
- **Every PR description is ASD-STE100**, written in a forked chat, and carries a mermaid diagram.
  Card: [`docs/guides/PR_DESCRIPTION_STANDARD.md`](docs/guides/PR_DESCRIPTION_STANDARD.md).
  The portable skill is `pr-standard` in [`zot24/skills`](https://github.com/zot24/skills), not here.
- **Never run a state-changing command against your real `~/.claude`.** Use `CLAUDE_HOME` and
  `AGENTS_HOME`.
- Tests are `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
