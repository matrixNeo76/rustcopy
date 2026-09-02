---
type: Reference
title: Security Policy
description: How to report vulnerabilities, and the security scope of this project.
status: stable
generated:
  by: process:claude-code
  at: 2026-08-06T00:00:00Z
---

# Security Policy

## Supported Versions

Only the latest release on `main` (currently `6.0.0`) receives security fixes. There is no
long-term support for older versions.

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for a security vulnerability. Instead, report it
privately by email to **adlibrosfi@gmail.com**, including:

- A description of the vulnerability and its potential impact.
- Steps to reproduce (a minimal repro is very helpful).
- The version/commit affected, if known.

Expect an initial response within a few days. There is no bug-bounty program; this is a small,
single-maintainer project.

## Scope

Security-relevant areas of this codebase:

- **`src/crypto.rs`** — AES-256-GCM streaming encryption/decryption (`--encrypt-aes256`/`--decrypt`).
  Each 1 MiB chunk uses a fresh random nonce; the on-disk format is documented in the module. A
  vulnerability here would mean encrypted backups are not actually confidential/authenticated.
- **`src/notify_sink.rs`/`src/notify_server.rs`** (the `notify-server` binary, feature-gated) —
  the `/notify` HTTP endpoint accepts an optional bearer token (`ROBOCOPY_NOTIFY_TOKEN`); with no
  token configured, the endpoint has no authentication at all (documented, not a bug) —
  `check_bind_security` refuses to bind to a non-loopback address without a token configured, to
  avoid an unauthenticated webhook receiver being exposed on the network by accident.
- **`src/hooks.rs`** (`--pre-command`/`--post-command`) and **`src/schedule.rs`**/**`src/service.rs`**
  (Task Scheduler / Windows Service integration) — these shell out to `cmd`/`schtasks.exe`/the
  Windows Service Control Manager using operator-supplied strings verbatim, with no
  escaping/sandboxing. This is a deliberate trust boundary: these flags are meant to be set by the
  operator running the backup, not by untrusted input, and are documented as such in `CLAUDE.md`.
  A report that this trust boundary is being crossed in an unexpected way (e.g. by data that
  originates from the *source* tree being copied rather than from CLI/config) is in scope.

- **`crates/rustcopy-gui`** (the desktop console) and **`src/gui_api.rs`**/**`src/job_editor.rs`**
  — the console reads reports and configurations and has exactly one write path,
  `job_editor::propose_config`, which writes a **new** file and refuses to overwrite. Its Tauri
  capabilities grant `dialog:allow-open`/`dialog:allow-save` and nothing else: the frontend has no
  filesystem permission of its own, and every read goes through a command backed by `gui_api`. A
  report that the frontend can reach a file the commands do not expose, that the editor can write
  outside the path the operator chose, or that a configured `webhook_url` reaches the interface
  un-truncated (it is cut to scheme and host, because the URL *is* the credential) is in scope.
  Note that roles are **not** planned as a security boundary here: anyone with a local session can
  run `robocopy_ingest.exe` or edit the TOML directly, and `ROADMAP.md` says so.

Out of scope: `robocopy.exe` itself (a Microsoft-owned binary this tool shells out to, not part of
this codebase), and denial-of-service reports that require local Administrator/physical access to
the host running the backup.
