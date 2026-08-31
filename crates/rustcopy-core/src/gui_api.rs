//! Read-only, serializable views for a user interface (Passo 3 of `PIANO_GUI_TAURI.md`, F53).
//!
//! This module exists so the Tauri commands of `crates/rustcopy-gui` can be **thin wrappers**: a
//! `#[tauri::command]` should call one function here and hand back what it returns, nothing more.
//! That is `PIANO_GUI_TAURI.md` §4.1 made mechanical rather than aspirational — if the judgement
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
//! **2. Read-only.** Nothing here writes, deletes, schedules or installs. The prohibitions kept in
//! ROADMAP F61 and applied to `--advise` hold identically for a GUI: it may show and propose,
//! never act. A v1 with no write path *cannot* damage a backup, which is the strongest guarantee
//! available and the reason §5.2 recommends it.
//!
//! # What is intentionally not here
//!
//! Live progress. `ThroughputProgress` already exposes pollable counters
//! (`current_bytes`/`files`/`average_mbps`) behind lock-free atomics, and §2.3 requires the UI to
//! **sample** them on its own timer rather than be notified per file. Wrapping that in a view here
//! would invite a per-event API, which is precisely the shape D18 showed to be ruinous at this
//! scale (~3 800 files/second on the real profile).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::IngestConfig;
use crate::errors::IngestError;
use crate::report::IngestReport;

/// How many per-file error entries [`ReportView`] returns in one page by default.
///
/// Small on purpose: this is what a table shows at once, and the total is always reported
/// alongside so the UI can say "12 of 10 000" without receiving 10 000.
pub const DEFAULT_ERROR_PAGE: usize = 100;

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
            .take(limit)
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

/// Lists the jobs a config file declares, resolved the same way `run_jobs` resolves them.
///
/// Single-job configs (no `[[jobs]]`) yield one entry, so a UI does not need two code paths.
pub fn list_jobs(config_path: &Path) -> Result<Vec<JobSummary>, IngestError> {
    let config = IngestConfig::load_from(config_path)?;
    let jobs = config.jobs.clone().unwrap_or_default();

    if jobs.is_empty() {
        let d = &config.defaults;
        return Ok(vec![JobSummary {
            name: d.name.clone().unwrap_or_else(|| "job1".to_string()),
            source: d.source.as_ref().map(|p| p.display().to_string()),
            dest: d.dest.as_ref().map(|p| p.display().to_string()),
            backup_type: d.backup_type.map(|k| format!("{k:?}").to_lowercase()),
            mirror: d.mirror.unwrap_or(false),
            verify_integrity: d.verify_integrity.unwrap_or(false),
            fast_verify: d.fast_verify.unwrap_or(false),
        }]);
    }

    Ok(jobs
        .iter()
        .enumerate()
        .map(|(idx, job)| {
            let resolved = job.merged_over(&config.defaults);
            JobSummary {
                // Mirrors `run_jobs`: the job's own name, else the positional fallback. Reading it
                // from `resolved` would be wrong now that `name` no longer inherits, and would have
                // been wrong before too — every unnamed job would have shown the same label.
                name: resolved
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("job{}", idx + 1)),
                source: resolved.source.as_ref().map(|p| p.display().to_string()),
                dest: resolved.dest.as_ref().map(|p| p.display().to_string()),
                backup_type: resolved
                    .backup_type
                    .map(|k| format!("{k:?}").to_lowercase()),
                mirror: resolved.mirror.unwrap_or(false),
                verify_integrity: resolved.verify_integrity.unwrap_or(false),
                fast_verify: resolved.fast_verify.unwrap_or(false),
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
}
