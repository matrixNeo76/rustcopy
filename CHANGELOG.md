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

### Fixed

- **`--install-schedule` now creates a task that actually runs.** It used `schtasks /SC` flags, whose Task Scheduler defaults do not start a task on battery, stop it when the charger is unplugged, kill it after 72 hours and lose a run missed while the PC was off. The task is now registered from XML with those settings fixed (a missed run is caught up as soon as possible, and a run does not overlap itself).
- **`--backup-type` now honours `--verify-integrity`.** It used to be silently ignored for generation backups. The copy is verified file by file against the source (only the files that generation copied), the report carries the integrity check, a failure exits with code 4, and a generation that did not verify is **not** recorded in the manifest, so the next incremental copies those files again instead of trusting them. The console warnings that said otherwise are gone.

### Added

- **`--restore-generation <ID|latest>` and `--list-generations <DIR>`**: rebuild the state of one generation of a `--backup-type` backup (the generation folders layered in the order the runs depended on each other, files already deleted from the source then left out), never overwriting or deleting anything at the target and refusing a target inside the backup. Until now getting a state back from an incremental/differential chain was done by hand.
- **Slint console, editor**: a "Verifica" button beside the source and destination folders says what is at that path (exists, how many files, how big) and only while the box still holds the checked text; every editor field now carries a sentence saying what it is for, also as its accessible description.
- **Slint console, gaps toward the Tauri console**: a page with one line per job of a multi-job configuration (route, kind of copy, settings that matter, outcome of that job's last run), "+ Nuovo job" in the editor (a single-job file refuses to become multi-job), and report problems paged 100 at a time with a CSV export of all of them.
- **Slint console, phase 5b parity**: open any configuration or report file, a queue of several jobs with its position, resume from a checkpoint, "Controlla prima" (files, size, free space, refusals before anything is copied), history of any report with an outcome filter and CSV export, credentials, folder pickers and inline warnings in the editor, recent folders and destinations, a help page with a guided example, restore preview.
- **Slint console, phase 6**: a safety level (Prudent by default, Standard, Expert) read only from the user's own settings, with every change logged; schedules prepared from the console and installed by the CLI; **Sposta** (copy, verify, then delete the originals after a confirmation, only files with an identical copy); single files as sources; pause and resume; eject a removable destination drive after a clean copy; an email channel for `notify-server` (`[smtp]`, behind the `smtp` feature, never in the default CLI).
- `README.md` carries the "Made with Slint" attribution badge.
- New documents: `CHECKLIST_ACCETTAZIONE_GUI.md` (gaps toward the Tauri console, comparison with TeraCopy and Cobian, side-by-side tests) and `CONTROLLI_MANUALI_GUI.md` (checks only a person or another machine can do).
- The Slint console shows a system notification when a copy ends while its window is not in front. The installer registers the identity (`rustcopy.console`) through the Start menu shortcut and a registry name; without it Windows simply shows nothing and the taskbar button still flashes.

### Changed

- The installer's console component now ships the Slint console (`rustcopy-ui.exe`, installed as `rustcopy-gui.exe`): no WebView2 or other runtime is needed any more. The installer smoke job builds it without Node. The Tauri console is no longer packaged.

## [7.8.1] - 2026-10-08

### Changed
- **Console: a clearer visual system on Job and Report** (F94). Larger text (14px body, 12px minimum),
  headline numbers in cards (files, size, duration, speed) on Report and a summary row (jobs, last run ok,
  to check, never run) on Job, taller table rows, a bigger sidebar. The other tabs keep their current look
  until the same rules are applied to them.
- Dependencies: `tauri` 2.12, `tauri-build` 2.7, the dialog and notification plugins, `tokio`, `toml`,
  `clap`, `dirs` 7, `xxhash-rust`, `thiserror`. Tried on a release build of the console (folder picker,
  copy, run attach, end-of-run notification) and on the full test suites before merging.

### Fixed
- **Explorer drag-and-drop no longer offers "Copia con RustCopy" for an unsafe drop** (a target inside
  a dragged folder, a whole drive, two dragged folders with the same name). Before, dropping a folder
  into one of its own subfolders started a copy that kept copying itself.
- **The console no longer imports the dynamic C runtime again** (F92). Updating `tauri-build` to 2.7 turned
  on a new default (`staticVCRuntime`) that linked the Visual C++ runtime statically but the Universal C
  runtime dynamically, so `rustcopy-gui.exe` started importing `api-ms-win-crt-*` DLLs. It is switched off
  in the console's Tauri configuration, and the installer test in CI now also runs when `Cargo.lock` or the
  console changes.
- The "Copia" tab clears a refusal message as soon as the folders or the destination change.
- CI: the dependency audit no longer fails on Dependabot pull requests (their read-only token cannot
  create the check run the audit action reports through).

## [7.8.0] - 2026-10-07

### Changed
- **Console: outcomes you can read at a glance** (F93). The Report tab opens with one sentence,
  "Backup riuscito: 150 file (2.0 GB) in 0.09s", "…con avvisi" with the reasons, or "Backup non
  riuscito", instead of a bare exit code; robocopy's own wording moves under "Dettagli tecnici".
  Job and History show a status chip ("Riuscito", or what the code means) instead of a number.
  Settings shows plain-language names (the TOML key stays beside it) and yes/no instead of
  true/false. After a quick folder sync, Run attaches to the copy by itself.

### Added
- **"Copia" tab** (F95): copy one or more folders into a destination without writing a configuration
  file. Pick the folders and the destination, optionally "Controlla prima" for file counts and size,
  then "Copia"; progress follows in the Esegui tab. It only copies (no mirror, no purge), refuses a
  destination inside its own source, a whole drive, and two folders with the same name.
- **Installer smoke test in CI** (F92): a workflow builds the real installer and installs, checks and
  uninstalls it on a Windows Server 2022 runner (files, Shell extension registration, install report,
  installed CLI, clean removal).

### Fixed
- **`--resume-from` now keeps the interrupted run's settings** (D25). A resumed run used to forget the
  bandwidth limit, the excluded files and folders, the age filters, the hash algorithm and more: a run
  throttled to 3 MB/s resumed at full speed, and an interrupted `--dry-run` resumed as a real copy. They
  are now restored, with one rule: a resume never does more than a fresh run would -- `--mirror`,
  retention, shell commands, webhook URLs and encryption keys are deliberately not restored.

## [7.7.0] - 2026-10-03

### Fixed
- **Installation on a clean Windows machine** (D30/F92): the installer failed on Windows Server 2016
  and 2022 (and would on any machine without the Visual C++ Redistributable). Every binary imported
  the dynamic C runtime, so the optional Explorer Shell extension could not even be loaded, its
  registration failed, and Setup rolled the **whole** install back (exit code 5). The binaries now
  link the C runtime statically (`+20 KB` CLI, `+123 KB` console, `+93 KB` Shell DLL), so the
  Redistributable is no longer a requirement, and registering the Shell extension can no longer
  take the rest of the install down with it: if it fails it is reported and skipped.

### Added
- **Automatic install report**: every install run, silent or not, completed or failed,
  writes `C:\ProgramData\rustcopy\install-reports\install-<date>.txt` plus a copy of Setup's own log
  beside it, with the operating system and build, privileges, command-line options, chosen
  components, runtime and WebView2 state, pending reboots, the exit code of the Shell registration
  and the final outcome. `/ReportDir=<folder>` redirects it. On failure Setup also says where the
  report is. No PowerShell is started to produce it.
- `scripts/collect-install-diagnostics.ps1` (read-only deep dive: event log, Defender, AppLocker,
  Code Integrity) and `scripts/check-static-crt.ps1` (fails if a binary imports the dynamic C
  runtime again; run by a new CI job).

## [7.6.1] - 2026-10-02

### Changed
- **Visible labels on icon-only controls** (F89, wave 3): the Job table's settings strip now reads
  "Cifrato", "N cicli", "N esclusioni", "N in parallelo"; the per-row actions read
  "Impostazioni", "Storico", "Modifica"; the Editor's reorder arrows are labelled "Ordine di
  esecuzione"; the path bar's star reads "Aggiungi ai preferiti" / "Nei preferiti". Tooltips are
  unchanged.

## [7.6.0] - 2026-10-02

### Changed
- **Plainer language in the console** (F89, waves 1-2): the file pickers, empty states and
  path fields no longer lead with file-format jargon ("TOML"/"JSON"); raw robocopy flags and
  developer identifiers (`/MT`, `/XJ`, `Args::validate()`, `keep_generations`) are gone from
  tooltips and visible text; a "VSS" entry was added to the Aiuto glossary; the two messages that
  pointed non-technical operators at actions they cannot perform now say so plainly.
- **Modifica groups its fields**: name/source/destination/file filter stay visible, while
  "Comportamento della copia" and "Opzioni avanzate" are collapsible sections that open by
  themselves whenever they hold a non-default setting, so a collapsed section never hides a
  customised value or the verify-with-generations warning.

### Fixed
- Bumped the transitive `devalue` dependency of the console's frontend (npm audit, high).

## [7.5.0] - 2026-09-21

### Fixed
- A backup destination on a network share (UNC path) longer than 240 characters with
  `--long-paths` produced an invalid long-path prefix (`\\?\` glued onto the original path
  instead of the real `\\?\UNC\server\share\...` convention), which Windows does not resolve —
  robocopy would fail loudly rather than silently lose data, but the run itself never worked.
  Full write-up: `ANALYSIS.md` D28.

### Added
- **Windows Server hardening for the installer** (F90): detects Server Core (no desktop shell at
  all) and hides the console/Shell-extension components entirely instead of offering something
  that could never run; warns — without blocking setup, same as the existing VC++/WebView2
  checks — when the detected Windows/Server version predates the Universal CRT requirement
  (Windows 10 1607+ / Server 2016+); warns when the Shell extension is selected on a detected
  Server SKU, since it loads into every signed-in user's Explorer process on a Remote Desktop
  Session Host, not just one desktop.
- **Production deployment guidance** (F90): a new section in `RUNBOOK.md` covers recommended
  antivirus/EDR exclusions for backup source/destination paths, checking VSS writer health before
  relying on `--vss-snapshot` against an application server, and partial mitigations for the
  still-open code-signing gap (F60).

## [7.4.1] - 2026-09-15

### Fixed
- **Critical**: the Explorer Shell extension (F85) could crash `explorer.exe` itself — taking
  down the desktop, taskbar and every open window at once, on some machines requiring a full
  reboot to recover — for any right-click or cross-drive drag onto any folder or drive, not just
  a drag between two rustcopy-managed folders. The handler read a Windows drag-and-drop data
  structure without first checking which of its union fields was actually valid; a non-standard
  drag source (a cloud-sync folder, another Shell extension, antivirus Shell hooks) could trigger
  undefined behavior inside Explorer's own process. Installations that never selected the Shell
  extension component, or never used the affected drag gesture, were not exposed. Full write-up:
  `ANALYSIS.md` D29.

## [7.4.0] - 2026-09-14

### Added
- **Operational status on the Job screen** (F86): each job now shows an "Ultima esecuzione"
  column — outcome, date and throughput from its own run history, honestly distinguishing "never
  run" from "no data available yet" (an unresolved `{timestamp}` report path) rather than guessing.
  A compact icon strip flags encryption, retention, exclusions and non-default thread counts at a
  glance. Three new per-row actions jump straight to that job's Impostazioni, Storico or Modifica,
  instead of retyping a path in another tab. A "pianificato" badge appears once per file when a
  Windows scheduled task references it — file-level, since a scheduled run always executes every
  job in the file, never a single one.

### Fixed
- The verification algorithm on the Report screen never translated to a friendly label ("SHA-256",
  "BLAKE3") — a casing mismatch between the lookup table and the real value meant it silently
  always fell back to the raw wire form ("sha256"); BLAKE3 was missing from the table outright.
- The Help screen's introduction still claimed the console never runs backups, copies, or deletes
  anything — false since the Esegui tab was added, and contradicted by the very next section of
  the same page.
- The Report screen could show a stale report if two loads overlapped (a manual open racing a
  cross-tab jump from Esegui) — the same out-of-order-response guard already added to every other
  pane that loads from more than one trigger.

## [7.3.0] - 2026-09-11

### Added
- **Richer Report screen** (F84): the exit-code icon is now derived server-side
  (`RobocopyStatus::is_success()`) instead of a naive "exit code 0" check, which is wrong for
  robocopy — a `1` alone is a genuine success. A new "File e byte" section surfaces robocopy's
  full summary (skipped/mismatch/failed/extra, both counts and bytes), previously parsed and then
  discarded down to just the copied total. "Configurazione usata" now shows every non-default
  setting that was active for the run, not only a handful. A real start timestamp
  (`started_at`) sits alongside the pre-existing finish time.
- **Explorer Shell extension** (F85, closes the long-backlogged F51): dragging one or more
  folders onto another with the right mouse button (or across drives) now offers "Copia con
  RustCopy" on Explorer's own drop-confirmation menu, alongside the native Copy/Move. Picking it
  hands the drop straight to the desktop console with the copy already queued and visible — not a
  silent background process. Network (UNC) destinations default to a conservative 8 threads
  instead of this machine's full logical-CPU count, a safety choice validated against a real cold
  NAS benchmark that found no throughput benefit past a handful of threads. Installed as an
  optional component of the existing setup (`gui\shell`, nested under the desktop console, which
  it requires to function) — no separate installer, no manual registry step.

### Fixed
- The Report screen's "File e byte" section showed "dettaglio non disponibile" on any
  Italian-locale Windows install, because the underlying parser only recognized the English
  `Files`/`Bytes` row labels robocopy prints — it now tries the Italian forms too.
- A truncated robocopy summary line (fewer than six columns) was previously accepted and
  zero-filled, showing a real count as if it were a confirmed zero; it is now rejected outright.
- The Report screen's file/byte counts could come from only one of robocopy's two summary rows
  (Files or Bytes) instead of requiring both, risking a real file count paired with silently
  zeroed bytes.

## [7.2.0] - 2026-09-09

### Added
- **Live "what's copying now"** in *Esegui*: the console shows the most recently completed
  file's name next to the progress bar while a sync is running, reusing the same robocopy line
  already parsed for byte/file counts — no new log volume, no new flag reaching the child
  process. The "Dettagli" output panel can also now be opened while a run is still in progress,
  not only after it finishes.

### Fixed
- The panel above stayed collapsed by default and, once opened during a run, snapped shut again
  on the next 1-second poll (a one-way Svelte binding re-applying itself); both are now respected
  as an explicit choice the operator makes, not something reset out from under them.
- `run_status`'s live-progress path no longer holds the console's run-state lock while reading
  files from disk — a slow or network destination could otherwise stall the "Ferma" button for as
  long as that read took.
- The current-file name preserved spaces correctly instead of only the trailing word when
  robocopy's own output used space-padded columns instead of tabs.

## [7.1.0] - 2026-09-08

### Added
- **Guided creation of a real job** from Job's empty state (F83): a new "Crea la tua
  configurazione" wizard — name, source/destination via native folder pickers, a
  Verifica integrità checkbox — writes a real, reusable TOML without starting it, filling a gap
  neither the existing fake-data example (F79) nor QuickSync (F71, fixed name, starts
  immediately) actually covered.
- **Path inspection** (F73): a "Verifica" button next to Sorgente/Destinazione in *Modifica*
  reports existence and file/folder counts on demand.
- **Job name validation** (F72): the editor rejects Windows-reserved characters, control
  characters, reserved device names (including legacy superscript forms), and — as of this
  release — a trailing `.`/space, before the name is ever written to disk.
- **Inline guidance throughout *Modifica*** (F74/F75/F77): suggestions for Pattern and Escludi
  file, the real per-machine default shown for Thread, and tooltips reusing *Aiuto*'s own text
  for `backup_type` and every checkbox.
- **Friendlier errors**: the single-job-to-multi-job split error (F78) now shows a concrete TOML
  example with the real job name instead of the raw English message; a Report placeholder shows
  the real default path (F76); *Storico* exposes the exit-code meaning through a single shared
  core function instead of a second hardcoded table (F81); the restore-preview button in *Report*
  now warns up front when no configuration has been loaded in the session, instead of failing
  silently on a relative-path report (F82).

### Fixed
- `History.svelte`'s CSV export could include the `"…"` placeholder instead of the real exit-code
  meaning if triggered in the brief window before every code's meaning had resolved.
- The Pattern field's own multi-extension suggestion (`*.jpg;*.png;*.gif`) was verified against
  real `robocopy.exe` and found to silently copy zero files — corrected to single-pattern
  suggestions only.
- Thread's placeholder read `navigator.hardwareConcurrency`, which Chromium can clamp for
  fingerprinting protection; now reads the same value the core would actually use.

## [7.0.0] - 2026-09-07

### Added
- The console can now **resume an interrupted run from a checkpoint**: the *Run* pane finds any
  `*.checkpoint.json` beside the loaded config and offers to continue it, going through the same
  fixed-argument builder (`runner::resume_arguments`) that already covers `run_arguments`, so the
  F61 prohibitions (`--force-purge`, `--mirror`, install/uninstall) apply identically. A resumed
  run only inherits pattern/threads/retries/verify-integrity from the interruption, not the rest
  of the original configuration (bandwidth limit, exclusions, hash algorithm, mirror included) —
  pre-existing behaviour of `checkpoint::build_resume_args`, documented for the first time here.
- The console shows a **coarse job queue** while a `[[jobs]]` batch runs (which job is waiting,
  running or done — never a guessed per-job outcome, which stays in Report/Storico), and can
  **save or delete a credential** in the Windows Credential Manager directly from *Impostazioni*
  (the same store `keyring:NAME` already reads) — the secret travels only over Tauri's IPC
  channel, never a process argument.
- **A visual rework of the console**, in three risk-ordered levels (`PIANO_GUI.md` §10): the
  layout now fills the window instead of hugging the top-left corner, tables use explicit column
  widths, a vertical sidebar with icons (`@lucide/svelte`) replaces the row of text buttons and
  frees the header to show the loaded config's filename, related content is grouped into cards,
  and the default window size grew from 1100×700 to 1440×900.
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
- **F62**: `--list-schedules` lists every Task Scheduler entry that invokes this binary (filtered on
  the binary's own path, not a specific config), reusing the CSV query/parsing already built for
  `schedule::referencing_config`. Exposed to the console as `gui_api::list_all_schedules`.
- **F63**: `--purge-preview-path <PATH>` (requires `--mirror`) writes the complete, untruncated list
  of files a mirror run would delete, without ever asking for confirmation or looking at
  `--force-purge` — a preview is a read, never an authorization. The retention (`--keep-generations`)
  half of this is deliberately not done yet.
- **F64**: the console's *Report* pane can preview a restore before it happens — it runs
  `--restore-from <report> --dry-run` against the real CLI in a throwaway report path and shows the
  swapped source/destination, file/byte counts and robocopy's own outcome, then deletes the scratch
  report. No new core logic: `--dry-run` already composed with `--restore-from`.
- **F65**: a preflight free-space check runs after the prescan and aborts with a new dedicated exit
  code (`6`) if the destination doesn't have enough room, plus a configurable safety margin
  (`--space-safety-margin-percent`, default 5%) and an opt-out (`--skip-space-check`) for
  destinations where free space can't be queried reliably (e.g. some network shares).
- **F66**: the console can save named favorites (job configs and reports) above the existing
  "Recent" list, entirely client-side — no new Tauri command, no new `JobConfig`/`Args` field.
- **F67**: the editor's job tabs gained move-up/move-down controls to reorder `[[jobs]]` before
  a batch runs (run order follows file order, fixed once a batch starts).
- **F68**: native folder pickers for the editor's *Sorgente*/*Destinazione* fields, replacing
  hand-typed paths — the console's last remaining path field without one.
- **F69**: the editor's `keep_generations` field is now editable to **raise** an existing value
  (never to introduce or lower one — the core already permitted raising it, the console just
  didn't expose it).
- **F70**: `backup_type` (full/incremental/differential) is now selectable in the editor,
  disabled whenever the job mirrors. Closed a real gap in the core along the way: `apply_draft`
  had no check at all for the `mirror`+`backup_type` combination.

### Changed
- The installer is now a **single** setup with the console as an optional component, rather than a
  second bundle produced by Tauri's bundler. Measured before deciding: the console is 8.9 MB
  against a 13.7 MB CLI install, because Tauri renders through the system WebView2 instead of
  shipping a browser engine. Two installers would have meant two version streams and two
  SmartScreen reputations for 8.9 MB. WebView2 is detected and reported — only when the console is
  selected — without blocking setup.

### Fixed
- **D24**: the console flashed a black console window every time it invoked `schtasks.exe`
  (scheduling checks, install/uninstall) — two spawns were missing `CREATE_NO_WINDOW`, the same
  flag the CLI's own child-process spawn already carried. Found in the same visual audit as the
  rework above, not a design choice.
- **D23**: `--bandwidth-limit-mbps` always fatal-errored against a real `robocopy.exe` (exit 16,
  zero files copied) — real robocopy refuses `/IPG` combined with `/MT` outright, and `build_args`
  pushed `/MT` unconditionally regardless of the bandwidth flag. `/MT` is now omitted entirely
  (robocopy's own single-threaded default) whenever a bandwidth limit is set.
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

[Unreleased]: https://github.com/matrixNeo76/rustcopy/compare/v7.0.0...HEAD
[7.0.0]: https://github.com/matrixNeo76/rustcopy/compare/v6.0.0...v7.0.0
[6.0.0]: https://github.com/matrixNeo76/rustcopy/compare/v5.4.2...v6.0.0
[5.4.2]: https://github.com/matrixNeo76/rustcopy/compare/v5.4.1...v5.4.2
[5.4.1]: https://github.com/matrixNeo76/rustcopy/compare/v5.4.0...v5.4.1
[5.4.0]: https://github.com/matrixNeo76/rustcopy/compare/v5.1.0...v5.4.0
[5.1.0]: https://github.com/matrixNeo76/rustcopy/compare/v0.2.0...v5.1.0
[0.2.0]: https://github.com/matrixNeo76/rustcopy/releases/tag/v0.2.0
