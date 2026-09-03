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
  VALUTAZIONE_AI.md
  PIANO_GUI_TAURI.md
  PIANO_GUI_ESPANSIONE.md
  docs/cli-reference.md docs/installation.md
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

  # 5. Documentation/code consistency. These check only facts with a *single source of truth* --
  #    the filesystem, or crates/rustcopy-core/src/cli.rs -- and only across TRACKED_DOCS, like every gate above.
  #    Prose claims are deliberately out of scope: a prototype that grepped for sentences like
  #    "flag X is [NOT IMPLEMENTED]" and cross-checked cli.rs produced three findings, all three
  #    false positives (historical passages, and a correction reading "is no longer [NOT
  #    IMPLEMENTED]"). Regex cannot tell an assertion from a negation or a quotation. That class
  #    of drift is what the PR review bot is for; a gate people learn to ignore is worse than none.
  #    Test counts are excluded for the same reason: PIANO_MIGLIORAMENTI.md legitimately records
  #    past figures ("296/311 test, 19 Ago") that any naive equality check would flag.

  # 5a. Relative links resolve *from the linking file's own directory*. A link written while a
  #     doc lived in the repo root silently breaks when the doc moves under docs/ -- exactly what
  #     happened to cli-reference.md's RUNBOOK link (24 Agosto 2026).
  echo "== relative links resolve =="
  local dir target
  for f in "${TRACKED_DOCS[@]}" "${INDEX_FILES[@]}"; do
    [[ -f "$f" ]] || continue
    dir="$(dirname "$f")"
    while read -r target; do
      target="${target%$'\r'}"          # docs are CRLF here; a bare \r is invisible but non-empty
      [[ -n "$target" ]] || continue
      if [[ ! -e "$dir/$target" ]]; then
        echo "::error file=${f}::Broken relative link: ${target} (does not resolve from ${dir}/)"
        status=1
      fi
    done < <(
      # Destination only: strip an optional "title", drop in-page anchors, then keep just the
      # relative paths. Excluding by leading letter (the old `[^)h#]`) silently skipped any real
      # relative target starting with h -- howto/, helpers/ -- so filter by URI scheme instead.
      grep -o ']([^)]*)' "$f" 2>/dev/null \
        | sed 's/^](//; s/)$//' \
        | sed 's/[[:space:]]\+["'"'"'].*$//' \
        | sed 's/#.*//' \
        | grep -v '^[a-zA-Z][a-zA-Z0-9+.-]*:' \
        | sort -u
    )
  done
  [[ $status -eq 0 ]] && echo "ok"

  # 5b. No absolute file:/// links. These point into whoever wrote them's own disk: invisible to
  #     that author, broken for every other reader and in the rendered view on GitHub.
  #     RUNBOOK.md carried five of them for weeks.
  echo "== no absolute local links =="
  for f in "${TRACKED_DOCS[@]}"; do
    [[ -f "$f" ]] || continue
    if grep -q "file:///" "$f" 2>/dev/null; then
      echo "::error file=${f}::Contains an absolute file:/// link — use a repository-relative path"
      status=1
    fi
  done
  [[ $status -eq 0 ]] && echo "ok"

  # 5c. Every CLI flag is documented somewhere in the bundle. Catches a flag added to cli.rs
  #     without a matching line in the reference.
  echo "== every CLI flag is documented =="
  local field flag
  while read -r field; do
    field="${field%$'\r'}"
    [[ -n "$field" ]] || continue
    flag="--${field//_/-}"
    if ! grep -l -- "$flag" "${TRACKED_DOCS[@]}" >/dev/null 2>&1; then
      echo "::error file=crates/rustcopy-core/src/cli.rs::${flag} exists in cli.rs but is documented in no tracked .md"
      status=1
    fi
  done < <(
    # `#[arg(...)]` frequently spans several lines (--source, --dest and --report-path all do),
    # so a fixed -A window misses the field that follows. Track the attribute block instead and
    # take the first `pub <field>` after it closes.
    awk '
      /#\[arg\(/            { inattr = 1; islong = 0 }
      inattr && /long/      { islong = 1 }
      inattr && /\)\]/      { inattr = 0; pending = islong; next }
      pending && /pub [a-z_0-9]+/ {
        match($0, /pub [a-z_0-9]+/)
        print substr($0, RSTART + 4, RLENGTH - 4)
        pending = 0
      }
    ' crates/rustcopy-core/src/cli.rs | sort -u
  )
  [[ $status -eq 0 ]] && echo "ok"

  # 5d. Scripts and example configs referenced by the docs actually exist.
  echo "== referenced scripts and examples exist =="
  local ref
  while read -r ref; do
    ref="${ref%$'\r'}"
    [[ -n "$ref" ]] || continue
    if [[ ! -e "$ref" ]]; then
      echo "::error::Documentation references ${ref}, which does not exist"
      status=1
    fi
  done < <(grep -oh 'scripts/[a-z._-]*\.\(ps1\|sh\)\|examples/[a-z._-]*\.toml' "${TRACKED_DOCS[@]}" 2>/dev/null | sort -u)
  [[ $status -eq 0 ]] && echo "ok"

  # 5e. Mermaid: the two syntax errors that actually broke rendering on GitHub (23 Agosto 2026).
  #     A `subgraph` title containing a comma or `&` must use the `Id ["Title"]` form, and labels
  #     cannot carry backslash-escaped quotes. Deliberately a targeted lint rather than a full
  #     parse: validating properly needs the mermaid npm package, and an unpinned install would
  #     reintroduce the supply-chain problem this repo just pinned okf to avoid.
  echo "== mermaid syntax =="
  local ln
  for f in "${TRACKED_DOCS[@]}"; do
    [[ -f "$f" ]] || continue
    while IFS=: read -r ln _; do
      [[ -n "$ln" ]] || continue
      echo "::error file=${f},line=${ln}::Unquoted subgraph title containing ',' or '&' — use: subgraph Id [\"Title\"]"
      status=1
    done < <(
      # Two shapes are invalid: a bare title with ',' or '&' before any bracket, and a bracketed
      # title left unquoted (`subgraph Cache [Primary, secondary]`). The first pattern alone let
      # the second through.
      grep -nE '^[[:space:]]*subgraph [^["]*[,&]|^[[:space:]]*subgraph [^[]*\[[^"]*[,&]' "$f" 2>/dev/null
    )
    if awk '/^```mermaid/{m=1;next} /^```/{m=0} m' "$f" 2>/dev/null | grep -q '\\"'; then
      echo "::error file=${f}::Mermaid label contains an escaped quote — use the #quot; entity instead"
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
