# CLAUDE.md — Claude Code & AI Agent Prompt Instructions

This file contains specific context, conventions, and operational instructions for Claude Code and AI assistant agents working on `robocopy-ingest-cli` (`rustcopy`).

---

## 1. Quick Reference & Commands

- **Build Project**: `rtk cargo build` or `cargo build`
- **Build Release**: `rtk cargo build --release` or `cargo build --release`
- **Run Full Test Suite**: `rtk cargo test` (ALWAYS prefer `rtk cargo test` for token optimization)
- **Run Specific Test**: `rtk cargo test <test_name>`
- **Check Compilation**: `rtk cargo check`
- **Summarize Module Impact**: `rtk smart <file>`
- **Query Code Graph**: `graphify query "<question>" --graph src/graphify-out/graph.json`

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
- All CLI flags defined in `src/cli.rs` must also support optional TOML config overrides via `src/config.rs` (`IngestConfig`). `merge_config` only overwrites a field when the CLI still holds clap's own default for it (see the comment in `Args::merge_config`); it cannot yet distinguish "explicit CLI value equal to the default" from "no CLI value at all" (would need `ArgMatches::value_source`).
- Paths should be normalized via `normalize_path_arg` to handle Windows backslashes correctly.
- Default pattern is `*` for full recursive copy. `--mirror` mode runs `check_mirror_safety` in `main.rs`, which diffs the destination tree against the source inventory and aborts (dedicated exit code 3) unless `--force-purge` is given or the run is interactive and the operator confirms. This only works when a full prescan was taken; `--no-prescan` + `--mirror` always requires `--force-purge`.
- Stdout/stderr decoding uses `src/oem_codec.rs` (a hardcoded CP850 table plus a `GetOEMCP()` runtime check), **not** `encoding_rs::Encoding::for_label(b"ibm850")` — `encoding_rs` does not implement single-byte DOS/OEM code pages, so `for_label(b"ibm850")` always returns `None` and silently falls back to UTF-8. Do not reintroduce that pattern.
- `--enable-dedup`, `--cloud-sync-target`, `--install-service` are accepted for forward compatibility but are **not implemented** (see `[NOT IMPLEMENTED]` markers in `src/cli.rs`). Don't describe these as working features in docs.
- `--serve-dashboard` and its backing `src/server.rs` mock were **removed** (Release 5.4.0) in favor of the `notify-server` binary. Do not reintroduce a `--serve-dashboard` flag.
- `notify-server` (`src/bin/notify_server.rs`, `src/notify_server.rs`, `src/notify_sink.rs`) receives `--webhook-url` POSTs and fans them out to configurable channels. It is a **separate, feature-gated binary** (`--features notify-server`): axum must never become a default dependency of the main `robocopy_ingest` binary. `src/notify_sink.rs` (the `NotificationSink` trait and channel impls) has no axum dependency and is always compiled/tested; only `src/notify_server.rs` (the axum `Router`) and the bin are feature-gated.
- `--encrypt-aes256` performs real AES-256-GCM (see `src/crypto.rs`), applied to destination files after the transfer (and after integrity verification, so verification still compares plaintext). It is not a no-op. **But it is not production-ready**: it reads each file wholly into RAM (OOMs on large files, D3) and there is no decrypt path anywhere in the CLI, so encrypted backups cannot be restored by this tool (D4). Do not recommend it to users until F25a/F25b land.
- `--restore-from` is **currently unreachable**: `--source`/`--dest` carry `default_value = ""`, and clap rejects an empty `PathBuf` value before it ever evaluates `required_unless_present`, so every invocation fails with `a value is required for '--source <PATH>'`. Do not document restore mode as working (D1/F24). Note the existing restore test calls `build_restore_args()` directly and therefore does not catch this — any fix needs a black-box test that runs the compiled binary.
- `--fast-verify` and `--ignore-transient-missing` are declared in `src/cli.rs` and read by **nothing** (D2). Treat them like the other unimplemented flags.
- See `ANALYSIS.md` Parte 3 for the full post-5.1.0 defect list (D1-D10) and `ROADMAP.md` milestones 5.2.0/5.3.0/6.0.0 for what is planned.

---

## 3. Communication & Tone Guidelines

- Keep responses concise and direct.
- Code comments and commit messages must be in **English**.
- Documentation files (`README.md`, `ARCHITECTURE.md`, `ANALYSIS.md`, `ROADMAP.md`) are written in **Italian**, as requested by the project maintainers.
