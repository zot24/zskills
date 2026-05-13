#!/usr/bin/env bash
# Augment the mdBook output with raw .md files and llms.txt / llms-full.txt
# so AI agents can fetch one URL and ingest the whole project.
# https://llmstxt.org for the convention.

set -euo pipefail

SRC="${1:-docs}"
OUT="${2:-book}"
BASE="${SITE_URL:-https://zot24.github.io/zskills}"

# 1. Copy raw .md files alongside the generated HTML.
#    Lets AI tools fetch https://site/commands.md instead of parsing HTML.
for f in "$SRC"/*.md; do
  [ -e "$f" ] || continue
  base=$(basename "$f")
  case "$base" in
    SUMMARY.md) continue ;;  # mdbook-internal, not navigable content
  esac
  cp "$f" "$OUT/$base"
done

# 2. llms.txt — short index for AI agents
{
  echo "# zskills"
  echo
  echo "> Package manager for Claude Code skills. Declarative install across marketplaces, single static Rust binary. Manages plugins (via marketplaces, settings.json) AND Agent Skills (raw SKILL.md format under ~/.claude/skills/) from one skills.toml. Supports git sources, GitHub-archive tarballs (for non-git marketplaces), and npm packages (with glob ownership via claims patterns)."
  echo
  echo "## Docs"
  echo
  echo "- [Introduction]($BASE/index.md): Overview, install, quick start"
  echo "- [Commands]($BASE/commands.md): Every subcommand with flags and defaults"
  echo "- [Use cases]($BASE/use-cases.md): 12 worked workflows (bootstrap a machine, centralize duplicates, adopt npm bundles, one-command upgrade, etc.)"
  echo "- [Architecture]($BASE/architecture.md): Three-state model (intent / inventory / activation), marketplace update strategies, ownership tracking"
  echo "- [Troubleshooting]($BASE/troubleshooting.md): Diagnostic recipes"
  echo "- [Changelog]($BASE/changelog.md): Release history"
  echo
  echo "## Source"
  echo
  echo "- [GitHub repository](https://github.com/zot24/zskills)"
  echo "- [Issues](https://github.com/zot24/zskills/issues)"
  echo "- [Releases](https://github.com/zot24/zskills/releases)"
  echo
  echo "## Optional"
  echo
  echo "- [llms-full.txt]($BASE/llms-full.txt): All docs concatenated for single-fetch ingestion"
} > "$OUT/llms.txt"

# 3. llms-full.txt — every doc concatenated, for one-shot context loading
{
  echo "# zskills — full documentation"
  echo
  echo "Source: https://github.com/zot24/zskills"
  echo "Site: $BASE"
  echo
  for f in "$SRC/index.md" "$SRC/commands.md" "$SRC/use-cases.md" "$SRC/architecture.md" "$SRC/troubleshooting.md" "$SRC/changelog.md"; do
    [ -e "$f" ] || continue
    echo
    echo "---"
    echo "## $(basename "$f" .md)"
    echo "---"
    echo
    cat "$f"
  done
} > "$OUT/llms-full.txt"

echo "Wrote $OUT/llms.txt and $OUT/llms-full.txt"
echo "Mirrored .md files: $(find "$OUT" -maxdepth 1 -name '*.md' | wc -l | tr -d ' ')"
