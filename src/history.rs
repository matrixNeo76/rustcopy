//! Append-only index of completed runs (Fase 0 of `VALUTAZIONE_AI.md`, closes the read half of
//! ROADMAP F50).
//!
//! Every run already produces a rich `IngestReport`, and then forgets it: each report is an
//! isolated JSON file, and with P1's `{timestamp}` placeholder in `--report-path` the number of
//! those files grows without bound. Answering "how long does this job usually take?" meant opening
//! and reconciling all of them by hand. This module keeps one compact line per run so the question
//! becomes a single streaming read.
//!
//! # Why NDJSON and not SQLite
//!
//! `VALUTAZIONE_AI.md` originally proposed SQLite, flagging `rusqlite`'s bundled C build as a risk
//! to verify against this crate's Windows+Linux CI before committing to it. Weighed against the
//! precedent already in the codebase, the dependency isn't worth taking:
//!
//! - [`crate::generations`] (D19/D20) already established append-only NDJSON here, with streaming
//!   readers, legacy-format detection and torn-line recovery — tested code to imitate rather than
//!   a second storage engine to maintain.
//! - The scale is tiny. A [`RunRecord`] serializes to a few hundred bytes; one run per hour for a
//!   decade is single-digit MB. SQLite's indexes and query planner buy nothing at that size.
//! - No C toolchain, so nothing changes for the `ubuntu-latest`/`windows-latest` CI matrix.
//! - An operator debugging at 3 AM can `grep` it. That is worth more here than a query language.
//!
//! # Memory discipline (D20/D21)
//!
//! [`RunHistory::load_recent`] streams the file and retains at most `limit` records in a ring
//! buffer, so peak memory is bounded by the caller's window and not by the file's length. Do not
//! add a "load everything" entry point — the whole point of D20 was that callers were paying for
//! history they immediately discarded.
//!
//! # Failure policy
//!
//! Unlike a corrupt `GenerationManifest` (fatal, D14 — it would let an incremental diff against a
//! wrong baseline), a damaged history line costs only a less-informed suggestion. This module
//! therefore mirrors [`crate::cache::IngestCache`]'s tolerant behaviour: skip what cannot be parsed
//! and carry on. **A backup must never fail because its statistics file did.**

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::IngestError;
use crate::report::IngestReport;

/// File name, kept beside the report file rather than at the destination root — see
/// [`RunHistory::path_for`] for the measurement behind that choice.
pub const HISTORY_FILE_NAME: &str = ".rustcopy_history.jsonl";

/// How many records [`RunHistory::load_recent`] keeps when a caller doesn't say. Chosen so a job
/// running hourly still has roughly two months of context to reason over.
pub const DEFAULT_HISTORY_WINDOW: usize = 1_000;

/// One completed run, flattened from [`IngestReport`] to just what statistics need.
///
/// Deliberately **not** a copy of the whole report: this file is read in full by every `--advise`
/// invocation, so it carries aggregates, never per-file data. The full report stays on disk and
/// remains the detailed record; `report_path` points back to it.
///
/// Every field added later must carry `#[serde(default)]`, exactly like `report.rs`'s post-F26c
/// additions — an older line has to stay readable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunRecord {
    pub timestamp: DateTime<Utc>,
    /// `None` for a single-job invocation; the `[[jobs]]` name otherwise (F33).
    #[serde(default)]
    pub job: Option<String>,
    pub source: String,
    pub dest: String,
    /// The process exit code this run produced — `0`-`5`, carrying the semantics of AGENTS.md
    /// rule 12. Kept as the raw number so a `4` (copied, but integrity mismatched) stays
    /// distinguishable from a `1` (the copy itself failed).
    pub exit_code: u8,
    pub total_files: usize,
    pub total_bytes: u64,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub elapsed_seconds: f64,
    pub throughput_mbps: f64,
    pub inventory_seconds: f64,
    pub transfer_seconds: f64,
    #[serde(default)]
    pub verification_seconds: Option<f64>,
    #[serde(default)]
    pub integrity_status: Option<String>,
    #[serde(default)]
    pub integrity_errors: usize,
    pub threads: u16,
    pub logical_cpus: usize,
    #[serde(default)]
    pub backup_type: Option<String>,
    pub dry_run: bool,
    #[serde(default)]
    pub report_path: Option<String>,
}

impl RunRecord {
    /// Flattens a finished report into an index line.
    ///
    /// `exit_code` is passed in rather than derived: `main.rs` computes it once in `execute()`
    /// (`RunOutcome::exit_code`), and recomputing it here from report fields would be a second,
    /// silently divergent definition of the same contract.
    pub fn from_report(report: &IngestReport, exit_code: u8, report_path: Option<&Path>) -> Self {
        let integrity = report.integrity_check.as_ref();
        Self {
            timestamp: report.timestamp,
            job: None,
            source: report.source.clone(),
            dest: report.dest.clone(),
            exit_code,
            total_files: report.total_files,
            total_bytes: report.total_bytes,
            files_copied: report.robocopy_transfer.files_copied,
            bytes_copied: report.robocopy_transfer.bytes_copied,
            elapsed_seconds: report.phase_timing.total_seconds,
            throughput_mbps: report.robocopy_transfer.throughput_mbps,
            inventory_seconds: report.phase_timing.inventory_seconds,
            transfer_seconds: report.phase_timing.transfer_seconds,
            verification_seconds: report.phase_timing.verification_seconds,
            integrity_status: integrity.map(|check| format!("{:?}", check.status)),
            integrity_errors: integrity.map(|check| check.total_errors).unwrap_or(0),
            threads: report.configuration.threads,
            logical_cpus: report.host_metadata.logical_cpus,
            backup_type: None,
            dry_run: report.configuration.dry_run,
            report_path: report_path.map(|p| p.to_string_lossy().into_owned()),
        }
    }

    /// Attaches the `[[jobs]]` name, so two jobs sharing a destination stay distinguishable even
    /// though D12 already namespaces the file itself.
    pub fn with_job(mut self, job: Option<&str>) -> Self {
        self.job = job.map(str::to_owned);
        self
    }

    /// Attaches `--backup-type` (F34), which lives on `Args` rather than in the report.
    pub fn with_backup_type(mut self, backup_type: Option<String>) -> Self {
        self.backup_type = backup_type;
        self
    }

    /// True when the run moved real data. A dry run's timings describe a scan, not a transfer, so
    /// mixing the two would quietly bias every duration statistic downwards.
    pub fn is_real_transfer(&self) -> bool {
        !self.dry_run
    }
}

/// A bounded window of the most recent runs, newest last.
#[derive(Debug, Clone, Default)]
pub struct RunHistory {
    records: Vec<RunRecord>,
    /// Lines the reader could not parse. Surfaced so `--advise` can say the sample is incomplete
    /// instead of silently reasoning over less data than the operator thinks it has.
    skipped_lines: usize,
}

impl RunHistory {
    /// Sibling of the report file: `<report_dir>/.rustcopy_history.jsonl`, namespaced per job the
    /// same way `GenerationManifest::path_for` and `cache::default_cache_path` are (D12) — without
    /// that, two `[[jobs]]` entries sharing a directory would interleave their runs into one
    /// history and every statistic derived from it would describe a job that doesn't exist.
    ///
    /// # Why next to the report and not at `<dest>`
    ///
    /// `.ingest_cache` (F28) and `.rustcopy_generations.json` (F34) do live at the destination
    /// root, so that was the obvious place — and it is wrong for this file, for a reason measured
    /// rather than assumed. Writing anything into `<dest>` after a run changes that directory's
    /// mtime, and robocopy notices on the **next** run: with the index at `<dest>`, a repeat sync
    /// over an unchanged tree went from copying 2 items to copying 3. A statistics file must not
    /// perturb the transfer it is measuring.
    ///
    /// The two existing files get away with it because they are opt-in (`--fast-verify`,
    /// `--backup-type`); this one is written by every run, which would make the effect universal.
    ///
    /// Living beside the report is also the better fit conceptually: the report is already
    /// rustcopy's record *about* a run rather than backed-up content, and it already sits outside
    /// the destination tree. P1's `{timestamp}` placeholder varies the report's file name but never
    /// its directory, so the history stays in one place across runs.
    pub fn path_for(report_path: &Path, job_name: Option<&str>) -> PathBuf {
        let base = report_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(HISTORY_FILE_NAME);
        match job_name {
            Some(name) => crate::namespaced_path(&base, name),
            None => base,
        }
    }

    /// Appends one line. Mirrors `GenerationManifest::append_generation`: open in append mode,
    /// write a single serialized line, never rewrite the file.
    ///
    /// There is no legacy-format branch here because this file has only ever had one format —
    /// unlike the manifest, which predates D19.
    pub fn append(
        report_path: &Path,
        job_name: Option<&str>,
        record: &RunRecord,
    ) -> Result<(), IngestError> {
        let path = Self::path_for(report_path, job_name);
        let mut line = serde_json::to_string(record).map_err(|error| {
            IngestError::io(
                &path,
                std::io::Error::new(std::io::ErrorKind::InvalidData, error),
            )
        })?;
        line.push('\n');

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| IngestError::io(&path, error))?;
        file.write_all(line.as_bytes())
            .map_err(|error| IngestError::io(&path, error))
    }

    /// Streams the file and keeps at most `limit` of the most recent records.
    ///
    /// Returns an empty history when the file doesn't exist — a first run has no past, which is a
    /// normal state and not an error. Unparseable lines are counted in [`Self::skipped_lines`] and
    /// skipped; see the module docs for why this is tolerant where the generation manifest is not.
    pub fn load_recent(
        report_path: &Path,
        job_name: Option<&str>,
        limit: usize,
    ) -> Result<Self, IngestError> {
        let path = Self::path_for(report_path, job_name);
        if !path.exists() {
            return Ok(Self::default());
        }
        let file = std::fs::File::open(&path).map_err(|error| IngestError::io(&path, error))?;
        Self::read_from(BufReader::new(file), limit, &path)
    }

    /// The reader half of [`Self::load_recent`], split out so tests can drive it without a file.
    fn read_from<R: BufRead>(reader: R, limit: usize, path: &Path) -> Result<Self, IngestError> {
        let mut window: VecDeque<RunRecord> = VecDeque::new();
        let mut skipped_lines = 0usize;

        for line in reader.lines() {
            let line = line.map_err(|error| IngestError::io(path, error))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<RunRecord>(trimmed) {
                Ok(record) => {
                    if limit > 0 && window.len() == limit {
                        window.pop_front();
                    }
                    window.push_back(record);
                }
                Err(_) => skipped_lines += 1,
            }
        }

        Ok(Self {
            records: window.into(),
            skipped_lines,
        })
    }

    /// Every record in the window, oldest first.
    pub fn records(&self) -> &[RunRecord] {
        &self.records
    }

    /// How many lines could not be parsed.
    pub fn skipped_lines(&self) -> usize {
        self.skipped_lines
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Only the runs that actually moved data, which is the right sample for any timing or
    /// throughput statistic. See [`RunRecord::is_real_transfer`].
    pub fn real_transfers(&self) -> Vec<&RunRecord> {
        self.records
            .iter()
            .filter(|record| record.is_real_transfer())
            .collect()
    }

    /// The most recent run in the window, dry runs included.
    pub fn latest(&self) -> Option<&RunRecord> {
        self.records.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn record(seconds: f64, exit_code: u8) -> RunRecord {
        RunRecord {
            timestamp: Utc::now(),
            job: None,
            source: "C:/src".into(),
            dest: "D:/dst".into(),
            exit_code,
            total_files: 10,
            total_bytes: 1_000,
            files_copied: 5,
            bytes_copied: 500,
            elapsed_seconds: seconds,
            throughput_mbps: 10.0,
            inventory_seconds: 1.0,
            transfer_seconds: seconds - 1.0,
            verification_seconds: None,
            integrity_status: None,
            integrity_errors: 0,
            threads: 8,
            logical_cpus: 8,
            backup_type: None,
            dry_run: false,
            report_path: None,
        }
    }

    #[test]
    fn path_sits_beside_the_report_and_is_namespaced_per_job() {
        let report = Path::new("D:/reports/ingest-report.json");
        let single = RunHistory::path_for(report, None);
        let per_job = RunHistory::path_for(report, Some("nightly"));

        assert_eq!(single.file_name().unwrap(), ".rustcopy_history.jsonl");
        assert_eq!(
            single.parent(),
            report.parent(),
            "the index must sit beside the report, never inside --dest: writing into the              destination after a run perturbs the next robocopy transfer"
        );
        assert_ne!(single, per_job);
        assert!(
            per_job.to_string_lossy().contains("nightly"),
            "the job name must appear in the path, got {}",
            per_job.display()
        );
    }

    #[test]
    fn a_missing_file_is_an_empty_history_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let history = RunHistory::load_recent(&dir.path().join("report.json"), None, 10).unwrap();
        assert!(history.is_empty());
        assert_eq!(history.skipped_lines(), 0);
    }

    #[test]
    fn append_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let first = record(10.0, 0);
        let second = record(20.0, 4);

        RunHistory::append(&dir.path().join("report.json"), None, &first).unwrap();
        RunHistory::append(&dir.path().join("report.json"), None, &second).unwrap();

        let history = RunHistory::load_recent(&dir.path().join("report.json"), None, 10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history.records()[0], first);
        assert_eq!(history.latest().unwrap().exit_code, 4);
    }

    #[test]
    fn append_never_rewrites_earlier_lines() {
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=5 {
            RunHistory::append(&dir.path().join("report.json"), None, &record(i as f64, 0))
                .unwrap();
        }
        let raw =
            std::fs::read_to_string(RunHistory::path_for(&dir.path().join("report.json"), None))
                .unwrap();
        assert_eq!(raw.lines().count(), 5, "one line per run, appended");
    }

    #[test]
    fn the_window_keeps_the_most_recent_records_not_the_first_ones() {
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=10 {
            RunHistory::append(&dir.path().join("report.json"), None, &record(i as f64, 0))
                .unwrap();
        }

        let history = RunHistory::load_recent(&dir.path().join("report.json"), None, 3).unwrap();

        assert_eq!(history.len(), 3);
        let seen: Vec<f64> = history
            .records()
            .iter()
            .map(|r| r.elapsed_seconds)
            .collect();
        assert_eq!(
            seen,
            vec![8.0, 9.0, 10.0],
            "a bounded window must drop the oldest, not truncate the newest"
        );
    }

    #[test]
    fn a_corrupt_line_is_skipped_and_counted_rather_than_failing_the_load() {
        let raw = format!(
            "{}\nnot json at all\n{}\n",
            serde_json::to_string(&record(1.0, 0)).unwrap(),
            serde_json::to_string(&record(2.0, 0)).unwrap()
        );

        let history =
            RunHistory::read_from(Cursor::new(raw), 100, Path::new("history.jsonl")).unwrap();

        assert_eq!(history.len(), 2, "the readable lines still load");
        assert_eq!(
            history.skipped_lines(),
            1,
            "and the damage is reported, not hidden"
        );
    }

    #[test]
    fn a_torn_trailing_line_does_not_lose_the_rest_of_the_history() {
        // An append interrupted mid-write leaves a partial last line; D19 hit exactly this on the
        // generation manifest.
        let complete = serde_json::to_string(&record(1.0, 0)).unwrap();
        let torn = &complete[..complete.len() / 2];
        let raw = format!("{complete}\n{torn}");

        let history =
            RunHistory::read_from(Cursor::new(raw), 100, Path::new("history.jsonl")).unwrap();

        assert_eq!(history.len(), 1);
        assert_eq!(history.skipped_lines(), 1);
    }

    #[test]
    fn dry_runs_are_excluded_from_the_transfer_sample() {
        let mut dry = record(1.0, 0);
        dry.dry_run = true;

        let raw = format!(
            "{}\n{}\n",
            serde_json::to_string(&dry).unwrap(),
            serde_json::to_string(&record(5.0, 0)).unwrap()
        );
        let history = RunHistory::read_from(Cursor::new(raw), 100, Path::new("h.jsonl")).unwrap();

        assert_eq!(history.len(), 2, "both runs are kept in the history");
        assert_eq!(
            history.real_transfers().len(),
            1,
            "but only the real transfer may feed timing statistics"
        );
    }

    #[test]
    fn a_record_written_before_a_new_field_existed_still_deserializes() {
        // Every optional field must carry #[serde(default)], like report.rs's post-F26c additions.
        let minimal = r#"{
            "timestamp": "2026-08-25T00:00:00Z",
            "source": "C:/src",
            "dest": "D:/dst",
            "exit_code": 0,
            "total_files": 1,
            "total_bytes": 2,
            "files_copied": 1,
            "bytes_copied": 2,
            "elapsed_seconds": 1.0,
            "throughput_mbps": 1.0,
            "inventory_seconds": 0.5,
            "transfer_seconds": 0.5,
            "threads": 4,
            "logical_cpus": 4,
            "dry_run": false
        }"#;

        let parsed: RunRecord = serde_json::from_str(minimal).expect("older line must still parse");
        assert_eq!(parsed.job, None);
        assert_eq!(parsed.integrity_errors, 0);
        assert_eq!(parsed.backup_type, None);
    }

    #[test]
    fn a_limit_of_zero_means_unbounded_rather_than_discarding_everything() {
        let raw = format!(
            "{}\n{}\n",
            serde_json::to_string(&record(1.0, 0)).unwrap(),
            serde_json::to_string(&record(2.0, 0)).unwrap()
        );
        let history = RunHistory::read_from(Cursor::new(raw), 0, Path::new("h.jsonl")).unwrap();
        assert_eq!(history.len(), 2);
    }
}
