#!/usr/bin/env bash
#
# Single source of truth for this project's OKF documentation bundle (AGENTS.md rule 15).
#
#   scripts/okf-docs.sh index   # regenerate the index.md files, then commit them
#   scripts/okf-docs.sh check   # run every OKF gate (what CI runs)
#
# Why a staging directory instead of running okf against the repo root:
# `okf` operates on a *bundle* (a directory) and has no exclude-pattern flag, so pointing it
# at the repo root would also walk `.agents/skills/` and `graphify-out/` — directories this
# convention does not own — and `okf index` would write index.md files into them. Staging
# exactly the tracked docs sidesteps that with no extra tooling. The staged layout mirrors
# the repo layout, so paths inside the generated indexes are valid at their real locations.
#
# `okf index` preserves existing frontmatter while rewriting the body, so the root index's
# `okf_version` declaration survives regeneration, and regeneration is byte-idempotent —
# which is what makes the `check` diff a usable CI gate rather than a permanently red one.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# The documents this convention owns. A new permanent doc goes here and nowhere else —
# every gate below is driven from this one list.
TRACKED_DOCS=(
  README.md ARCHITECTURE.md ANALYSIS.md ROADMAP.md CLAUDE.md AGENTS.md
  RUNBOOK.md CHANGELOG.md SECURITY.md NEXT_SESSION_PROMPT.md PIANO_MIGLIORAMENTI.md
  docs/archive/PIANO_NOTIFY_SERVER.md docs/archive/AGENT_HARNESS_PLAN.md
)

# Bundle indexes okf generates, one per directory level holding concepts. These are not
# concepts themselves (§12: an index.md declares only `okf_version`), so they are excluded
# from the per-file `okf parse` pass below.
INDEX_FILES=(index.md docs/index.md docs/archive/index.md)

stage_bundle() {
  local stage="$1"
  local f
  for f in "${TRACKED_DOCS[@]}" "${INDEX_FILES[@]}"; do
    [[ -f "$f" ]] || continue
    mkdir -p "$stage/$(dirname "$f")"
    cp "$f" "$stage/$f"
  done
}

cmd_index() {
  local stage f
  stage="$(mktemp -d)"
  trap 'rm -rf "$stage"' RETURN
  stage_bundle "$stage"
  okf index "$stage" >/dev/null
  for f in "${INDEX_FILES[@]}"; do
    if [[ ! -f "$stage/$f" ]]; then
      echo "error: okf did not generate $f" >&2
      return 1
    fi
    mkdir -p "$(dirname "$f")"
    cp "$stage/$f" "$f"
    echo "wrote $f"
  done
}

cmd_check() {
  local status=0 stage f

  # 1. No root-level .md file outside the tracked set. A doc with no OKF frontmatter and no
  #    entry here can otherwise go unnoticed for weeks — AGENT_HARNESS_PLAN.md sat in root
  #    for 10 days before a manual audit caught it (20 Agosto 2026).
  echo "== untracked root-level .md files =="
  for f in *.md; do
    if [[ ! " ${TRACKED_DOCS[*]} ${INDEX_FILES[*]} " =~ " ${f} " ]]; then
      echo "::error file=${f}::Untracked root-level .md — add OKF frontmatter and add it to TRACKED_DOCS in scripts/okf-docs.sh (or move it under docs/archive/ if superseded)"
      status=1
    fi
  done
  [[ $status -eq 0 ]] && echo "ok"

  # 2. Every tracked doc parses as an OKF concept.
  echo "== okf parse =="
  for f in "${TRACKED_DOCS[@]}"; do
    if [[ ! -f "$f" ]]; then
      echo "::error file=${f}::Listed in TRACKED_DOCS but missing from the working tree"
      status=1
      continue
    fi
    okf parse "$f" >/dev/null || { echo "::error file=${f}::okf parse failed"; status=1; }
  done
  [[ $status -eq 0 ]] && echo "ok"

  stage="$(mktemp -d)"
  trap 'rm -rf "$stage"' RETURN
  stage_bundle "$stage"

  # 3. Bundle-level conformance and health. validate checks more than parse alone
  #    (cross-concept link resolution); lint catches orphan concepts, which is what the
  #    index files exist to prevent.
  echo "== okf validate =="
  okf validate "$stage" || status=1
  echo "== okf lint =="
  okf lint "$stage" || status=1

  # 4. The committed indexes match what okf would generate right now.
  echo "== index freshness =="
  okf index "$stage" >/dev/null
  for f in "${INDEX_FILES[@]}"; do
    if ! diff -u "$f" "$stage/$f" >/dev/null 2>&1; then
      echo "::error file=${f}::Out of date — run scripts/okf-docs.sh index and commit the result"
      diff -u "$f" "$stage/$f" || true
      status=1
    fi
  done
  [[ $status -eq 0 ]] && echo "ok"

  return $status
}

case "${1:-}" in
  index) cmd_index ;;
  check) cmd_check ;;
  *) echo "usage: $0 {index|check}" >&2; exit 2 ;;
esac
