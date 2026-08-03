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
- `--enable-dedup`, `--cloud-sync-target`, `--install-service`, `--fast-verify` are accepted for forward compatibility but are **not implemented** (see `[NOT IMPLEMENTED]` markers in `src/cli.rs`). Don't describe these as working features in docs. `--fast-verify` specifically needs `cache.rs` (see below) wired into production before it can hash only the files a run actually touched instead of the whole source inventory; that's tracked separately as ROADMAP.md F28.
- `--ignore-transient-missing` (F26a, closes half of D2) **is implemented**: `integrity::ignore_transient_missing()` runs after `--verify-integrity` and drops `missing_in_dest`/`unreadable` entries matching well-known transient patterns (`.log`, `.tmp`, anything under `.git/objects/`), recomputing `status`. Has no effect without `--verify-integrity`. Do not confuse this with `--fast-verify` above — they're unrelated despite both touching the verification path.
- `--exclude-junctions` (F26d, closes D7) maps to robocopy's `/XJ` in `engine/robocopy.rs::build_args`. Without it, robocopy follows junction points/symlinked directories (its own default) — and so does the prescan: `scan::scan`/`scan::inventory`/`scan::directory_size` all take an explicit `follow_links: bool` parameter, driven from `!args.exclude_junctions` (or `!request.exclude_junctions` in the naive engine) at every call site in `main.rs`. Keep these two in sync — the whole point of the flag was that the prescan and the actual transfer used to walk different trees. `WalkDir` rejects cycles when following links, so a self-referencing junction errors per-entry instead of recursing forever.
- `--serve-dashboard` and its backing `src/server.rs` mock were **removed** (Release 5.4.0) in favor of the `notify-server` binary. Do not reintroduce a `--serve-dashboard` flag.
- `notify-server` (`src/bin/notify_server.rs`, `src/notify_server.rs`, `src/notify_sink.rs`) receives `--webhook-url` POSTs and fans them out to configurable channels. It is a **separate, feature-gated binary** (`--features notify-server`): axum must never become a default dependency of the main `robocopy_ingest` binary. `src/notify_sink.rs` (the `NotificationSink` trait and channel impls) has no axum dependency and is always compiled/tested; only `src/notify_server.rs` (the axum `Router`) and the bin are feature-gated.
- `--encrypt-aes256 <KEY>` / `--decrypt <KEY>` perform real streaming AES-256-GCM (see `src/crypto.rs`, fixed F25a/F25b, closes D3/D4). Encryption/decryption happen in fixed **1 MiB chunks** (`CryptoManager::encrypt_stream`/`decrypt_stream`), each with a fresh random nonce and an explicit length prefix — peak memory is O(chunk size), not O(file size), so this no longer OOMs on large files. On-disk format: `RCE1` magic header + repeated `nonce(12) || len(4, LE u32) || ciphertext+tag` records. `CryptoManager::encrypt_file`/`decrypt_file` write to a sibling temp file and `rename` atomically only on success — never leaves a half-transformed file at the real path. Both flags are mutually exclusive (`Args::validate()` rejects both being set, checked *before* the restore-mode early return since `--decrypt` is meant to be used with `--restore-from`). Do not reintroduce whole-file `std::fs::read`/`std::fs::write` for either direction.
- `--restore-from` is **fixed and verified (F24, closes D1)**: `--source`/`--dest` are now `Option<PathBuf>` with `required_unless_present = "restore_from"` (the previous `PathBuf` + `default_value = ""` combination silently made clap treat the arg as unconditionally required — an empty-string default is treated as "no default" by clap, so `required_unless_present` never took effect). Access them via `Args::source()`/`Args::dest()` (`&Path`, panics if called when neither was supplied — an invariant clap enforces before `validate()`/these accessors ever run in a real invocation), never the raw `Option` fields directly outside `cli.rs`. Covered by a black-box test that runs the compiled binary (`tests/cli_smoke.rs::restore_from_runs_end_to_end_without_source_or_dest`), not just `build_restore_args()` in isolation — the latter is exactly what let the original bug ship undetected.
- `restore::build_restore_args` takes the **original, already-parsed `Args`** for the current invocation and clones it, overriding only the fields that must come from the backup report (source/dest reversed, pattern, threads, retries, retry_wait_seconds, verify_integrity). Do not rewrite it to build a fresh `Args` via `try_parse_from` again — that was the F25b regression: every other flag typed alongside `--restore-from` (notably `--decrypt`, but also a custom `--log-path`/`--report-path`/`--webhook-url`) got silently discarded because `main.rs` replaces `args` wholesale with this function's return value. Caught only by a black-box test that combined `--restore-from` with `--decrypt` end-to-end, not by the pre-existing unit test (which never passed any extra flag).
- `--fast-verify` and `--ignore-transient-missing` are declared in `src/cli.rs` and read by **nothing** (D2). Treat them like the other unimplemented flags.
- `check_mirror_safety` in `main.rs` is `async fn` (F26b, closes D5): the destination walk it does runs inside `tokio::task::spawn_blocking`, like every other blocking filesystem operation in that file (inventory, transfer, verify, crypto, the dest poller). Don't call it synchronously from `execute()` again — that's exactly the regression it fixed (froze the tokio executor, and `Ctrl+C` handling with it, for the whole scan on large trees).
- `report::SCHEMA_VERSION` is `2` (F26c, closes D6): bumped from `1` after a past release renamed `integrity::Mismatch`'s fields (`source_sha256`/`dest_sha256` → `kind`/`algorithm`/`source_digest`/`dest_digest`) without bumping it. Those four fields carry `#[serde(default)]` (with a dedicated `impl Default for MismatchKind`, documented as a deserialization fallback only) so a genuinely old-format report — which `restore::build_restore_args` parses in full — stays deserializable instead of failing `--restore-from` outright. `path` has no default; without it there's nothing to identify the entry by.
- See `ANALYSIS.md` Parte 3 for the full post-5.1.0 defect list (D1-D10 — D1-D2, D5-D7 now resolved; D8-D10 remain lower-priority opportunities) and `ROADMAP.md` milestones 5.2.0 (closed)/5.3.0/6.0.0 for what is planned.

---

## 3. Communication & Tone Guidelines

- Keep responses concise and direct.
- Code comments and commit messages must be in **English**.
- Documentation files (`README.md`, `ARCHITECTURE.md`, `ANALYSIS.md`, `ROADMAP.md`) are written in **Italian**, as requested by the project maintainers.
