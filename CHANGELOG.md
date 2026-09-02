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

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Version numbers match `Cargo.toml`.

For full technical detail behind any entry, see `ANALYSIS.md` (defect list, `D<N>`) and
`ROADMAP.md` (feature list, `F<N>`) — this file is a linear, user-facing summary of both.

## [Unreleased]

### Added
- **A desktop console** (milestone 7.0.0), shipped as an **optional component** of the installer.
  It reads what rustcopy already wrote and prepares configuration proposals. It does not run
  backups, and it never copies, deletes, schedules or installs. Five panes: the jobs a TOML
  describes, every resolved setting, an editor, the run history with its deterministic analysis,
  and a help page.
  - *Settings* shows the two things the TOML does not state: which layer supplied the value that
    wins for each job, and which settings carry a consequence — `mirror` deletes, `dry_run` copies
    nothing, `fast_verify` trusts the source, `xxh3` is not cryptographic, `keep_generations`
    prunes. A configured `webhook_url` is cut to scheme and host before it crosses the IPC
    boundary: a webhook URL *is* the credential, and a settings pane ends up in screenshots.
  - *Editor* is the only write path in the application, and it writes a **new** file: the running
    configuration is never touched, and the substitution stays with the operator. One rule governs
    it — the editor may narrow risk, never widen it. `mirror` cannot go off → on (it can be turned
    off), retention cannot be introduced or lowered, the prescan cannot be removed from a mirroring
    job, and omitting a job never deletes it. `webhook_url`, `pre_command` and `post_command` are
    outside the form and copied verbatim.
  - Native file pickers, one shared path across panes, recent files, and empty states that say what
    each pane needs.
- `keyring:NAME` as a fourth form for `--encrypt-aes256`/`--decrypt` keys, reading the **Windows
  Credential Manager**, plus `--set-credential` (secret read from **stdin**, never an argument,
  which would be visible in the process list) and `--delete-credential`. `env:`/`file:`/literal are
  unchanged: a scheduled task's command is captured at install time and cannot be migrated by
  editing a file.
- `scripts/check-versions.sh` and a CI job behind it: four files declare the release version and no
  build step held them together. The installer script's own header had admitted the drift without
  preventing it.

### Changed
- The installer is now a **single** setup with the console as an optional component, rather than a
  second bundle produced by Tauri's bundler. Measured before deciding: the console is 8.9 MB
  against a 13.7 MB CLI install, because Tauri renders through the system WebView2 instead of
  shipping a browser engine. Two installers would have meant two version streams and two
  SmartScreen reputations for 8.9 MB. WebView2 is detected and reported — only when the console is
  selected — without blocking setup.

### Fixed
- The installed console loaded the **dev server** instead of its own frontend, showing
  `ERR_CONNECTION_REFUSED` on any machine without Vite running — which is every machine an
  installer reaches. Tauri decides dev-vs-production from the `custom-protocol` feature, not from
  the cargo profile, and `frontendDist` pointed at a path that did not exist; the first defect
  hid the second. Reported by a user launching the application: `cargo build`, `clippy` and 422
  tests were all green on that binary, because none of them opens a window. A `compile_error!` now
  makes a release build without `custom-protocol` fail to compile, and CI builds the frontend so it
  checks what ships. See `ANALYSIS.md` D22.
- Editor drafts stayed bound to the shared configuration path rather than the file they were read
  from, so changing the path without reloading and then writing would have mixed jobs from two
  files. Switching tabs also destroyed every other pane, silently discarding open edits.

- `--advise`: analyses this job's run history and prints deterministic suggestions — a safe repeat
  interval derived from observed durations, what retaining N generations would cost, which
  `--threads` value has actually performed best, runs that stand out, and recurring integrity
  failures. Needs neither `--source` nor `--dest`, involves no language model and no network, and
  every suggestion shows the measurements behind it. It suggests and never applies: destructive
  operations stay with the operator.
- A run-history index: every completed run appends one NDJSON line to `.rustcopy_history.jsonl`,
  written **beside the report** (in the `--report-path` directory), never inside `--dest` —
  writing into the destination changes its mtime and perturbs the next robocopy transfer. Namespaced
  per job under `[[jobs]]`. A failure to write it never fails an otherwise successful backup.
- `rustcopy-flow` skill v1.1.0: two new molecules — *Diagnose* (answers questions about past runs)
  and *Notify* (sets up and troubleshoots `--webhook-url` / `notify-server`).

- `.github/workflows/security-audit.yml`: runs `rustsec/audit-check` against the RustSec advisory database on any push/PR to `main` touching `Cargo.toml`/`Cargo.lock`, plus a weekly cron so a disclosure against an already-merged dependency doesn't go unnoticed until the next bump.
- `docs/cli-reference.md` and `docs/installation.md`: the complete CLI flag table, exit codes,
  per-feature behaviour, installation requirements and notify-server setup, moved out of the
  README so each has a home of its own. Both tracked by the OKF documentation bundle.
- Project logo as a README header image (`images/rustcopy.jpg`).
- Added `LICENSE` (MIT), `SECURITY.md`, `.editorconfig`, `.github/workflows/ci.yml` (test on
  Windows + Linux, `cargo fmt --check`, `cargo clippy -D warnings`), `.github/dependabot.yml`.
- Filled in `Cargo.toml` metadata (`repository`, `homepage`, `keywords`, `categories`, `readme`,
  corrected `description`).
- Tagged the release history retroactively: `v0.2.0`, `v5.1.0`, `v5.4.0`, `v5.4.1`, `v5.4.2`,
  `v6.0.0`.
- `cargo fmt --all` applied across the whole tree (mechanical, no behavior change) so the new CI's
  `fmt --check` starts green.

### Changed
- README restructured as a landing page: 29.5 KB down to 7.1 KB. The CLI flag table alone had
  grown to 41.9% of the file, so a first-time visitor met `--keep-generations` before learning
  what the tool does. Reference material now lives in `docs/`, linked from a documentation index.
  No content was dropped, only relocated.
- CI pins `okf` to an exact version (0.2.2) instead of installing the latest. The docs gate
  compares generated indexes byte-for-byte, so an unpinned install let an upstream release fail an
  unrelated pull request — which is exactly what happened when 0.2.2 changed how `&` is escaped.
- **D10** reclassified from open defect to documented known limitation. Its actionable half was
  done (graph regenerated, Rust-node reachability 5.7% -> 80.5%, root cause re-diagnosed); what
  remains — LLM-based extraction not tracing indirect dispatch through `Box<dyn Trait>`, closures
  and intermediate variables — has no fix, so listing it as open implied work that cannot succeed.
  The standing prescription is unchanged: the graph is a navigation aid, never an anti-dead-code
  gate. No defects are open now.
- **Breaking (library API)**: `ScanSummary::files` is now `Arc<[ScannedFile]>` instead of
  `Vec<ScannedFile>` (D21). Read-only uses are unaffected — it derefs to `&[ScannedFile]` — but
  code that moved or mutated the `Vec`, or constructed a `ScanSummary` literal, needs updating
  (`.into()` on construction, `Arc::clone` to share, `.to_vec()` if an owned copy is genuinely
  wanted). Only the `robocopy_ingest` binaries consume this today; flagged here because the type
  is `pub` and the next release carrying it should be semver-major.

### Fixed
- Restoring a backup no longer copies rustcopy's own bookkeeping files into the restore target.
  `--restore-from` reverses source and destination, so a previous run's destination becomes the
  next run's source; `.ingest_cache` and `.rustcopy_generations.json` were being inventoried as if
  they were backed-up content, and a restore combined with `--decrypt` failed outright on them
  (`missing RCE1 header`) because they were never encrypted.
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

[Unreleased]: https://github.com/matrixNeo76/rustcopy/compare/v6.0.0...HEAD
[6.0.0]: https://github.com/matrixNeo76/rustcopy/compare/v5.4.2...v6.0.0
[5.4.2]: https://github.com/matrixNeo76/rustcopy/compare/v5.4.1...v5.4.2
[5.4.1]: https://github.com/matrixNeo76/rustcopy/compare/v5.4.0...v5.4.1
[5.4.0]: https://github.com/matrixNeo76/rustcopy/compare/v5.1.0...v5.4.0
[5.1.0]: https://github.com/matrixNeo76/rustcopy/compare/v0.2.0...v5.1.0
[0.2.0]: https://github.com/matrixNeo76/rustcopy/releases/tag/v0.2.0
