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
7. **Real vs. Mock Features**: `src/cloud.rs` and `src/service.rs` are not wired into the pipeline (`--cloud-sync-target`, `--install-service` are no-ops, marked `[NOT IMPLEMENTED]` in `--help`). `src/cache.rs` is **partially wired**: `IngestCache` is used by `--fast-verify` (F28) to skip re-hashing files with unchanged source size+mtime, but `--enable-dedup` (transfer-level deduplication) remains a no-op. Do not describe unimplemented features as working in docs. `--encrypt-aes256` (`src/crypto.rs`) and the webhook (`src/notify.rs`) *are* fully implemented (AES-256-GCM; HTTPS via reqwest+rustls). `--serve-dashboard`/`src/server.rs` were removed (Release 5.4.0), replaced by the `notify-server` binary.
8. **`notify-server` stays feature-gated**: axum, and anything that depends on it, must live behind `#[cfg(feature = "notify-server")]` (`src/notify_server.rs`, `src/bin/notify_server.rs`). `src/notify_sink.rs` (the channel trait and implementations) has no axum dependency and must stay always-compiled and always-tested. Verify with `cargo tree | grep -i axum` (must be empty without the feature) before committing any change here.
9. **Backup Generations Use Naive Engine**: `src/generations.rs` uses `engine::naive::copy_selected` for selective file copies in incremental backups, **not** `robocopy.exe`. Robocopy's file-selection arguments match filenames by pattern at every directory level during its scan — there is no way to hand it an explicit list of specific relative paths to copy. Do not attempt to route incremental generation copies through `transfer()`/`RobocopyEngine`.
10. **Multi-Job Args Discipline**: In multi-job mode (`[[jobs]]`), `main.rs::run_jobs` reconstructs `Args` for each job from a fresh **clone of the original CLI invocation**, never from `try_parse_from` nor from a previous job's already-merged `Args`. This is the same discipline as `restore::build_restore_args` / `checkpoint::build_resume_args` (lesson F25b: rebuilding via `try_parse_from` silently drops every flag not explicitly re-specified).

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
│   └── notify_server.rs  # Thin notify-server binary entrypoint (feature "notify-server" only).
├── restore.rs       # Disaster Recovery reverse restore engine.
├── checkpoint.rs    # Interrupted-run checkpoint save/resume (--resume-from).
├── generations.rs   # Backup generation manifest (.rustcopy_generations.json) & diffing.
├── vss.rs           # Volume Shadow Copy snapshot creation/cleanup (vssadmin).
├── cache.rs         # Incremental state cache (.ingest_cache) — used by --fast-verify.
├── cloud.rs         # Cloud provider sync abstraction (stub, NOT IMPLEMENTED).
├── service.rs       # Windows Service SCM integration (stub, NOT IMPLEMENTED).
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
- All **223 unit and integration tests** (default build) MUST pass before committing changes. With `--features notify-server`, **236** must pass.
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
- Use `graphify extract src/ --no-viz --no-cluster` to update the AST graph when refactoring modules.
- Use `graphify query "<question>" --graph src/graphify-out/graph.json` to inspect cross-module dependencies and call graphs.
