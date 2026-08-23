---
type: Reference
title: AGENTS.md — Developer & AI Agent Guidelines
description: Architectural rules, directory tree, and testing conventions for this codebase.
status: stable
generated:
  by: process:claude-code
  at: 2026-08-06T00:00:00Z
---

# AGENTS.md — Developer & AI Agent Guidelines for robocopy-ingest-cli

Welcome to `robocopy-ingest-cli` (`rustcopy`). This document serves as the primary guidance file for **AI Agents (Antigravity, Claude, Codex, GPT-4)** and **Human Contributors** working on this codebase.

---

## 1. Project Overview & Architecture Principles

`robocopy-ingest-cli` is a high-performance Windows-native backup, ingestion, disaster recovery, and live-monitoring CLI written in Rust. It wraps `robocopy.exe` for real transfers while keeping the entire test suite, baseline naive engine, integrity verification, and parsing layer 100% cross-platform (runnable on Windows, Linux, and macOS).

### Core Architectural Rules:
1. **Zero-Allocation Stdout Streaming**: Always read `robocopy.exe` output using binary `read_until` byte buffers (`Vec<u8>`) to avoid heap allocations per copied file.
2. **Drain Both Pipes, Never Discard Stderr**: `stdout` and `stderr` are both `Stdio::piped()` and drained concurrently (stderr on its own `std::thread`, forwarded to `tracing::warn!`). Do not go back to `Stdio::null()` for stderr — that was the previous (wrong) fix for the pipe-buffer deadlock; draining both pipes concurrently is the correct fix.
3. **OEM/ANSI CP850 Decoding**: Windows Robocopy outputs text in OEM code pages (CP850 by default on Western European Windows installs). Decode via `crate::oem_codec::decode_robocopy_output` (`src/oem_codec.rs`), **not** `encoding_rs::Encoding::for_label(b"ibm850")` — `encoding_rs` does not implement single-byte DOS/OEM code pages and that call always returns `None`, silently falling back to UTF-8.
4. **Memory Bounds (Anti-OOM)**:
   - Logging channels MUST use bounded channels (`bounded_channel(10_000)`); dropped lines are counted and exposed via `LogHandle::dropped_lines()` / `IngestReport.log_lines_dropped`.
   - Report mismatch lists MUST be capped to `10_000` items (`MAX_REPORTED_ERRORS`).
5. **Path Normalization & Signal Handling**: Strip trailing separators from arguments. `Ctrl+C` terminates *only* the tracked child PID (published into an `Arc<AtomicU32>` by `ProcessRunner`), never every `robocopy.exe` process on the host (`taskkill /IM` by image name is banned — use `/PID`).
6. **Mirror Safety**: `--mirror` runs `check_mirror_safety` before the transfer, which diffs the destination against the source inventory and aborts (dedicated exit code 3) unless `--force-purge` is given or the run is interactive and confirmed. Never remove this check or make `--force-purge` the default.
7. **Real vs. Mock Features**: `src/cloud.rs` is not wired into the pipeline (`--cloud-sync-target` is a no-op, marked `[NOT IMPLEMENTED]` in `--help`). `src/cache.rs` is **partially wired**: `IngestCache` is used by `--fast-verify` (F28) to skip re-hashing files with unchanged source size+mtime, but `--enable-dedup` (transfer-level deduplication) remains a no-op. Do not describe unimplemented features as working in docs. `--encrypt-aes256` (`src/crypto.rs`) and the webhook (`src/notify.rs`) *are* fully implemented (AES-256-GCM; HTTPS via reqwest+rustls). `src/service.rs` (F37/F41) **is now a real, generic Windows Service Control Manager integration** via the `windows-service` crate, shared by both binaries — `robocopy_ingest --install-service`/`--uninstall-service` register/remove an idle service (F37); `notify-server --install-service`/`--uninstall-service` register/remove a **separate** service that actually hosts the axum server (F41, see rules 13-14). Don't describe either as a stub. `--serve-dashboard`/`src/server.rs` were removed (Release 5.4.0), replaced by the `notify-server` binary.
8. **`notify-server` stays feature-gated**: axum, and anything that depends on it, must live behind `#[cfg(feature = "notify-server")]` (`src/notify_server.rs`, `src/bin/notify_server.rs`). `src/notify_sink.rs` (the channel trait and implementations) has no axum dependency and must stay always-compiled and always-tested. Verify with `cargo tree | grep -i axum` (must be empty without the feature) before committing any change here.
9. **Backup Generations Use Naive Engine**: `src/generations.rs` uses `engine::naive::copy_selected` for selective file copies in incremental/differential backups, **not** `robocopy.exe`. Robocopy's file-selection arguments match filenames by pattern at every directory level during its scan — there is no way to hand it an explicit list of specific relative paths to copy. Do not attempt to route generation copies through `transfer()`/`RobocopyEngine`.
10. **Multi-Job Args Discipline**: In multi-job mode (`[[jobs]]`), `main.rs::run_jobs` reconstructs `Args` for each job from a fresh **clone of the original CLI invocation**, never from `try_parse_from` nor from a previous job's already-merged `Args`. This is the same discipline as `restore::build_restore_args` / `checkpoint::build_resume_args` (lesson F25b: rebuilding via `try_parse_from` silently drops every flag not explicitly re-specified). The same discipline applies to `schedule::strip_schedule_flags` (F36): it filters raw argv rather than reconstructing `Args`.
11. **Pre/Post Job Commands (F39)**: `--pre-command` failing (non-zero exit or unable to spawn) aborts the job **before anything is copied** (`IngestError::PreCommandFailed`) — proceeding would risk backing up inconsistent data (e.g. a database that didn't actually stop). `--post-command` failing does **not** fail an already-successful job — it's logged and recorded in `IngestReport.post_command_error`, mirroring the pre-existing `webhook_error` non-fatal pattern. Both run via `cmd /C` (Windows) / `sh -c` (elsewhere), in `src/hooks.rs`.
12. **Exit Codes**: `0` success, `1` transfer failed, `2` usage/unrecoverable error, `3` `--mirror` purge aborted, `4` `--verify-integrity` found a mismatch (transfer itself succeeded), `5` `--keep-generations` retention purge aborted (F35 — kept distinct from `3` so a scheduler can tell mirror-purge and retention-purge aborts apart).
13. **Service Dispatch Precedes the Tokio Runtime (F37/F41)**: both `robocopy_ingest`'s and `notify-server`'s `main()` are plain `fn`s, not `#[tokio::main]` — each checks raw `std::env::args()` for the internal `--run-as-service` marker (`service::is_service_launch()`) *before* building any tokio `Runtime`, because `windows_service::service_dispatcher::start` blocks the calling OS thread until SCM stops the service and must not run on a tokio worker thread. Never move that check after runtime construction, in either binary.
14. **Two Separate Windows Service Identities (F41)**: `robocopy_ingest` (`"RustcopyIngestService"`, idle, F37) and `notify-server` (`"RustcopyNotifyServer"`, hosts axum, F41) each register their **own** service — notify-server's real work was deliberately NOT bolted onto `robocopy_ingest`'s idle service, because that would make the default `robocopy_ingest` binary conditionally depend on axum, violating rule 8. `service.rs`'s SCM plumbing (`install_named`/`uninstall_named`/`start_dispatcher`/`register_and_wait_for_stop`) is generic/name-parameterized and shared by both; it has zero axum dependency either way. The axum-hosting service body lives entirely in `src/bin/notify_server.rs`.
15. **The documentation bundle is an OKF bundle, and `scripts/okf-docs.sh` is its single entry point**: that script owns the one definition of `TRACKED_DOCS` and implements every gate — no untracked root `.md`, `okf parse` per doc, `okf validate` + `okf lint` on a staged bundle, and that the committed indexes match `okf index` output. CI's `docs` job is one line calling `scripts/okf-docs.sh check`, so what CI runs is what a developer runs locally; **do not re-inline the doc list into `ci.yml` or a second script.** Adding a permanent doc: give it minimal OKF v0.2 frontmatter (`type`/`title`/`description`/`status`/`generated`, `verified` only where something actually checked the claim), append it to `TRACKED_DOCS`, run `scripts/okf-docs.sh index`, and commit the regenerated indexes. A one-off scratch file needs none of this. Three points that are easy to get wrong: (a) an `index.md` declares **only** `okf_version` (§12) — it is a bundle index, not a concept, so never give it `type`/`title`/`description`; (b) OKF indexes **per directory** and builds its link graph from **markdown body links**, not frontmatter fields — an index under `docs/` does not de-orphan documents in the repo root, and `dependencies:`/`related:` frontmatter keys are inert; (c) `generated.at`/`verified.at` are provenance stamps, not "last edited" — OKF's staleness model doesn't flag an old `generated.at` on a `stable` doc with no `ttl`, so don't bump it on routine edits, but **do** bump `verified.at` when a rewrite is substantial enough that the original attestation no longer covers the new content. This rule exists because three separate failures went undetected for days: `AGENT_HARNESS_PLAN.md` sat in root unfrontmattered for 10 days; `PIANO_MIGLIORAMENTI.md` kept `verified: 2026-08-17` after its B5 section was rewritten on 20 Agosto; and `ARCHITECTURE.md` — the one file carrying a CI-issued `verified` stamp for its cross-platform claim — still asserted macOS coverage that CI never had. Recording trust metadata without a gate that fails on it is decoration.

---

## 2. Directory Structure & Module Responsibilities

```text
src/
├── main.rs          # Application entrypoint & signal handling (Ctrl+C).
├── lib.rs           # Library root exporting public modules.
├── cli.rs           # Clap argument parsing, validation, and TOML merging.
├── config.rs        # TOML configuration file parser (IngestConfig + JobConfig).
├── scan.rs          # Pre-scan inventory walking & directory sizing.
├── integrity.rs     # Rayon parallel hashing (BLAKE3, SHA-256, xxHash3) & OOM cap.
├── logging.rs       # Non-blocking bounded file logging subscriber with rotation.
├── report.rs        # JSON report generator with HostMetadata & PhaseTiming.
├── html_report.rs   # Standalone HTML5/SVG interactive dashboard generator.
├── notify.rs        # Async HTTP Webhook POST notification client.
├── notify_sink.rs   # NotificationSink trait + LogSink/NtfySink/GenericWebhookSink (always compiled).
├── notify_server.rs # axum Router for the notify-server binary (feature "notify-server" only).
├── bin/
│   └── notify_server.rs  # notify-server binary entrypoint (feature "notify-server" only); own Windows service identity (F41).
├── restore.rs       # Disaster Recovery reverse restore engine.
├── checkpoint.rs    # Interrupted-run checkpoint save/resume (--resume-from).
├── generations.rs   # Backup generation manifest (.rustcopy_generations.json), diffing & retention (F35).
├── vss.rs           # Volume Shadow Copy snapshot creation/cleanup (vssadmin).
├── cache.rs         # Incremental state cache (.ingest_cache) — used by --fast-verify.
├── cloud.rs         # Cloud provider sync abstraction (stub, NOT IMPLEMENTED).
├── hooks.rs         # Pre/post job command execution (--pre-command/--post-command, F39).
├── schedule.rs      # Windows Task Scheduler integration (--install-schedule/--uninstall-schedule, F36).
├── service.rs       # Generic Windows Service SCM integration (F37/F41), shared by robocopy_ingest (idle) and notify-server (hosts axum).
├── crypto.rs        # Zero-Trust AES-256 streaming encryption manager.
├── exit_code.rs     # Robocopy bitmask exit code decoder & status rules.
├── errors.rs        # IngestError enum & retry classification.
├── oem_codec.rs     # CP850 decode table + GetOEMCP() runtime check.
├── progress.rs      # Monotonic throughput progress bar.
├── testkit.rs       # ScriptedRunner & test doubles for cross-platform mocks.
└── engine/
    ├── mod.rs       # CopyEngine trait & CopyRequest / CopyOutcome definitions.
    ├── robocopy.rs  # Windows Robocopy process builder & parser.
    └── naive.rs     # Cross-platform baseline single-thread copy + selective copy for generations.
```

---

## 3. Mandatory Testing Guidelines

- **Never declare success without running `cargo test`** (and `cargo test --features notify-server` if you touched `src/notify_server.rs`, `src/notify_sink.rs`, or `src/bin/notify_server.rs`).
- All **326 unit and integration tests** (default build) MUST pass before committing changes. With `--features notify-server`, **341** must pass.
- Cross-Platform Constraint: Unit tests inside `src/engine/robocopy.rs`, `src/integrity.rs`, `src/notify.rs`, `src/notify_sink.rs`, etc. MUST pass on Linux and macOS using `ScriptedRunner`/scripted test doubles.

### Test Commands:
```bash
# Run full test suite (default: no axum, no notify-server)
cargo test

# Run the notify-server test suite too (builds axum + both binaries)
cargo test --features notify-server

# Run tests with backtrace on failure
RUST_BACKTRACE=1 cargo test

# Verify axum never leaks into the default dependency tree
cargo tree | grep -i axum   # must print nothing
```

---

## 4. Coding & Style Conventions

- **Language Standard**: Rust 2021 edition.
- **Error Handling**: Use `anyhow::Result` for application-level error handling in `main.rs`, and custom `IngestError` in core library modules.
- **Doc Comments**: Maintain standard Rustdoc comments (`///`) on all public functions, traits, and structs in English.

---

## 5. Tooling & Optimization Workflow (RTK & Graphify)

All AI agents and contributors working on this repository MUST utilize the installed tooling to optimize token consumption and maintain code graph clarity:

### RTK (Token Optimization Proxy):
- Use `rtk cargo test` instead of raw `cargo test` to compress test logs and save tokens.
- Use `rtk smart <file>` to obtain a 2-line technical summary of module impact.
- Use `rtk cargo check` or `rtk git status` to keep context clean.

### Graphify (Code Graph & AST):
- The canonical graph is `graphify-out/graph.json` at the **repo root** — it covers the whole repo (code + docs + skills, not just `src/`), rebuilt with `/graphify . --mode deep` (last done 21 August 2026, see `ANALYSIS.md` D10). Use `/graphify --update` to refresh it incrementally after code changes, **not** `graphify extract src/ ...` — that writes a second, divergent graph under `src/graphify-out/`. `.gitignore` now excludes any nested `graphify-out/` (`**/graphify-out/` with a root-only negation) so a stray one can't be accidentally committed, but it would still silently drift from the real one — the fix is not running that command, not just hiding its output from git.
- Use `graphify query "<question>" --graph graphify-out/graph.json` to inspect cross-module dependencies and call graphs.
- **Never use the graph as an anti-dead-code gate.** Reachability from `main()` sits at ~80% of Rust nodes, and the ~110 unreachable ones that are real production code (`atomic_write`, `ProcessRunner`, `ChannelWriter`, `LogHandle`, …) are all genuinely called — LLM-based extraction does not reliably trace indirect dispatch (`Box<dyn Trait>`, closures handed to `spawn_blocking`, methods reached through intermediate variables), which in Rust is everywhere. Find dead code with `grep`/`clippy`; that is how D8 was actually found. This is a known limitation of the tool, not an open defect — see `ANALYSIS.md` D10 for why it was reclassified rather than scheduled.

**Note**: `rtk` is installed on this machine with the hook active (`~/.claude/settings.json` runs `rtk hook claude`) — commands typed directly (not just explicit `rtk <cmd>` invocations) are auto-rewritten. Verify with `rtk gain`; it should report accumulated savings, not a "no hook installed" warning.

### Agent Skills (`.agents/skills/`)
- `rustcopy-flow/` — compound skill (2-level: orchestrator + molecules) that lets any coding CLI (Claude Code, OpenCode, etc.) drive `robocopy_ingest.exe` end-to-end: quick copy/mirror, generation backups + retention, restore, and Task Scheduler/service automation. Zero MCP dependency — pure Bash/PowerShell against the compiled binary, so it works outside this repo too (mirrored to `~/.claude/skills/rustcopy-flow/` for global use). See `rustcopy-flow/SKILL.md` for the scenario routing table and `rustcopy-flow/molecules/` for the per-phase steps.
- Other skills (`clean-architecture`, `clean-code`, `code-review-excellence`, `rust-async-patterns`, `rust-mcp-server-generator`, `windows-server-backup`) are general-purpose, not rustcopy-specific.
