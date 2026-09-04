//! Read-only, serializable views for a user interface (Passo 3 of `docs/archive/PIANO_GUI_TAURI.md`, F53).
//!
//! This module exists so the Tauri commands of `crates/rustcopy-gui` can be **thin wrappers**: a
//! `#[tauri::command]` should call one function here and hand back what it returns, nothing more.
//! That is `docs/archive/PIANO_GUI_TAURI.md` §4.1 made mechanical rather than aspirational — if the judgement
//! lives in Rust and is tested here, the frontend has nothing left to decide.
//!
//! It is deliberately **stack-agnostic**: nothing here knows about Tauri, and it compiles and is
//! tested today, before the frontend framework has been chosen. Whatever that choice turns out to
//! be, this surface does not change.
//!
//! # Two rules this module enforces, not merely documents
//!
//! **1. No per-file data crosses the IPC boundary unbounded.** `ScanSummary` is already safe by
//! construction — it holds `Arc<[ScannedFile]>` and does not derive `Serialize`, so a 1.34M-file
//! inventory *cannot* be sent (D21). `IntegrityCheck` is not: its `mismatches`, `missing_in_dest`
//! and `unreadable` lists are per-file and capped only at
//! [`crate::integrity::MAX_REPORTED_ERRORS`] (10 000 each), so a single report could carry 30 000
//! paths in one message. [`ReportView`] therefore returns a **page** plus the true total, never the
//! whole list.
//!
//! **2. Read-only.** Nothing *here* writes, deletes, schedules or installs. The prohibitions kept
//! in ROADMAP F61 and applied to `--advise` hold identically for a GUI: it may show and propose,
//! never act. The one write path the application has lives in [`crate::job_editor`] (F54), kept in
//! its own module precisely so this boundary stays visible in the file tree rather than dissolving
//! into a surface that both reads and writes.
//!
//! # What is intentionally not here
//!
//! Live progress. `ThroughputProgress` already exposes pollable counters
//! (`current_bytes`/`files`/`average_mbps`) behind lock-free atomics, and §2.3 requires the UI to
//! **sample** them on its own timer rather than be notified per file. Wrapping that in a view here
//! would invite a per-event API, which is precisely the shape D18 showed to be ruinous at this
//! scale (~3 800 files/second on the real profile).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::checkpoint::Checkpoint;
use crate::config::{IngestConfig, JobConfig};
use crate::errors::IngestError;
use crate::history::{RunHistory, RunRecord, DEFAULT_HISTORY_WINDOW};
use crate::integrity::HashAlgorithm;
use crate::report::IngestReport;

/// How many per-file error entries [`ReportView`] returns in one page by default.
///
/// Small on purpose: this is what a table shows at once, and the total is always reported
/// alongside so the UI can say "12 of 10 000" without receiving 10 000.
pub const DEFAULT_ERROR_PAGE: usize = 100;

/// Hard ceiling on a page, whatever the caller asks for.
///
/// Making `limit` a parameter was necessary — without it the paging here was unreachable — but it
/// handed the boundary rule to the caller: `read_report_page(path, 0, usize::MAX)` would return
/// every stored path from all three lists, up to 30 000 strings in one IPC response, which is
/// precisely what this module exists to prevent. The limit is clamped instead of rejected: a UI
/// asking for more than this is not doing anything wrong, it simply cannot have it in one message.
pub const MAX_ERROR_PAGE: usize = 1_000;

/// Hard ceiling on how many runs one history call returns.
///
/// Same reasoning as [`MAX_ERROR_PAGE`], applied before the mistake rather than after it: the
/// caller picks a window and the library decides what it can actually have in one message. A
/// `RunRecord` is a few hundred bytes, so this is a generous bound and still a bound.
pub const MAX_HISTORY_PAGE: usize = 500;

/// One backup job as configured, for a list view. Carries no run state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSummary {
    /// The job's own name, or the `jobN` fallback `run_jobs` would assign. Never inherited from
    /// the shared defaults — that inheritance was a real defect (two unnamed jobs shared one
    /// report, cache and manifest), fixed in `JobConfig::merged_over`.
    pub name: String,
    pub source: Option<String>,
    pub dest: Option<String>,
    pub backup_type: Option<String>,
    /// True when the job would run `--mirror`, which purges at the destination. A UI must surface
    /// this differently from an ordinary copy; it is the single most destructive setting a job
    /// can carry.
    pub mirror: bool,
    pub verify_integrity: bool,
    /// Shown **beside** `verify_integrity`, never instead of it. `--fast-verify` skips files whose
    /// source size and mtime are unchanged since the last clean run, so it trusts the source's
    /// identity rather than re-reading the destination's bytes: independent corruption at the
    /// destination is not caught on a run that skips that file. A UI rendering "verified: yes"
    /// without this overstates the guarantee.
    pub fast_verify: bool,
    /// True when `source` or `dest` still holds a template placeholder like `<PERCORSO_SORGENTE>`.
    ///
    /// Most of `examples/` is written to be **read and adapted**, not run, and a list that renders
    /// `<PERCORSO_SORGENTE_1>` in the source column shows a job that looks configured and is not.
    /// Deciding that is a judgement about the configuration, so it is made here rather than by a
    /// frontend guessing at angle brackets.
    pub unconfigured: bool,
    /// The report this job would write, resolved and — for a job that came from `[[jobs]]` and
    /// never set its own `report_path` — namespaced exactly as `run_jobs` namespaces it (F33/D12),
    /// via the same `namespaced_path` `main.rs` calls. `None` when the path still carries `{timestamp}`
    /// (P1): that placeholder is resolved fresh at the *start* of each run, so nothing computed
    /// ahead of time (or after the fact, from this list) can predict what a specific past run
    /// actually wrote. Used by the console's Esegui tab to link a just-finished run to its own
    /// report (Livello 1, punto 5, `PIANO_GUI.md` §10) — deliberately not attempted for a
    /// `{timestamp}` config rather than guessed at and wrong.
    pub report_path: Option<String>,
}

/// Same default `--report-path` clap gives `Args` (`cli.rs`), applied here because `JobSummary`
/// is built from `JobConfig`/`IngestConfig`, which — unlike `Args` — has no default of its own:
/// `report_path` stays `None` in the TOML until something resolves it, exactly the gap clap's own
/// `default_value` fills for a real invocation.
const DEFAULT_REPORT_PATH: &str = "./robocopy_ingest_report.json";

/// The report path a `JobSummary` should show, resolved and — when `namespace_with` is given —
/// namespaced via [`crate::namespaced_path`] exactly as `run_jobs` namespaces it. `None` when the
/// result still carries [`crate::REPORT_PATH_TIMESTAMP_PLACEHOLDER`]: that placeholder is resolved
/// fresh at the start of each run, so nothing computed here can predict what a specific past run
/// actually wrote.
fn report_path_for_summary(
    resolved: Option<&Path>,
    namespace_with: Option<&str>,
    anchor: &Path,
) -> Option<String> {
    let mut path = resolved
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REPORT_PATH));
    if let Some(name) = namespace_with {
        path = crate::namespaced_path(&path, name);
    }
    if path
        .to_string_lossy()
        .contains(crate::REPORT_PATH_TIMESTAMP_PLACEHOLDER)
    {
        return None;
    }
    // A relative path in the TOML means relative to the config file, not to whatever directory
    // the console process happens to have as its own working directory (Desktop, for a Start Menu
    // shortcut) — the same convention `start_job` already applies by setting `current_dir` before
    // spawning. Without this, "Apri il report di questa run" resolved a relative report_path
    // against the console's own cwd instead of the config's, and failed with a plain "path not
    // found" — found by clicking the button against a real run, not by reading the code.
    if path.is_relative() {
        path = anchor.join(path);
    }
    Some(path.display().to_string())
}

/// Whether a path is still a template placeholder rather than a real path.
///
/// Deliberately narrow: `<…>` wrapping the whole value. Windows forbids `<` and `>` in paths, so a
/// value shaped this way cannot be a path anyone meant to use, and the check cannot fire on a real
/// one.
fn is_placeholder(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .is_some_and(|text| text.starts_with('<') && text.ends_with('>') && text.len() > 2)
}

/// A page of per-file error paths, plus the total the page was taken from.
///
/// `total` is the count actually present in the report, which is itself capped at
/// [`crate::integrity::MAX_REPORTED_ERRORS`]; `truncated_at_source` says whether that cap was hit,
/// so a UI can distinguish "10 000 errors" from "at least 10 000 errors".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorPage {
    pub entries: Vec<String>,
    pub total: usize,
    pub offset: usize,
    pub truncated_at_source: bool,
}

impl ErrorPage {
    fn of(all: &[String], offset: usize, limit: usize, truncated_at_source: bool) -> Self {
        let entries = all
            .iter()
            .skip(offset)
            .take(limit.min(MAX_ERROR_PAGE))
            .cloned()
            .collect::<Vec<_>>();
        Self {
            entries,
            total: all.len(),
            offset,
            truncated_at_source,
        }
    }
}

/// Everything a UI needs to render one run, with the per-file lists paged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportView {
    pub timestamp: String,
    pub source: String,
    pub dest: String,
    pub total_files: usize,
    pub total_bytes: u64,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub elapsed_seconds: f64,
    pub throughput_mbps: f64,
    pub exit_code_meaning: Option<String>,
    pub integrity_status: Option<String>,
    /// Count only. The paths themselves come from [`Self::mismatches`] and the other pages.
    pub integrity_error_count: usize,
    pub mismatches: ErrorPage,
    pub missing_in_dest: ErrorPage,
    pub unreadable: ErrorPage,
    pub encrypted: bool,
    pub webhook_error: Option<String>,
    pub post_command_error: Option<String>,
    pub copy_error: Option<String>,
}

impl ReportView {
    /// Builds a view from a report, taking `limit` entries from each error list starting at
    /// `offset`.
    ///
    /// Paging is not an optimisation here, it is the boundary rule: a report may hold 10 000
    /// entries in each of three lists, and sending 30 000 strings to a WebView in one message is
    /// the IPC-shaped version of the mistake D18 made with logging.
    pub fn from_report(report: &IngestReport, offset: usize, limit: usize) -> Self {
        let integrity = report.integrity_check.as_ref();
        let empty: Vec<String> = Vec::new();
        let truncated = integrity.map(|c| c.truncated).unwrap_or(false);

        let mismatch_paths: Vec<String> = integrity
            .map(|c| c.mismatches.iter().map(|m| m.path.clone()).collect())
            .unwrap_or_default();

        Self {
            timestamp: report.timestamp.to_rfc3339(),
            source: report.source.clone(),
            dest: report.dest.clone(),
            total_files: report.total_files,
            total_bytes: report.total_bytes,
            files_copied: report.robocopy_transfer.files_copied,
            bytes_copied: report.robocopy_transfer.bytes_copied,
            elapsed_seconds: report.phase_timing.total_seconds,
            throughput_mbps: report.robocopy_transfer.throughput_mbps,
            exit_code_meaning: report.robocopy_transfer.exit_code_meaning.clone(),
            integrity_status: integrity.map(|c| format!("{:?}", c.status)),
            integrity_error_count: integrity.map(|c| c.total_errors).unwrap_or(0),
            mismatches: ErrorPage::of(&mismatch_paths, offset, limit, truncated),
            missing_in_dest: ErrorPage::of(
                integrity.map(|c| &c.missing_in_dest).unwrap_or(&empty),
                offset,
                limit,
                truncated,
            ),
            unreadable: ErrorPage::of(
                integrity.map(|c| &c.unreadable).unwrap_or(&empty),
                offset,
                limit,
                truncated,
            ),
            encrypted: report.encrypted,
            webhook_error: report.webhook_error.clone(),
            post_command_error: report.post_command_error.clone(),
            copy_error: report.copy_error.clone(),
        }
    }
}

/// A window of completed runs, newest last, with what the reader could not parse.
///
/// Holds [`RunRecord`]s, which are per-run aggregates — never per-file data. That is what makes
/// this safe to send whole: the inventory is not reachable from here, and the integrity error
/// lists live in [`ReportView`], where they are paged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryView {
    pub runs: Vec<RunRecord>,
    /// Lines the index reader could not parse. Surfaced rather than hidden so a UI can say the
    /// sample is incomplete instead of quietly showing less than the operator believes.
    pub skipped_lines: usize,
    /// The ceiling actually applied, so a UI can tell "these are all of them" from "these are the
    /// most recent 500".
    pub limit_applied: usize,
}

/// Reads the run history that lives beside `report_path`.
///
/// `job_name` selects the per-job index (F33/D12 namespace it); `None` is the single-job file.
pub fn read_history(
    report_path: &Path,
    job_name: Option<&str>,
    limit: usize,
) -> Result<HistoryView, IngestError> {
    // `clamp`, not `min`: `RunHistory::load_recent` treats a limit of zero as **unbounded** — a
    // documented behaviour with a test of its own — so capping only the upper end left a caller
    // able to pass 0 and receive the entire index. The ceiling needed a floor.
    let limit = limit.clamp(1, MAX_HISTORY_PAGE);
    let history = RunHistory::load_recent(report_path, job_name, limit)?;
    Ok(HistoryView {
        runs: history.records().to_vec(),
        skipped_lines: history.skipped_lines(),
        limit_applied: limit,
    })
}

/// Runs the deterministic advisor over that same history.
///
/// The judgement stays here rather than in any UI: `advise::analyse` is where the thresholds, the
/// minimum sample sizes and the two-gate anomaly rule are tested. A frontend that re-derived any
/// of that would be a second, silently diverging definition.
pub fn read_advice(
    report_path: &Path,
    job_name: Option<&str>,
) -> Result<Vec<crate::advise::Advice>, IngestError> {
    let history = RunHistory::load_recent(report_path, job_name, DEFAULT_HISTORY_WINDOW)?;
    Ok(crate::advise::analyse(&history))
}

/// Scheduled tasks (Windows Task Scheduler) whose command line references `config_path` —
/// read-only, for the console's "does a schedule already point at this file" badge
/// (PIANO_GUI.md, Onda 1). Answers a question, never acts on one: there is no
/// install/uninstall path through this function or anything that calls it.
pub fn schedules_referencing(config_path: &Path) -> Result<Vec<String>, IngestError> {
    crate::schedule::referencing_config(config_path)
}

/// A checkpoint (`checkpoint::Checkpoint`) found on disk, with the path it was read from — needed
/// to resume it later, since [`Checkpoint`] itself carries no notion of where it lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSummary {
    /// Absolute path to the `*.checkpoint.json` file — what `start_resume`/`resume_arguments`
    /// need, since resuming means passing this exact path to `--resume-from`.
    pub path: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source: String,
    pub dest: String,
    /// Why the checkpoint was written, e.g. `"interrupted by Ctrl+C"` — shown verbatim, not
    /// interpreted: the reason string is written once, at the point of interruption, and nothing
    /// here has more information about it than that.
    pub reason: String,
}

/// Checkpoints found directly inside `dir` — for Onda 3's "elenco dei checkpoint trovati accanto
/// ai report" (`PIANO_GUI.md`). Deliberately a directory scan rather than computing one expected
/// path per job: a job's effective `--report-path` can be namespaced per job (F33/D12) or carry a
/// `{timestamp}` placeholder resolved fresh on every run (P1), so there is no single path to
/// compute from a config alone without duplicating that resolution logic here — and every
/// duplicated judgement is a second place for it to drift from `main.rs`'s real behaviour.
/// Scanning for what is actually on disk sidesteps the whole problem: a checkpoint's own
/// `source`/`dest`/`timestamp` say what it is, read directly from the file, not inferred from a
/// job's current configuration (which may have changed since the checkpoint was written).
///
/// Sorted newest first. A checkpoint that fails to parse (partial write, unrelated `.checkpoint.json`
/// left by something else) is silently skipped rather than failing the whole listing — the same
/// tolerance `IngestCache::load_from` and `RunHistory`'s skipped-line handling already apply to
/// other best-effort, non-critical reads; one unreadable file must not hide every other real one.
pub fn list_checkpoints(dir: &Path) -> Result<Vec<CheckpointSummary>, IngestError> {
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // A directory that does not exist yet (a config that has never produced a checkpoint) is
        // "no checkpoints", not an error — the caller does not need to check first.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(error) => return Err(IngestError::io(dir, error)),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".checkpoint.json"))
        {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(checkpoint) = serde_json::from_str::<Checkpoint>(&content) else {
            continue;
        };
        found.push(CheckpointSummary {
            path: path.display().to_string(),
            timestamp: checkpoint.timestamp,
            source: checkpoint.source,
            dest: checkpoint.dest,
            reason: checkpoint.reason,
        });
    }

    found.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
    Ok(found)
}

/// Stores `secret` under `name` in the Windows Credential Manager (F56's `keyring:NAME` form) —
/// the console's Onda 2 equivalent of `--set-credential`. `secret` arrives here only through
/// Tauri's IPC channel, the same safety property `--set-credential`'s stdin-only intake has on the
/// CLI: it is never a process argument, so it never appears in a process list.
///
/// `crypto::write_credential`/`delete_credential` are `#[cfg(windows)]` with no non-Windows stub
/// (unlike `read_credential`, which has one) — `main.rs` handles that split inline at its own two
/// call sites rather than in `crypto.rs` itself, and this mirrors that same pattern rather than
/// adding a third copy of the stub to the shared module.
pub fn set_credential(name: &str, secret: &str) -> Result<(), IngestError> {
    #[cfg(windows)]
    {
        crate::crypto::write_credential(name, secret)
    }
    #[cfg(not(windows))]
    {
        let _ = (name, secret);
        Err(IngestError::Crypto(
            "credential storage needs the Windows Credential Manager, which this platform does not have"
                .to_string(),
        ))
    }
}

/// Removes `name` from the Windows Credential Manager — the console's Onda 2 equivalent of
/// `--delete-credential`.
pub fn delete_credential(name: &str) -> Result<(), IngestError> {
    #[cfg(windows)]
    {
        crate::crypto::delete_credential(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        Err(IngestError::Crypto(
            "credential storage needs the Windows Credential Manager, which this platform does not have"
                .to_string(),
        ))
    }
}

/// Lists the jobs a config file declares, resolved the same way `run_jobs` resolves them.
///
/// Single-job configs (no `[[jobs]]`) yield one entry, so a UI does not need two code paths.
pub fn list_jobs(config_path: &Path) -> Result<Vec<JobSummary>, IngestError> {
    let config = IngestConfig::load_from(config_path)?;
    let jobs = config.jobs.clone().unwrap_or_default();
    // Same anchor `start_job` gives the child process via `current_dir`: a relative report_path
    // in the TOML means relative to the config file, not to the console's own working directory.
    let anchor = config_path.parent().unwrap_or(Path::new("."));

    if jobs.is_empty() {
        let d = &config.defaults;
        // No `[[jobs]]` at all means `run_jobs` (and its namespacing) never runs — the single
        // implicit job writes exactly the resolved path, unmodified.
        let report_path = report_path_for_summary(d.report_path.as_deref(), None, anchor);
        return Ok(vec![JobSummary {
            name: d.name.clone().unwrap_or_else(|| "job1".to_string()),
            source: d.source.as_ref().map(|p| p.display().to_string()),
            dest: d.dest.as_ref().map(|p| p.display().to_string()),
            backup_type: d.backup_type.map(|k| format!("{k:?}").to_lowercase()),
            mirror: d.mirror.unwrap_or(false),
            verify_integrity: d.verify_integrity.unwrap_or(false),
            fast_verify: d.fast_verify.unwrap_or(false),
            unconfigured: is_placeholder(d.source.as_ref().map(|p| p.to_string_lossy()).as_deref())
                || is_placeholder(d.dest.as_ref().map(|p| p.to_string_lossy()).as_deref()),
            report_path,
        }]);
    }

    Ok(jobs
        .iter()
        .enumerate()
        .map(|(idx, job)| {
            let resolved = job.merged_over(&config.defaults);
            let name = resolved
                .name
                .clone()
                .unwrap_or_else(|| format!("job{}", idx + 1));
            // Namespace only when *this job itself* never set report_path — mirrors main.rs's own
            // `job.report_path.is_none()` check exactly (D12): `resolved` already folded in the
            // top-level default even when the job didn't ask for it, which would otherwise defeat
            // the check every time.
            let namespace_with = job.report_path.is_none().then_some(name.as_str());
            let report_path =
                report_path_for_summary(resolved.report_path.as_deref(), namespace_with, anchor);
            JobSummary {
                // Mirrors `run_jobs`: the job's own name, else the positional fallback. Reading it
                // from `resolved` would be wrong now that `name` no longer inherits, and would have
                // been wrong before too — every unnamed job would have shown the same label.
                name,
                source: resolved.source.as_ref().map(|p| p.display().to_string()),
                dest: resolved.dest.as_ref().map(|p| p.display().to_string()),
                backup_type: resolved
                    .backup_type
                    .map(|k| format!("{k:?}").to_lowercase()),
                mirror: resolved.mirror.unwrap_or(false),
                verify_integrity: resolved.verify_integrity.unwrap_or(false),
                fast_verify: resolved.fast_verify.unwrap_or(false),
                unconfigured: is_placeholder(
                    resolved
                        .source
                        .as_ref()
                        .map(|p| p.to_string_lossy())
                        .as_deref(),
                ) || is_placeholder(
                    resolved
                        .dest
                        .as_ref()
                        .map(|p| p.to_string_lossy())
                        .as_deref(),
                ),
                report_path,
            }
        })
        .collect())
}

/// Reads a JSON report from disk, taking `limit` error entries from `offset`.
///
/// The offset is a parameter and not a fixed `0` because otherwise the paging above would be
/// unreachable: a UI could see that a run produced 5 000 failures and have no way to ask for the
/// second hundred. Re-reading the file per page is deliberate — reports are read on demand, not in
/// a loop, and holding a parsed report in memory between calls would be state this module does not
/// want.
pub fn read_report_page(
    path: &Path,
    offset: usize,
    limit: usize,
) -> Result<ReportView, IngestError> {
    let report = parse_report(path)?;
    Ok(ReportView::from_report(&report, offset, limit))
}

/// Reads a report and returns the first page of its error lists.
pub fn read_report(path: &Path) -> Result<ReportView, IngestError> {
    read_report_page(path, 0, DEFAULT_ERROR_PAGE)
}

fn parse_report(path: &Path) -> Result<IngestReport, IngestError> {
    let content = std::fs::read_to_string(path).map_err(|error| IngestError::io(path, error))?;
    serde_json::from_str(&content).map_err(|error| {
        IngestError::io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })
}

/// Where a setting's effective value was written.
///
/// A `[[jobs]]` file has two places a value can come from and one place it can be absent from
/// both, and to an operator asking "why is *this* job mirroring?" the three are not
/// interchangeable. The TOML does not make it obvious — [`JobConfig::merged_over`] resolves the
/// value and the result carries no trace of where it came from — which is exactly what a settings
/// view can show that reading the file cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingOrigin {
    /// Written by this `[[jobs]]` entry itself.
    Job,
    /// Not written by the job: inherited from the file's top-level defaults.
    Inherited,
    /// Written nowhere in the file; the value shown is rustcopy's own default.
    Default,
}

/// One resolved setting, ready to render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingEntry {
    /// The TOML key — which is also the CLI flag, with `_` for `-`. Deliberately not translated:
    /// it is the string an operator greps for in the file, and a localised label would break that.
    pub key: String,
    /// The effective value, already rendered — and already cut short when `redacted` is set.
    pub value: String,
    pub origin: SettingOrigin,
    /// True when `value` is **not** the stored value, only enough of it to be recognised.
    pub redacted: bool,
    /// Why this setting deserves a second look, when it does. A judgement about backup semantics,
    /// so it is made here and not in the frontend (`docs/archive/PIANO_GUI_TAURI.md` §4.1).
    pub caution: Option<String>,
}

impl SettingEntry {
    /// Builds one entry from the job's own value and the value it would inherit.
    ///
    /// Taking both — rather than the already-merged value — is the whole point: the merged value
    /// alone cannot say whether the job asked for it.
    fn resolve<T>(
        key: &str,
        own: Option<&T>,
        inherited: Option<&T>,
        default: &str,
        render: impl Fn(&T) -> String,
    ) -> Self {
        let (value, origin) = match (own, inherited) {
            (Some(value), _) => (render(value), SettingOrigin::Job),
            (None, Some(value)) => (render(value), SettingOrigin::Inherited),
            (None, None) => (default.to_string(), SettingOrigin::Default),
        };
        Self {
            key: key.to_string(),
            value,
            origin,
            redacted: false,
            caution: None,
        }
    }

    fn caution_when(mut self, condition: bool, message: &str) -> Self {
        if condition {
            self.caution = Some(message.to_string());
        }
        self
    }

    /// Marks the value as cut short — conditionally, because an unset field has nothing to cut.
    /// Claiming otherwise would push the frontend into deciding when the flag means anything,
    /// which is the one thing this module exists to prevent.
    fn redacted_when(mut self, condition: bool) -> Self {
        self.redacted = condition;
        self
    }
}

/// A titled group of settings, so the frontend does not decide what belongs with what.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingGroup {
    pub title: String,
    pub entries: Vec<SettingEntry>,
}

/// Every setting of one job, grouped and resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSettings {
    /// The same label [`list_jobs`] gives the job, for the same reason: it must match the reports
    /// on disk.
    pub name: String,
    pub groups: Vec<SettingGroup>,
}

/// Cuts a URL down to scheme and host.
///
/// A webhook URL **is** the credential: whoever holds a Slack or Teams endpoint can post to that
/// channel, and the secret lives in the path or the query string. Where notifications go is
/// operational information an operator needs; the rest is not, and a settings pane is a window
/// that gets screenshotted and screen-shared. Showing it whole would make the UI a new home for
/// secrets, which ROADMAP's third security warning exists to prevent.
///
/// Cuts at the first `/`, `?` or `#` after the scheme — a query string alone can carry the token
/// (`https://host?token=…`), so stopping at the path would not be enough — and drops the
/// authority's `userinfo`, because `https://user:password@host` carries the credential *before*
/// the host, where neither of those cuts would reach it.
fn redact_endpoint(url: &str) -> String {
    let after_scheme = url.find("://").map(|idx| idx + 3).unwrap_or(0);
    let (scheme, remainder) = url.split_at(after_scheme);

    // Everything up to the first path/query/fragment separator is the authority.
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];

    // `user:password@host` puts a credential inside the authority itself, so keep only what
    // follows the last `@`. Nothing here is a secret once the userinfo is gone: a hostname is the
    // operational half an operator came to check.
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_userinfo, host)| host);

    if authority_end == remainder.len() {
        // No path and no query: there is no tail to stand in for, and appending an ellipsis would
        // invent one. Whether anything was removed at all is the caller's question, answered by
        // comparing this against the stored value.
        format!("{scheme}{host}")
    } else {
        format!("{scheme}{host}/…")
    }
}

fn settings_for(job: &JobConfig, base: &JobConfig, name: String) -> JobSettings {
    let resolved = job.merged_over(base);
    let flag = |value: Option<bool>| value.unwrap_or(false);
    // Renderers for the two field types that are not a plain `to_string`. Closures rather than
    // free functions because they have to match the stored `Option<T>` exactly: a `&Path`/`&[_]`
    // signature would be the tidier shape and would not fit `Option<&PathBuf>`/`Option<&Vec<_>>`.
    let render_path = |value: &PathBuf| value.display().to_string();
    let render_list = |value: &Vec<String>| {
        if value.is_empty() {
            "(nessuno)".to_string()
        } else {
            value.join(", ")
        }
    };

    let selection = SettingGroup {
        title: "Cosa viene copiato".to_string(),
        entries: vec![
            SettingEntry::resolve(
                "source",
                job.source.as_ref(),
                base.source.as_ref(),
                "—",
                render_path,
            )
            .caution_when(
                is_placeholder(resolved.source.as_ref().map(|p| p.to_string_lossy()).as_deref()),
                "Questo è ancora un segnaposto di un file modello, non un percorso: il job non è configurato.",
            ),
            SettingEntry::resolve(
                "dest",
                job.dest.as_ref(),
                base.dest.as_ref(),
                "—",
                render_path,
            )
            .caution_when(
                is_placeholder(resolved.dest.as_ref().map(|p| p.to_string_lossy()).as_deref()),
                "Questo è ancora un segnaposto di un file modello, non un percorso: il job non è configurato.",
            ),
            SettingEntry::resolve(
                "pattern",
                job.pattern.as_ref(),
                base.pattern.as_ref(),
                "*",
                |value| value.clone(),
            ),
            SettingEntry::resolve(
                "exclude_files",
                job.exclude_files.as_ref(),
                base.exclude_files.as_ref(),
                "(nessuno)",
                render_list,
            ),
            SettingEntry::resolve(
                "exclude_dirs",
                job.exclude_dirs.as_ref(),
                base.exclude_dirs.as_ref(),
                "(nessuno)",
                render_list,
            ),
            SettingEntry::resolve(
                "min_age_days",
                job.min_age_days.as_ref(),
                base.min_age_days.as_ref(),
                "—",
                |value| value.to_string(),
            )
            .caution_when(
                resolved.min_age_days.is_some(),
                "Esclude i file modificati da meno giorni di così.",
            ),
            SettingEntry::resolve(
                "max_age_days",
                job.max_age_days.as_ref(),
                base.max_age_days.as_ref(),
                "—",
                |value| value.to_string(),
            )
            .caution_when(
                resolved.max_age_days.is_some(),
                "Esclude i file modificati da più giorni di così.",
            ),
            SettingEntry::resolve(
                "exclude_junctions",
                job.exclude_junctions.as_ref(),
                base.exclude_junctions.as_ref(),
                "false",
                |value| value.to_string(),
            )
            .caution_when(
                !flag(resolved.exclude_junctions),
                "Le giunzioni vengono seguite: un collegamento a un altro albero viene copiato come se ne fosse contenuto.",
            ),
        ],
    };

    let execution = SettingGroup {
        title: "Come viene copiato".to_string(),
        entries: vec![
            SettingEntry::resolve(
                "mirror",
                job.mirror.as_ref(),
                base.mirror.as_ref(),
                "false",
                |value| value.to_string(),
            )
            .caution_when(
                flag(resolved.mirror),
                "Cancella in destinazione i file che non esistono più nella sorgente. È l'impostazione più distruttiva che un job possa avere.",
            ),
            SettingEntry::resolve(
                "dry_run",
                job.dry_run.as_ref(),
                base.dry_run.as_ref(),
                "false",
                |value| value.to_string(),
            )
            .caution_when(
                flag(resolved.dry_run),
                "Simulazione: nessun file viene copiato davvero.",
            ),
            SettingEntry::resolve(
                "threads",
                job.threads.as_ref(),
                base.threads.as_ref(),
                "(automatico)",
                |value| value.to_string(),
            ),
            SettingEntry::resolve(
                "retries",
                job.retries.as_ref(),
                base.retries.as_ref(),
                "3",
                |value| value.to_string(),
            ),
            SettingEntry::resolve(
                "retry_wait_seconds",
                job.retry_wait_seconds.as_ref(),
                base.retry_wait_seconds.as_ref(),
                "5",
                |value| value.to_string(),
            ),
            SettingEntry::resolve(
                "bandwidth_limit_mbps",
                job.bandwidth_limit_mbps.as_ref(),
                base.bandwidth_limit_mbps.as_ref(),
                "(illimitata)",
                |value| value.to_string(),
            ),
            SettingEntry::resolve(
                "no_prescan",
                job.no_prescan.as_ref(),
                base.no_prescan.as_ref(),
                "false",
                |value| value.to_string(),
            )
            .caution_when(
                flag(resolved.no_prescan) && flag(resolved.mirror),
                "Senza prescan, --mirror non può verificare cosa cancellerebbe: richiede sempre --force-purge.",
            ),
            SettingEntry::resolve(
                "long_paths",
                job.long_paths.as_ref(),
                base.long_paths.as_ref(),
                "false",
                |value| value.to_string(),
            ),
            SettingEntry::resolve(
                "preserve_timestamps",
                job.preserve_timestamps.as_ref(),
                base.preserve_timestamps.as_ref(),
                "false",
                |value| value.to_string(),
            ),
            SettingEntry::resolve(
                "preserve_acl",
                job.preserve_acl.as_ref(),
                base.preserve_acl.as_ref(),
                "false",
                |value| value.to_string(),
            ),
        ],
    };

    let verification = SettingGroup {
        title: "Verifica".to_string(),
        entries: vec![
            SettingEntry::resolve(
                "verify_integrity",
                job.verify_integrity.as_ref(),
                base.verify_integrity.as_ref(),
                "false",
                |value| value.to_string(),
            )
            .caution_when(
                !flag(resolved.verify_integrity),
                "Nessuna verifica: il report dice cosa robocopy dichiara di aver copiato, non un confronto dei byte.",
            ),
            SettingEntry::resolve(
                "fast_verify",
                job.fast_verify.as_ref(),
                base.fast_verify.as_ref(),
                "false",
                |value| value.to_string(),
            )
            .caution_when(
                flag(resolved.fast_verify),
                "Salta i file la cui sorgente è immutata dall'ultima verifica riuscita: si fida dell'identità della sorgente invece di rileggere i byte in destinazione, quindi una corruzione nata in destinazione può sfuggire.",
            ),
            SettingEntry::resolve(
                "hash_algo",
                job.hash_algo.as_ref(),
                base.hash_algo.as_ref(),
                "sha256",
                |value| format!("{value:?}").to_lowercase(),
            )
            .caution_when(
                matches!(resolved.hash_algo, Some(HashAlgorithm::Xxh3)),
                "xxh3 non è crittografico: rileva la corruzione, non la manomissione.",
            ),
            SettingEntry::resolve(
                "ignore_transient_missing",
                job.ignore_transient_missing.as_ref(),
                base.ignore_transient_missing.as_ref(),
                "false",
                |value| value.to_string(),
            )
            .caution_when(
                flag(resolved.ignore_transient_missing),
                "I file mancanti riconosciuti come transitori (.log, .tmp, .git/objects) non vengono segnalati.",
            ),
            SettingEntry::resolve(
                "compare_baseline",
                job.compare_baseline.as_ref(),
                base.compare_baseline.as_ref(),
                "false",
                |value| value.to_string(),
            ),
        ],
    };

    let generations = SettingGroup {
        title: "Generazioni e retention".to_string(),
        entries: vec![
            SettingEntry::resolve(
                "backup_type",
                job.backup_type.as_ref(),
                base.backup_type.as_ref(),
                "(copia semplice)",
                |value| format!("{value:?}").to_lowercase(),
            ),
            SettingEntry::resolve(
                "keep_generations",
                job.keep_generations.as_ref(),
                base.keep_generations.as_ref(),
                "(tutte)",
                |value| value.to_string(),
            )
            .caution_when(
                resolved.keep_generations.is_some(),
                "I cicli più vecchi vengono eliminati a fine run. La rotazione è per ciclo e non per singola generazione, per non orfanare un incrementale che dipende da un full.",
            ),
        ],
    };

    let outputs = SettingGroup {
        title: "Cosa viene scritto".to_string(),
        entries: vec![
            SettingEntry::resolve(
                "report_path",
                job.report_path.as_ref(),
                base.report_path.as_ref(),
                "ingest-report.json",
                render_path,
            ),
            SettingEntry::resolve(
                "log_path",
                job.log_path.as_ref(),
                base.log_path.as_ref(),
                "ingest.log",
                render_path,
            ),
            SettingEntry::resolve(
                "html_report_path",
                job.html_report_path.as_ref(),
                base.html_report_path.as_ref(),
                "—",
                render_path,
            ),
        ],
    };

    let hooks = SettingGroup {
        title: "Notifiche e comandi".to_string(),
        entries: vec![
            SettingEntry::resolve(
                "webhook_url",
                job.webhook_url.as_ref(),
                base.webhook_url.as_ref(),
                "—",
                |value| redact_endpoint(value),
            )
            // Derived from whether rendering actually changed the URL, not from the field being
            // set: a bare `https://host` comes back untouched, and labelling it "cut short" would
            // be the flag lying in the other direction.
            .redacted_when(
                resolved
                    .webhook_url
                    .as_deref()
                    .is_some_and(|url| redact_endpoint(url) != url),
            )
            .caution_when(
                resolved.webhook_url.is_some(),
                "L'URL di un webhook vale come credenziale: qui è troncato di proposito a schema e host.",
            ),
            SettingEntry::resolve(
                "pre_command",
                job.pre_command.as_ref(),
                base.pre_command.as_ref(),
                "—",
                |value| value.clone(),
            )
            .caution_when(
                resolved.pre_command.is_some(),
                "Eseguito prima del backup con i privilegi di chi lancia la run — se la run parte dal servizio, quelli dell'account del servizio. Un'uscita diversa da zero annulla il job.",
            ),
            SettingEntry::resolve(
                "post_command",
                job.post_command.as_ref(),
                base.post_command.as_ref(),
                "—",
                |value| value.clone(),
            )
            .caution_when(
                resolved.post_command.is_some(),
                "Eseguito a backup concluso, con gli stessi privilegi. Un fallimento viene registrato nel report ma non fa fallire il job.",
            ),
        ],
    };

    JobSettings {
        name,
        groups: vec![
            selection,
            execution,
            verification,
            generations,
            outputs,
            hooks,
        ],
    }
}

/// Reads a TOML config and returns every job's settings, resolved and grouped.
///
/// Read-only like the rest of this module: it opens the file the CLI already reads and renders it.
/// What it adds over opening the TOML in an editor is the two things the file does not state —
/// which value actually wins for each job ([`SettingOrigin`]), and which settings carry a
/// consequence worth knowing about ([`SettingEntry::caution`]).
///
/// # What this deliberately does not hide
///
/// `pre_command`/`post_command` are shown verbatim, because seeing exactly what a job executes is
/// the entire reason to look at them. That means a secret typed inline into one — `sqlcmd -P …` —
/// is displayed. Redaction cannot be done reliably on an arbitrary shell command, and a partially
/// redacted command would be worse than none: it would read as safe to show while not being so.
/// Secrets belong in `keyring:`/`env:`/`file:` (F56), not on a command line.
pub fn read_settings(config_path: &Path) -> Result<Vec<JobSettings>, IngestError> {
    let config = IngestConfig::load_from(config_path)?;
    let jobs = config.jobs.clone().unwrap_or_default();

    if jobs.is_empty() {
        // Single-job file: there is no inheritance, so nothing can be `Inherited`. Resolving
        // against an empty base states exactly that, instead of inventing a second layer.
        let name = config
            .defaults
            .name
            .clone()
            .unwrap_or_else(|| "job1".to_string());
        return Ok(vec![settings_for(
            &config.defaults,
            &JobConfig::default(),
            name,
        )]);
    }

    Ok(jobs
        .iter()
        .enumerate()
        .map(|(idx, job)| {
            let name = job
                .name
                .clone()
                .unwrap_or_else(|| format!("job{}", idx + 1));
            settings_for(job, &config.defaults, name)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A real report, produced by the compiled binary and then normalised (fixed timestamp, no
    /// real hostname). Deserialised rather than hand-built: it is the exact path a UI takes when
    /// it opens a report from disk, so the fixture exercises that too.  has 19
    /// fields without a serde default, which is also why constructing one by hand here would be
    /// noise rather than clarity.
    const SAMPLE_REPORT: &str = r#"{"schema_version":2,"timestamp":"2026-08-31T00:00:00Z","tool_version":"6.0.0","host_platform":"windows","host_metadata":{"hostname":"HOST","os_name":"windows","logical_cpus":8},"source":"D:/src","dest":"E:/dst","total_files":1,"total_bytes":2,"robocopy_transfer":{"engine":"robocopy","elapsed_seconds":0.058,"throughput_mbps":0.001,"bytes_copied":64,"files_copied":3,"exit_code":1,"exit_code_meaning":"files copied","retry_attempts_used":0,"dry_run":false},"integrity_check":{"files_checked":1,"bytes_hashed":2,"mismatches":[],"missing_in_dest":[],"unreadable":[],"status":"PASSED","truncated":false,"total_errors":0,"skipped_unchanged":0},"phase_timing":{"inventory_seconds":0.0049447,"transfer_seconds":0.0607128,"verification_seconds":0.0072939,"total_seconds":0.0737639},"configuration":{"threads":48,"retries":3,"retry_wait_seconds":5,"pattern":"*","verify_integrity":true,"compare_baseline":false,"dry_run":false},"log_lines_dropped":0,"encrypted":false,"decrypted":false}"#;

    fn report_with_errors(count: usize) -> IngestReport {
        let mut report: IngestReport =
            serde_json::from_str(SAMPLE_REPORT).expect("the fixture must stay deserialisable");
        if let Some(check) = report.integrity_check.as_mut() {
            check.missing_in_dest = (0..count).map(|i| format!("D:/tree/file{i}.dat")).collect();
            check.total_errors = count;
        }
        report
    }

    fn report_without_integrity() -> IngestReport {
        let mut report: IngestReport = serde_json::from_str(SAMPLE_REPORT).expect("deserialisable");
        report.integrity_check = None;
        report
    }

    /// The boundary rule, as a test rather than a comment: a report holding thousands of failed
    /// paths must not put thousands of strings into one IPC message.
    #[test]
    fn a_large_error_list_is_paged_not_sent_whole() {
        let report = report_with_errors(5_000);
        let view = ReportView::from_report(&report, 0, DEFAULT_ERROR_PAGE);

        assert_eq!(
            view.missing_in_dest.entries.len(),
            DEFAULT_ERROR_PAGE,
            "only a page crosses the boundary"
        );
        assert_eq!(
            view.missing_in_dest.total, 5_000,
            "but the true total is reported, so the UI can say 100 of 5000"
        );
        assert_eq!(view.integrity_error_count, 5_000);
    }

    /// A page past the end is empty rather than an error: a UI scrolling to the last page must not
    /// have to know the length in advance.
    #[test]
    fn paging_past_the_end_yields_an_empty_page() {
        let report = report_with_errors(10);
        let view = ReportView::from_report(&report, 50, DEFAULT_ERROR_PAGE);
        assert!(view.missing_in_dest.entries.is_empty());
        assert_eq!(view.missing_in_dest.total, 10);
        assert_eq!(view.missing_in_dest.offset, 50);
    }

    /// `truncated` on the source report means the cap was hit, so "total" is a floor, not a count.
    /// A UI that renders "10000 errors" instead of "at least 10000" is lying by rounding.
    #[test]
    fn a_report_truncated_at_source_says_so() {
        let mut report = report_with_errors(3);
        if let Some(check) = report.integrity_check.as_mut() {
            check.truncated = true;
        }
        let view = ReportView::from_report(&report, 0, DEFAULT_ERROR_PAGE);
        assert!(view.missing_in_dest.truncated_at_source);
    }

    /// A report with no integrity check at all (verification not requested) must render, not panic.
    #[test]
    fn a_report_without_an_integrity_check_still_produces_a_view() {
        let view = ReportView::from_report(&report_without_integrity(), 0, DEFAULT_ERROR_PAGE);
        assert_eq!(view.integrity_error_count, 0);
        assert!(view.mismatches.entries.is_empty());
        assert_eq!(view.integrity_status, None);
    }

    /// The view must be serializable: it is the whole point, and a non-`Serialize` field would only
    /// be discovered when the GUI crate finally tried to return it.
    #[test]
    fn the_view_serializes_to_json() {
        let view = ReportView::from_report(&report_with_errors(3), 0, DEFAULT_ERROR_PAGE);
        let json = serde_json::to_string(&view).expect("ReportView must serialize");
        assert!(json.contains("missing_in_dest"));

        let back: ReportView = serde_json::from_str(&json).expect("and round-trip");
        assert_eq!(back, view);
    }
    /// Most of `examples/` is written to be adapted, not run. A list that renders
    /// `<PERCORSO_SORGENTE_1>` in the source column shows a job that looks configured and is not —
    /// which is exactly how someone trying the product for the first time ends up staring at a
    /// screen that seems fine.
    #[test]
    fn a_job_still_holding_template_placeholders_is_flagged_as_unconfigured() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("modello.toml");
        std::fs::write(
            &path,
            "source = \"<PERCORSO_SORGENTE>\"
dest = \"<PERCORSO_NAS>\"
",
        )
        .expect("write");

        let jobs = list_jobs(&path).expect("reads");
        assert!(jobs[0].unconfigured);

        let settings = read_settings(&path).expect("reads");
        assert!(
            entry(&settings[0], "source").caution.is_some(),
            "and the settings pane says why, not just that"
        );
    }

    /// The check must not fire on a real path. Windows forbids `<` and `>` in paths, so the shape
    /// it looks for cannot occur in one — stated as a test rather than trusted.
    #[test]
    fn a_real_path_is_not_mistaken_for_a_placeholder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vero.toml");
        std::fs::write(
            &path,
            "source = \"D:/src\"
dest = \"E:/dst\"
",
        )
        .expect("write");

        assert!(!list_jobs(&path).expect("reads")[0].unconfigured);
        assert!(entry(&read_settings(&path).expect("reads")[0], "source")
            .caution
            .is_none());
    }

    /// `list_jobs` must label unnamed jobs the way `run_jobs` does — positionally — not with a
    /// shared name inherited from the defaults. Before `merged_over` stopped inheriting `name`,
    /// every unnamed job here would have rendered as the same label while writing to genuinely
    /// different destinations: a list that lies about which job is which.
    #[test]
    fn unnamed_jobs_are_listed_positionally_not_under_a_shared_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("jobs.toml");
        std::fs::write(
            &path,
            r#"
name = "condiviso"
verify_integrity = true

[[jobs]]
source = "D:/a"
dest = "E:/a"

[[jobs]]
source = "D:/b"
dest = "E:/b"
mirror = true
"#,
        )
        .expect("write config");

        let jobs = list_jobs(&path).expect("config parses");

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].name, "job1");
        assert_eq!(jobs[1].name, "job2");
        assert_ne!(
            jobs[0].name, jobs[1].name,
            "two jobs must be distinguishable"
        );
        assert!(
            jobs[0].verify_integrity && jobs[1].verify_integrity,
            "ordinary settings still inherit from the defaults"
        );
        assert!(!jobs[0].mirror);
        assert!(
            jobs[1].mirror,
            "--mirror must be visible per job: it purges"
        );
    }

    /// A single-job config yields one entry, so a UI needs one code path, not two.
    #[test]
    fn a_config_without_a_jobs_array_still_lists_one_job() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("single.toml");
        std::fs::write(
            &path,
            "source = \"D:/only\"
dest = \"E:/only\"
",
        )
        .expect("write");

        let jobs = list_jobs(&path).expect("config parses");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].source.as_deref(), Some("D:/only"));
    }
    /// The paging must be reachable from the public entry point, not only from
    /// `ReportView::from_report`. Without an offset parameter a UI could see that a run produced
    /// thousands of failures and have no way to ask for anything past the first page.
    #[test]
    fn the_second_page_of_errors_is_reachable_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("report.json");
        let report = report_with_errors(250);
        std::fs::write(&path, serde_json::to_string(&report).expect("serialize")).expect("write");

        let first = read_report(&path).expect("first page");
        let second =
            read_report_page(&path, DEFAULT_ERROR_PAGE, DEFAULT_ERROR_PAGE).expect("second page");

        assert_eq!(first.missing_in_dest.entries.len(), DEFAULT_ERROR_PAGE);
        assert_eq!(second.missing_in_dest.entries.len(), DEFAULT_ERROR_PAGE);
        assert_ne!(
            first.missing_in_dest.entries, second.missing_in_dest.entries,
            "the second page must hold different entries, not repeat the first"
        );
        assert_eq!(second.missing_in_dest.offset, DEFAULT_ERROR_PAGE);
        assert_eq!(
            second.missing_in_dest.total, 250,
            "the total stays the true one"
        );
    }

    /// `fast_verify` must travel alongside `verify_integrity`: a job that verifies with fast-verify
    /// on has a weaker guarantee than one without, and a UI showing only the first would overstate
    /// it.
    #[test]
    fn fast_verify_is_surfaced_next_to_verify_integrity() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("j.toml");
        std::fs::write(
            &path,
            "source = \"D:/a\"
dest = \"E:/a\"
verify_integrity = true
fast_verify = true
",
        )
        .expect("write");

        let jobs = list_jobs(&path).expect("parses");
        assert!(jobs[0].verify_integrity);
        assert!(
            jobs[0].fast_verify,
            "a UI must be able to tell a full verification from a fast one"
        );
    }
    /// `limit` is caller-controlled, so it needs a ceiling: without one,
    /// `read_report_page(path, 0, usize::MAX)` would return every stored path from all three
    /// lists — up to 30 000 strings in one IPC response, defeating the whole point of paging. The
    /// parameter was added to make paging reachable; this stops it from also making paging
    /// optional.
    #[test]
    fn an_oversized_page_limit_is_clamped() {
        let report = report_with_errors(5_000);

        let view = ReportView::from_report(&report, 0, usize::MAX);

        assert_eq!(
            view.missing_in_dest.entries.len(),
            MAX_ERROR_PAGE,
            "a caller asking for everything still gets at most one capped page"
        );
        assert_eq!(
            view.missing_in_dest.total, 5_000,
            "and is still told how many there really are"
        );
    }

    /// Below the ceiling the caller's limit is honoured exactly, so the clamp cannot be mistaken
    /// for a fixed page size.
    #[test]
    fn a_limit_below_the_ceiling_is_used_as_given() {
        let view = ReportView::from_report(&report_with_errors(5_000), 0, 7);
        assert_eq!(view.missing_in_dest.entries.len(), 7);
    }
    fn history_at(dir: &std::path::Path, runs: usize) -> std::path::PathBuf {
        let report = dir.join("report.json");
        for i in 0..runs {
            let mut rec: crate::history::RunRecord = serde_json::from_str(
                r#"{"timestamp":"2026-08-31T00:00:00Z","source":"D:/s","dest":"E:/d","exit_code":0,
                    "total_files":1,"total_bytes":2,"files_copied":1,"bytes_copied":2,
                    "elapsed_seconds":1.0,"throughput_mbps":1.0,"inventory_seconds":0.5,
                    "transfer_seconds":0.5,"threads":4,"logical_cpus":4,"dry_run":false}"#,
            )
            .expect("fixture");
            rec.elapsed_seconds = i as f64 + 1.0;
            RunHistory::append(&report, None, &rec).expect("append");
        }
        report
    }

    /// The history window is bounded the same way the error pages are — decided up front this
    /// time, rather than after a review pointed out that a caller could ask for everything.
    #[test]
    fn an_oversized_history_window_is_clamped() {
        let dir = tempfile::tempdir().expect("tempdir");
        let report = history_at(dir.path(), 20);

        let view = read_history(&report, None, usize::MAX).expect("reads");

        assert_eq!(
            view.limit_applied, MAX_HISTORY_PAGE,
            "the ceiling is reported, not hidden"
        );
        assert_eq!(
            view.runs.len(),
            20,
            "and everything below it still comes back"
        );
    }

    /// A window smaller than the history keeps the most recent runs, not the oldest — a console
    /// showing "the last 5" must not show the first 5.
    #[test]
    fn a_small_window_keeps_the_most_recent_runs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let report = history_at(dir.path(), 10);

        let view = read_history(&report, None, 3).expect("reads");

        assert_eq!(view.runs.len(), 3);
        let seen: Vec<f64> = view.runs.iter().map(|r| r.elapsed_seconds).collect();
        assert_eq!(seen, vec![8.0, 9.0, 10.0]);
    }

    /// A missing index is an empty history, not an error: a job that has never run is a normal
    /// state and a console must render it as one.
    #[test]
    fn a_job_that_has_never_run_yields_an_empty_history() {
        let dir = tempfile::tempdir().expect("tempdir");
        let view = read_history(&dir.path().join("report.json"), None, 50).expect("reads");
        assert!(view.runs.is_empty());
        assert_eq!(view.skipped_lines, 0);
    }

    /// The advice must cross the IPC boundary, which means it has to serialize — a fact only
    /// discovered at the boundary otherwise.
    #[test]
    fn advice_serializes_for_the_ipc_boundary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let report = history_at(dir.path(), 6);

        let advice = read_advice(&report, None).expect("analyses");
        assert!(
            !advice.is_empty(),
            "six runs produce at least one observation"
        );

        let json = serde_json::to_string(&advice).expect("Advice must serialize");
        let back: Vec<crate::advise::Advice> = serde_json::from_str(&json).expect("round-trip");
        assert_eq!(back, advice);
    }
    /// A zero limit must not mean "everything". `RunHistory::load_recent` documents zero as
    /// unbounded, so the upper-bound-only clamp this replaced let a caller ask for the whole index
    /// by asking for nothing — the ceiling had a hole in its floor.
    #[test]
    fn a_zero_history_limit_does_not_mean_unlimited() {
        let dir = tempfile::tempdir().expect("tempdir");
        let report = history_at(dir.path(), 20);

        let view = read_history(&report, None, 0).expect("reads");

        assert_eq!(
            view.limit_applied, 1,
            "zero is raised to the smallest real window"
        );
        assert_eq!(
            view.runs.len(),
            1,
            "and returns that window, not the whole index"
        );
    }

    /// A `[[jobs]]` file with one inherited setting, one overridden, and one written nowhere.
    fn settings_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("jobs.toml");
        std::fs::write(
            &path,
            r#"
source = "D:/src"
dest = "E:/dst"
verify_integrity = true
webhook_url = "https://hooks.slack.com/services/T00000/B00000/xoxbSuperSecretToken"

[[jobs]]
name = "documenti"

[[jobs]]
name = "archivio"
verify_integrity = false
mirror = true
pre_command = "net stop MSSQLSERVER"
"#,
        )
        .expect("write");
        (dir, path)
    }

    fn entry<'a>(settings: &'a JobSettings, key: &str) -> &'a SettingEntry {
        settings
            .groups
            .iter()
            .flat_map(|group| group.entries.iter())
            .find(|entry| entry.key == key)
            .unwrap_or_else(|| panic!("the view must carry {key}"))
    }

    /// The reason this view exists: the resolved value alone cannot say who asked for it, and an
    /// operator looking at a job that mirrors needs to know whether *that job* said so or whether
    /// it inherited it from the top of the file.
    #[test]
    fn a_setting_says_whether_the_job_asked_for_it_or_inherited_it() {
        let (_dir, path) = settings_fixture();
        let all = read_settings(&path).expect("reads");

        let documenti = &all[0];
        assert_eq!(entry(documenti, "verify_integrity").value, "true");
        assert_eq!(
            entry(documenti, "verify_integrity").origin,
            SettingOrigin::Inherited,
            "the job does not set it; it comes from the file's defaults"
        );

        let archivio = &all[1];
        assert_eq!(entry(archivio, "verify_integrity").value, "false");
        assert_eq!(
            entry(archivio, "verify_integrity").origin,
            SettingOrigin::Job,
            "this job overrides the shared default"
        );

        assert_eq!(
            entry(archivio, "long_paths").origin,
            SettingOrigin::Default,
            "written nowhere in the file"
        );
    }

    /// A webhook URL is the credential — holding a Slack endpoint is enough to post to that
    /// channel — and a settings pane is a window that gets screenshotted and screen-shared. The
    /// host must survive, because where notifications go is what an operator came to check.
    #[test]
    fn a_webhook_url_reaches_the_view_without_its_secret() {
        let (_dir, path) = settings_fixture();
        let all = read_settings(&path).expect("reads");

        let webhook = entry(&all[0], "webhook_url");
        assert!(
            !webhook.value.contains("xoxbSuperSecretToken"),
            "the secret must not cross into the view, got {}",
            webhook.value
        );
        assert!(
            webhook.value.starts_with("https://hooks.slack.com"),
            "but the destination must still be recognisable, got {}",
            webhook.value
        );
        assert!(webhook.redacted, "and the view must admit it is cut short");
    }

    /// A token can live in the query string with no path at all, so cutting at the first `/` after
    /// the scheme would have left it in place.
    #[test]
    fn redaction_cuts_a_query_string_too() {
        assert_eq!(
            redact_endpoint("https://notify.internal?token=secret"),
            "https://notify.internal/…"
        );
        assert_eq!(
            redact_endpoint("https://hooks.slack.com/services/T0/B0/zzz"),
            "https://hooks.slack.com/…"
        );
        assert_eq!(
            redact_endpoint("https://notify.internal"),
            "https://notify.internal",
            "a bare host holds no secret and is shown whole"
        );
    }

    /// A credential can sit **before** the host, where neither the path cut nor the query cut
    /// reaches it — and `https://user:password@host` has no path and no query at all, so the first
    /// version of this function returned it whole. The hostname must still survive: it is the half
    /// an operator opened the pane to check.
    #[test]
    fn redaction_drops_a_credential_carried_in_the_authority() {
        assert_eq!(
            redact_endpoint("https://user:password@notify.internal"),
            "https://notify.internal"
        );
        assert_eq!(
            redact_endpoint("https://token@notify.internal/hooks/abc"),
            "https://notify.internal/…"
        );
    }

    /// The flag has to track what rendering actually did. Set from "the field is present", it
    /// claimed a bare `https://host` had been cut short — the same failure as claiming an unset
    /// field had been, only in the other direction.
    #[test]
    fn a_url_that_survives_rendering_intact_is_not_marked_as_cut_short() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bare.toml");
        std::fs::write(
            &path,
            "source = \"D:/src\"
dest = \"E:/dst\"
webhook_url = \"https://notify.internal\"
",
        )
        .expect("write");

        let all = read_settings(&path).expect("reads");
        let webhook = entry(&all[0], "webhook_url");

        assert_eq!(webhook.value, "https://notify.internal");
        assert!(
            !webhook.redacted,
            "nothing was removed, so nothing may claim to have been"
        );

        let with_secret = dir.path().join("secret.toml");
        std::fs::write(
            &with_secret,
            "source = \"D:/src\"
dest = \"E:/dst\"
webhook_url = \"https://u:p@notify.internal\"
",
        )
        .expect("write");
        let all = read_settings(&with_secret).expect("reads");
        assert!(
            entry(&all[0], "webhook_url").redacted,
            "but a stripped userinfo must be declared, even with no path to cut"
        );
    }

    /// Cautions are judgements about backup semantics, so they belong here rather than in the
    /// frontend — and they must track the *effective* value, not the presence of a key.
    #[test]
    fn the_destructive_setting_is_flagged_only_where_it_is_actually_on() {
        let (_dir, path) = settings_fixture();
        let all = read_settings(&path).expect("reads");

        assert!(
            entry(&all[0], "mirror").caution.is_none(),
            "this job does not mirror"
        );
        let flagged = entry(&all[1], "mirror")
            .caution
            .as_ref()
            .expect("a mirroring job must carry the warning");
        assert!(flagged.contains("Cancella"));

        assert!(
            entry(&all[1], "pre_command").caution.is_some(),
            "a configured hook must say whose privileges it runs with"
        );
    }

    /// Seeing exactly what a job executes is the whole reason to look at a hook, so it is shown
    /// verbatim. Stated as a test because it is a deliberate choice, not an oversight: an arbitrary
    /// shell command cannot be redacted reliably, and a half-redacted one would read as safe.
    #[test]
    fn a_hook_command_is_shown_verbatim() {
        let (_dir, path) = settings_fixture();
        let all = read_settings(&path).expect("reads");
        assert_eq!(entry(&all[1], "pre_command").value, "net stop MSSQLSERVER");
        assert!(!entry(&all[1], "pre_command").redacted);
    }

    /// A file with no `[[jobs]]` has no second layer to inherit from, so claiming a value came
    /// from one would be an invention.
    #[test]
    fn a_single_job_file_never_reports_an_inherited_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("single.toml");
        std::fs::write(
            &path,
            "source = \"D:/src\"\ndest = \"E:/dst\"\nmirror = true\n",
        )
        .expect("write");

        let all = read_settings(&path).expect("reads");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "job1", "the label `run_jobs` would use");
        assert!(
            all[0]
                .groups
                .iter()
                .flat_map(|group| group.entries.iter())
                .all(|entry| entry.origin != SettingOrigin::Inherited),
            "nothing can be inherited when there is nothing to inherit from"
        );
        assert_eq!(entry(&all[0], "mirror").origin, SettingOrigin::Job);
        assert!(
            !entry(&all[0], "webhook_url").redacted,
            "an unset field has nothing to cut short, and saying otherwise would leave the              frontend deciding when the flag means anything"
        );
    }

    /// Same rule the rest of this module lives by: a view that does not serialize is discovered
    /// only when the GUI finally tries to return it.
    #[test]
    fn the_settings_view_serializes_to_json() {
        let (_dir, path) = settings_fixture();
        let all = read_settings(&path).expect("reads");

        let json = serde_json::to_string(&all).expect("JobSettings must serialize");
        let back: Vec<JobSettings> = serde_json::from_str(&json).expect("and round-trip");
        assert_eq!(back, all);
    }

    fn write_checkpoint(dir: &std::path::Path, name: &str, when: chrono::DateTime<chrono::Utc>) {
        use clap::Parser;
        let mut args = crate::cli::Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            "D:/src",
            "--dest",
            "E:/dst",
        ])
        .expect("parse");
        // Only reachable via `--restore-from`/`--resume-from` in the real CLI, but the accessors
        // this test needs are private to `cli.rs` beyond the struct fields themselves — setting
        // them directly is fine inside this crate.
        args.source = Some(std::path::PathBuf::from("D:/src"));
        args.dest = Some(std::path::PathBuf::from("E:/dst"));
        let mut checkpoint = Checkpoint::new(&args, "interrupted by Ctrl+C");
        checkpoint.timestamp = when;
        checkpoint
            .write_to(&dir.join(name))
            .expect("write checkpoint fixture");
    }

    /// A directory with no checkpoints (and one that does not exist at all) is "nothing found",
    /// not an error — the caller should not have to check existence first.
    #[test]
    fn list_checkpoints_on_a_missing_directory_is_empty_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist");

        let found = list_checkpoints(&missing).expect("missing dir is not an error");
        assert!(found.is_empty());
    }

    /// Newest first — an operator resuming interrupted work almost always wants the most recent
    /// interruption, not whichever the filesystem happened to enumerate first.
    #[test]
    fn list_checkpoints_sorts_newest_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let older = chrono::DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
            .expect("valid")
            .with_timezone(&chrono::Utc);
        let newer = chrono::DateTime::parse_from_rfc3339("2026-09-03T00:00:00Z")
            .expect("valid")
            .with_timezone(&chrono::Utc);
        write_checkpoint(dir.path(), "a.checkpoint.json", older);
        write_checkpoint(dir.path(), "b.checkpoint.json", newer);

        let found = list_checkpoints(dir.path()).expect("reads");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].timestamp, newer);
        assert_eq!(found[1].timestamp, older);
    }

    /// A file that is not a checkpoint at all (a report, a random `.json`, a `.checkpoint.json`
    /// left over from a build that no longer parses) must not hide the real ones next to it — one
    /// unreadable file is not a reason to report zero resumable runs.
    #[test]
    fn list_checkpoints_skips_unrelated_and_unparseable_json_silently() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_checkpoint(dir.path(), "good.checkpoint.json", chrono::Utc::now());
        std::fs::write(
            dir.path().join("report.json"),
            b"{\"not\":\"a checkpoint\"}",
        )
        .expect("write");
        std::fs::write(
            dir.path().join("broken.checkpoint.json"),
            b"not json at all",
        )
        .expect("write");

        let found = list_checkpoints(dir.path()).expect("reads despite the two bad files");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, "D:/src");
    }

    #[test]
    fn checkpoint_summary_serializes_to_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_checkpoint(dir.path(), "a.checkpoint.json", chrono::Utc::now());

        let found = list_checkpoints(dir.path()).expect("reads");
        let json = serde_json::to_string(&found).expect("CheckpointSummary must serialize");
        let back: Vec<CheckpointSummary> = serde_json::from_str(&json).expect("round-trip");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].dest, "E:/dst");
    }

    fn write_config(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("jobs.toml");
        std::fs::write(&path, body).expect("write");
        (dir, path)
    }

    fn job<'a>(summaries: &'a [JobSummary], name: &str) -> &'a JobSummary {
        summaries
            .iter()
            .find(|j| j.name == name)
            .unwrap_or_else(|| panic!("no job named {name} in {summaries:?}"))
    }

    /// `PathBuf::display()` renders `\` on Windows and `/` on Unix for the exact same logical
    /// path — normalised here so these tests assert on the namespacing/defaulting logic itself,
    /// not on which CI runner happened to build it.
    fn report_path_of(summaries: &[JobSummary], name: &str) -> Option<String> {
        job(summaries, name)
            .report_path
            .as_ref()
            .map(|p| p.replace('\\', "/"))
    }

    /// No `[[jobs]]` at all means `run_jobs` — and its namespacing — never runs: the single
    /// implicit job's report_path is exactly clap's own `--report-path` default, untouched.
    #[test]
    fn report_path_for_a_single_implicit_job_is_the_plain_default() {
        let (dir, path) = write_config("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let jobs = list_jobs(&path).expect("reads");
        assert_eq!(jobs.len(), 1);
        // Anchored at the config's own directory — same convention `start_job` gives the child
        // process via `current_dir` — not left relative (which resolved against the console's own
        // cwd, not the config's, and 404'd when "Apri il report di questa run" tried it for real).
        let expected = dir.path().join("./robocopy_ingest_report.json");
        assert_eq!(
            jobs[0].report_path.as_ref().map(|p| p.replace('\\', "/")),
            Some(expected.display().to_string().replace('\\', "/"))
        );
    }

    /// F33/D12: a `[[jobs]]` entry that never set its own report_path gets one namespaced with
    /// its own name — mirrors `main.rs::run_jobs`'s `job.report_path.is_none()` check exactly, so
    /// two jobs sharing an inherited default do not appear to share one report.
    #[test]
    fn report_path_is_namespaced_per_job_when_the_job_did_not_set_its_own() {
        let (dir, path) = write_config(
            r#"
[[jobs]]
name = "documenti"
source = "D:/docs"
dest = "E:/docs"

[[jobs]]
name = "archivio"
source = "D:/arch"
dest = "E:/arch"
"#,
        );
        let jobs = list_jobs(&path).expect("reads");
        let expect_at = |file: &str| {
            Some(
                dir.path()
                    .join(file)
                    .display()
                    .to_string()
                    .replace('\\', "/"),
            )
        };
        assert_eq!(
            report_path_of(&jobs, "documenti"),
            expect_at("./robocopy_ingest_report.documenti.json")
        );
        assert_eq!(
            report_path_of(&jobs, "archivio"),
            expect_at("./robocopy_ingest_report.archivio.json")
        );
    }

    /// A job that sets its own `report_path` is never namespaced — matches `main.rs` exactly:
    /// the check is on the job's *own* field, not the merged/resolved value.
    #[test]
    fn report_path_is_not_namespaced_when_the_job_set_its_own() {
        let (dir, path) = write_config(
            r#"
[[jobs]]
name = "documenti"
source = "D:/docs"
dest = "E:/docs"
report_path = "reports/documenti.json"
"#,
        );
        let jobs = list_jobs(&path).expect("reads");
        let expected = dir.path().join("reports/documenti.json");
        assert_eq!(
            report_path_of(&jobs, "documenti"),
            Some(expected.display().to_string().replace('\\', "/"))
        );
    }

    /// P1: `{timestamp}` is resolved fresh at the start of each real run. Nothing computed ahead
    /// of time (or, here, read back from the config after the fact) can predict what a specific
    /// past run actually wrote — so this must say "unknown", not guess.
    #[test]
    fn report_path_is_none_when_it_still_carries_the_timestamp_placeholder() {
        let (_dir, path) = write_config(
            "source = \"D:/src\"\ndest = \"E:/dst\"\nreport_path = \"report-{timestamp}.json\"\n",
        );
        let jobs = list_jobs(&path).expect("reads");
        assert_eq!(jobs[0].report_path, None);
    }
}
