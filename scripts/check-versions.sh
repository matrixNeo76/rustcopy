#!/usr/bin/env bash
#
# Fails when the four places that declare rustcopy's version disagree (F60).
#
# `Cargo.toml`'s `[workspace.package].version` is the single source of truth; the other three are
# copies that no build step keeps in step with it:
#
#   - installer/rustcopy.iss           MyAppVersion, which names the produced setup.exe
#   - crates/rustcopy-gui/tauri.conf.json   version, which Tauri stamps on the console
#   - crates/rustcopy-gui/ui/package.json   version, the frontend package
#
# Why a gate and not a generator: two of these are read by tools that run outside cargo (Inno
# Setup, npm), so a build-time sync would have to run before them and would be skipped by anyone
# invoking those tools directly — which is exactly how a version drifts. A check runs in CI and
# fails the branch instead, which is the only place the mistake can still be cheap.
#
# The installer script itself used to carry a comment admitting there was "no automated sync" and
# that drift had bitten this repo before. This is that comment, made to do something.

set -euo pipefail

cd "$(dirname "$0")/.."

fail=0

expected=$(sed -n '/^\[workspace\.package\]/,/^\[/p' Cargo.toml \
  | sed -n 's/^version = "\([^"]*\)".*/\1/p' | head -1)

if [ -z "$expected" ]; then
  echo "check-versions: could not read [workspace.package].version from Cargo.toml" >&2
  exit 1
fi

echo "source of truth: Cargo.toml [workspace.package].version = $expected"

check() {
  local label="$1" file="$2" found="$3"
  if [ "$found" = "$expected" ]; then
    printf '  ok   %-38s %s\n' "$label" "$found"
  else
    printf '  FAIL %-38s %s (expected %s)\n' "$label" "${found:-<not found>}" "$expected"
    fail=1
  fi
}

check "installer/rustcopy.iss" installer/rustcopy.iss \
  "$(sed -n 's/^#define MyAppVersion "\([^"]*\)".*/\1/p' installer/rustcopy.iss | head -1)"

check "crates/rustcopy-gui/tauri.conf.json" crates/rustcopy-gui/tauri.conf.json \
  "$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
      crates/rustcopy-gui/tauri.conf.json | head -1)"

check "crates/rustcopy-gui/ui/package.json" crates/rustcopy-gui/ui/package.json \
  "$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
      crates/rustcopy-gui/ui/package.json | head -1)"

if [ "$fail" -ne 0 ]; then
  echo
  echo "Update the declarations above to match Cargo.toml, or change Cargo.toml if the release" >&2
  echo "version itself is what moved. All four are part of one decision." >&2
  exit 1
fi

echo "all version declarations agree"
