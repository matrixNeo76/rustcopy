# AGENTS.md — Developer & AI Agent Guidelines for robocopy-ingest-cli

Welcome to `robocopy-ingest-cli` (`rustcopy`). This document serves as the primary guidance file for **AI Agents (Antigravity, Claude, Codex, GPT-4)** and **Human Contributors** working on this codebase.

---

## 1. Project Overview & Architecture Principles

`robocopy-ingest-cli` is a high-performance Windows-native backup, ingestion, disaster recovery, and live-monitoring CLI written in Rust. It wraps `robocopy.exe` for real transfers while keeping the entire test suite, baseline naive engine, integrity verification, and parsing layer 100% cross-platform (runnable on Windows, Linux, and macOS).

### Core Architectural Rules:
1. **Zero-Allocation Stdout Streaming**: Always read `robocopy.exe` output using binary `read_until` byte buffers (`Vec<u8>`) to avoid heap allocations per copied file.
2. **Never Redirect Stderr to Unread Pipe**: Always direct `stderr` to `Stdio::null()` to prevent Windows pipe buffer deadlocks.
3. **Lossy OEM/ANSI Decoding**: Windows Robocopy outputs text in OEM code pages (e.g. CP850/CP437). Always decode stdout lines via `String::from_utf8_lossy`.
4. **Memory Bounds (Anti-OOM)**:
   - Logging channels MUST use bounded channels (`bounded_channel(10_000)`).
   - Report mismatch lists MUST be capped to `10_000` items (`MAX_REPORTED_ERRORS`).
5. **Path Normalization**: Strip trailing separators from arguments to prevent quote escaping bugs (`"C:\data\"` -> `"C:\data"`).

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
- All **120+ unit and integration tests** MUST pass before committing changes.
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
