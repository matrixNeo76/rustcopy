---
type: Log
title: Changelog
description: Cronologia lineare delle versioni, in stile Keep a Changelog.
status: stable
generated:
  by: process:claude-code
  at: 2026-08-06T00:00:00Z
---

# Changelog

All notable changes to this project are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); version numbers match `Cargo.toml`.

For full technical detail behind any entry, see `ANALYSIS.md` (defect list, `D<N>`) and
`ROADMAP.md` (feature list, `F<N>`) — this file is a linear, user-facing summary of both.

## [Unreleased]

### Fixed
- **D13**: log lines emitted during a `[[jobs]]` multi-job batch (including those emitted inside
  `tokio::task::spawn_blocking`, notably the robocopy invocation itself) are now tagged with the
  owning job's name, via a `tracing` span propagated through a new `spawn_blocking_with_span`
  helper.
- **D14**: `GenerationManifest::save` and `IngestCache::save_to` now write atomically (temp file +
  rename) instead of a bare `fs::write` — a manifest at real-world scale (1.34M files) can reach
  ~174 MB per generation, and a crash mid-write previously risked corrupting it, permanently
  breaking future incremental/differential/retention runs against that destination.
- **D15**: a copy failure in a `--backup-type` generation backup now returns exit code 1 (transfer
  failed) instead of 2 (usage/unrecoverable error), matching the plain-sync pipeline's semantics,
  and always writes a JSON report (previously none was written on this path).
- **D16**: `vss::remap_to_shadow` produced a wrong (mixed `/`/`\`) path when run on a non-Windows
  host — found by the project's first-ever Linux CI run. No production impact (the function is
  only reachable from Windows-only code paths), but its pure logic and unit test were not
  platform-gated and had never actually been exercised on Linux before. Also fixed several tests
  that were stale (asserting a `--pattern` default changed long ago) or missing a `#[cfg(windows)]`
  gate they needed, all likewise never caught before this session's CI addition.

- **D17**: `--min-age-days`/`--max-age-days` are now applied by the prescan and the naive engine
  too, not only by the real robocopy transfer, so the two no longer disagree on which files are in
  scope. Their `--help` text also had the direction inverted; corrected after verifying the real
  `robocopy.exe` semantics empirically.
- **D18**: the default log level dropped from `debug` to `info` — the per-file `debug` line had
  produced a 356 MB log on a real 1.34M-file run — and `--log-max-bytes` now rotates *during* a
  run, not only at the next process start. A failed rotation no longer resets the byte counter,
  which had let the file grow past the cap unchecked.
- **D19**: the generation manifest is now NDJSON, appended one line per generation, instead of the
  whole history being rewritten on every run (~174 MB per generation at real-world scale). Pre-D19
  manifests still load and are migrated forward on the next write; a torn trailing line from an
  interrupted append is recovered rather than fatal.
- **D20**: the manifest is no longer loaded in full by callers that don't need it. `--backup-type
  full` reads nothing at all, incremental/differential stream out only their reference generation,
  and retention loads a metadata-only index. Measured: 580 MB retained before, 145 MB and ~0 MB
  respectively after.
- **D21**: the scan inventory is shared rather than copied at each hop. `verify` alone had held
  four live copies of the whole file list; measured 580 MB before, 145 MB after.

### Changed
- **Breaking (library API)**: `ScanSummary::files` is now `Arc<[ScannedFile]>` instead of
  `Vec<ScannedFile>` (D21). Read-only uses are unaffected — it derefs to `&[ScannedFile]` — but
  code that moved or mutated the `Vec`, or constructed a `ScanSummary` literal, needs updating
  (`.into()` on construction, `Arc::clone` to share, `.to_vec()` if an owned copy is genuinely
  wanted). Only the `robocopy_ingest` binaries consume this today; flagged here because the type
  is `pub` and the next release carrying it should be semver-major.

### Repository
- Added `LICENSE` (MIT), `SECURITY.md`, `.editorconfig`, `.github/workflows/ci.yml` (test on
  Windows + Linux, `cargo fmt --check`, `cargo clippy -D warnings`), `.github/dependabot.yml`.
- Filled in `Cargo.toml` metadata (`repository`, `homepage`, `keywords`, `categories`, `readme`,
  corrected `description`).
- Tagged the release history retroactively: `v0.2.0`, `v5.1.0`, `v5.4.0`, `v5.4.1`, `v5.4.2`,
  `v6.0.0`.
- `cargo fmt --all` applied across the whole tree (mechanical, no behavior change) so the new CI's
  `fmt --check` starts green.

## [6.0.0] - 2026-08-05

### Added
- **F30**: `--vss-snapshot` — Volume Shadow Copy snapshot of the source before copying, via
  `vssadmin.exe`.
- **F31**: `--resume-from` — writes a checkpoint on `Ctrl+C`, resumable via `--resume-from
  <checkpoint>` (relies on robocopy's own same-size-same-timestamp skip, not mid-file resume).
- **F33**: `[[jobs]]` — multiple backup jobs in one TOML config file, run sequentially in one
  process.
- **F34**: `--backup-type <full|incremental|differential>` — Cobian-style backup generations, each
  recorded in a per-destination manifest.
- **F35**: `--keep-generations <N>` — retention/rotation of old generations by cycle (a `full` plus
  its following `incremental`/`differential` runs).
- **F36**: `--install-schedule`/`--uninstall-schedule` — Task Scheduler integration.
- **F37**: `--install-service`/`--uninstall-service` — real Windows Service Control Manager
  integration (idle service infrastructure; F41 later builds real work on top of it).
- **F39**: `--pre-command`/`--post-command` — run a command before/after the backup.

### Fixed
- **F26a-d** (milestone 5.2.0): mirror-safety threshold, async `check_mirror_safety`, report schema
  version bump, `--exclude-junctions`.
- **F27-F29d** (milestone 5.3.0): `--log-level`/`--quiet`/log rotation, `--fast-verify`,
  `--ignore-transient-missing`, `xxh3` hash algorithm, dedicated integrity-failure exit code,
  removal of unused `CopyRequestBuilder` dead code.

## [5.4.2] - 2026-08-01

### Added
- **F25a/F25b**: real streaming AES-256-GCM encryption/decryption (`--encrypt-aes256`/`--decrypt`),
  1 MiB chunks with atomic temp-file-then-rename writes — closes the "encrypt whole file in RAM"
  and "no decrypt path" defects.

## [5.4.1] - 2026-07-31

### Fixed
- **F24**: `--restore-from` is now actually reachable from the CLI — `--source`/`--dest` were
  unconditionally required by a clap default-value bug, making restores unusable without dummy
  flags.

## [5.4.0] - 2026-07-31

### Added
- `notify-server` — a separate, feature-gated (`--features notify-server`) axum binary that
  receives `--webhook-url` POSTs and fans them out to configurable notification channels
  (log/ntfy/generic webhook sinks).

## [5.1.0] - 2026-07-30

### Added
- Real implementations of the three critical safety/robustness gaps identified in the initial
  audit: `--mirror` purge safety check, correct CP850/OEM console decoding, and `Ctrl+C` killing
  only the tracked robocopy.exe child instead of every robocopy.exe on the host.

## [0.2.0] - 2026-07-30

Initial commit.
