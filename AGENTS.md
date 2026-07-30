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
7. **Real vs. Mock Features**: `src/cache.rs`, `src/cloud.rs`, `src/service.rs` are not wired into the pipeline (`--enable-dedup`, `--cloud-sync-target`, `--install-service` are no-ops, marked `[NOT IMPLEMENTED]` in `--help`). `--serve-dashboard` only serves a static status page. Do not describe these as working in docs or fix requests without actually wiring them up first. `--encrypt-aes256` (`src/crypto.rs`) and the webhook (`src/notify.rs`) *are* fully implemented (AES-256-GCM; HTTPS via reqwest+rustls).

---

## 2. Directory Structure & Module Responsibilities

```text
src/
├── main.rs          # Application entrypoint & signal handling (Ctrl+C).
├── lib.rs           # Library root exporting public modules.
├── cli.rs           # Clap argument parsing, validation, and TOML merging.
├── config.rs        # TOML configuration file parser (IngestConfig).
├── scan.rs          # Pre-scan inventory walking & directory sizing.
├── integrity.rs     # Rayon parallel hashing (BLAKE3 & SHA-256) & OOM cap.
├── logging.rs       # Non-blocking bounded file logging subscriber.
├── report.rs        # JSON report generator with HostMetadata & PhaseTiming.
├── html_report.rs   # Standalone HTML5/SVG interactive dashboard generator.
├── notify.rs        # Async HTTP Webhook POST notification client.
├── server.rs        # Integrated Live Web Dashboard HTTP server.
├── restore.rs       # Disaster Recovery reverse restore engine.
├── cache.rs         # Incremental state cache (.ingest_cache) manager.
├── crypto.rs        # Zero-Trust AES-256 streaming encryption manager.
├── exit_code.rs     # Robocopy bitmask exit code decoder & status rules.
├── errors.rs        # IngestError enum & retry classification.
├── oem_codec.rs     # CP850 decode table + GetOEMCP() runtime check.
├── progress.rs      # Monotonic throughput progress bar.
├── testkit.rs       # ScriptedRunner & test doubles for cross-platform mocks.
└── engine/
    ├── mod.rs       # CopyEngine trait & CopyRequest / CopyOutcome definitions.
    ├── robocopy.rs  # Windows Robocopy process builder & parser.
    └── naive.rs     # Cross-platform baseline single-thread copy.
```

---

## 3. Mandatory Testing Guidelines

- **Never declare success without running `cargo test`**.
- All **140 unit and integration tests** MUST pass before committing changes.
- Cross-Platform Constraint: Unit tests inside `src/engine/robocopy.rs`, `src/integrity.rs`, `src/notify.rs`, etc. MUST pass on Linux and macOS using `ScriptedRunner`.

### Test Commands:
```bash
# Run full test suite
cargo test

# Run tests with backtrace on failure
RUST_BACKTRACE=1 cargo test
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
