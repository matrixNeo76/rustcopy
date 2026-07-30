# CLAUDE.md — Claude Code & AI Agent Prompt Instructions

This file contains specific context, conventions, and operational instructions for Claude Code and AI assistant agents working on `robocopy-ingest-cli` (`rustcopy`).

---

## 1. Quick Reference & Commands

- **Build Project**: `cargo build`
- **Build Release**: `cargo build --release`
- **Run Full Test Suite**: `cargo test`
- **Run Specific Test**: `cargo test <test_name>`
- **Check Compilation**: `cargo check`

---

## 2. Key Architecture Concepts for Claude

When editing or extending `robocopy-ingest-cli`, keep these design patterns in mind:

### Robocopy Process Execution:
- The actual execution goes through the `CommandRunner` trait (`src/engine/robocopy.rs`).
- `ProcessRunner` is used on Windows (`#[cfg(windows)]`), while `ScriptedRunner` is used in unit tests on all OSes.
- Never modify Robocopy flag building without updating the corresponding unit tests in `robocopy::tests`.

### Memory Safety & Anti-OOM Controls:
- **Logging**: Uses `tokio::sync::mpsc::channel(10_000)` with `try_send`. Never switch back to an `unbounded_channel`.
- **Integrity Report Capping**: `IntegrityCheck` caps error vectors at `MAX_REPORTED_ERRORS = 10_000`.

### CLI Argument Rules:
- All CLI flags defined in `src/cli.rs` must also support optional TOML config overrides via `src/config.rs` (`IngestConfig`).
- Paths should be normalized via `normalize_path_arg` to handle Windows backslashes correctly.

---

## 3. Communication & Tone Guidelines

- Keep responses concise and direct.
- Code comments and commit messages must be in **English**.
- Documentation files (`README.md`, `ARCHITECTURE.md`, `ANALYSIS.md`, `ROADMAP.md`) are written in **Italian**, as requested by the project maintainers.
