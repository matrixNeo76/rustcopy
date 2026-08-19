//! JSON report produced at the end of every run.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cli::Args;
use crate::engine::CopyOutcome;
use crate::errors::IngestError;
use crate::exit_code::RobocopyStatus;
use crate::integrity::IntegrityCheck;
use crate::progress::{speedup_factor, throughput_mbps};
use crate::scan::ScanSummary;

/// Schema version, so downstream consumers can detect format changes.
///
/// F26c (closes D6): bumped 1 -> 2 because a past release renamed `integrity::Mismatch`'s fields
/// (`source_sha256`/`dest_sha256` -> `kind`/`algorithm`/`source_digest`/`dest_digest`) without
/// bumping this constant, so a v1-labelled report could actually be in either shape. Downstream
/// consumers (and `restore::build_restore_args`, which parses the full report to drive
/// `--restore-from`) can now tell the two apart. See `integrity::Mismatch` for the
/// `#[serde(default)]` fields that keep genuinely old (v1, pre-rename) reports deserializable
/// rather than failing outright.
pub const SCHEMA_VERSION: u32 = 2;

/// System host metadata for cross-machine benchmark comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostMetadata {
    pub hostname: String,
    pub os_name: String,
    pub logical_cpus: usize,
}

impl Default for HostMetadata {
    fn default() -> Self {
        Self {
            hostname: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "unknown".to_string()),
            os_name: std::env::consts::OS.to_string(),
            logical_cpus: num_cpus::get(),
        }
    }
}

/// Execution time breakdown per pipeline phase (in seconds).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PhaseTiming {
    pub inventory_seconds: f64,
    pub transfer_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_seconds: Option<f64>,
    pub total_seconds: f64,
}

/// Configuration echoed back into the report for reproducibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationReport {
    pub threads: u16,
    pub retries: u32,
    pub retry_wait_seconds: u64,
    pub pattern: String,
    pub verify_integrity: bool,
    pub compare_baseline: bool,
    pub dry_run: bool,
}

impl From<&Args> for ConfigurationReport {
    fn from(args: &Args) -> Self {
        Self {
            threads: args.threads,
            retries: args.retries,
            retry_wait_seconds: args.retry_wait_seconds,
            pattern: args.pattern.clone(),
            verify_integrity: args.verify_integrity,
            compare_baseline: args.compare_baseline,
            dry_run: args.dry_run,
        }
    }
}

/// Metrics for one transfer (robocopy or baseline).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferReport {
    pub engine: String,
    pub elapsed_seconds: f64,
    pub throughput_mbps: f64,
    pub bytes_copied: u64,
    pub files_copied: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code_meaning: Option<String>,
    pub retry_attempts_used: u32,
    pub dry_run: bool,
}

impl From<&CopyOutcome> for TransferReport {
    fn from(outcome: &CopyOutcome) -> Self {
        Self {
            engine: outcome.engine.to_string(),
            elapsed_seconds: round3(outcome.elapsed.as_secs_f64()),
            throughput_mbps: round3(throughput_mbps(outcome.bytes_copied, outcome.elapsed)),
            bytes_copied: outcome.bytes_copied,
            files_copied: outcome.files_copied,
            exit_code: outcome.exit_code,
            exit_code_meaning: outcome
                .exit_code
                .map(|code| RobocopyStatus::new(code).describe()),
            retry_attempts_used: outcome.retry_attempts_used,
            dry_run: outcome.dry_run,
        }
    }
}

/// P2: delta between this run's transfer and whatever run's report already existed at
/// `--report-path` immediately before this run overwrote it. This crate has no directory-of-
/// historical-reports convention of its own -- the file at `--report-path` is overwritten every
/// run by design -- so "the previous run" here specifically means "whatever was already sitting
/// at this exact path a moment ago", read before this run's `write_to` call touches it, not a
/// directory scan for the most recent file. `PIANO_MIGLIORAMENTI.md` P2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunComparison {
    pub previous_timestamp: DateTime<Utc>,
    pub files_copied_delta: i64,
    pub elapsed_seconds_delta: f64,
    pub throughput_mbps_delta: f64,
    /// Percent change in throughput vs the previous run. Absent (not zero, not infinite) when
    /// the previous run's throughput was 0 -- e.g. an all-skipped incremental run -- since
    /// dividing by that would be undefined, not meaningfully "0% faster".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_mbps_delta_percent: Option<f64>,
}

impl RunComparison {
    fn between(current: &IngestReport, previous: &IngestReport) -> Self {
        let current_mbps = current.robocopy_transfer.throughput_mbps;
        let previous_mbps = previous.robocopy_transfer.throughput_mbps;
        Self {
            previous_timestamp: previous.timestamp,
            files_copied_delta: current.robocopy_transfer.files_copied as i64
                - previous.robocopy_transfer.files_copied as i64,
            elapsed_seconds_delta: round3(
                current.robocopy_transfer.elapsed_seconds
                    - previous.robocopy_transfer.elapsed_seconds,
            ),
            throughput_mbps_delta: round3(current_mbps - previous_mbps),
            throughput_mbps_delta_percent: if previous_mbps > 0.0 {
                Some(round3(
                    (current_mbps - previous_mbps) / previous_mbps * 100.0,
                ))
            } else {
                None
            },
        }
    }
}

/// Reads and parses whatever report (if any) already exists at `path`, for
/// `IngestReport::previous_run_comparison` (P2). Mirrors `IngestCache::load_from`'s "silently
/// fall back to nothing on a parse error" pattern (see `cache.rs`) -- a corrupt or foreign JSON
/// file at `path` degrades this nice-to-have field to absent, it must never fail the run that's
/// trying to write a report of its own.
pub fn read_previous_report(path: &Path) -> Option<IngestReport> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Full report written to `--report-path`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestReport {
    pub schema_version: u32,
    pub timestamp: DateTime<Utc>,
    pub tool_version: String,
    pub host_platform: String,
    pub host_metadata: HostMetadata,
    pub source: String,
    pub dest: String,
    pub total_files: usize,
    pub total_bytes: u64,
    pub robocopy_transfer: TransferReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_transfer: Option<TransferReport>,
    /// How many times faster robocopy was than the naive baseline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speedup_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity_check: Option<IntegrityCheck>,
    pub phase_timing: PhaseTiming,
    pub configuration: ConfigurationReport,
    /// Log lines dropped because the bounded async log channel was full (audit-trail gap).
    #[serde(default)]
    pub log_lines_dropped: u64,
    /// Whether destination files were encrypted with AES-256-GCM after the transfer.
    #[serde(default)]
    pub encrypted: bool,
    /// Whether destination files were decrypted with AES-256-GCM after the transfer (F25b,
    /// typically set during `--restore-from` of an encrypted backup).
    #[serde(default)]
    pub decrypted: bool,
    /// Non-fatal problem encountered while delivering the completion webhook, if any.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub webhook_error: Option<String>,
    /// F39: non-fatal problem encountered running `--post-command`, if any. Unlike a
    /// `--pre-command` failure (which aborts the run before this report even exists),
    /// `--post-command` runs after the backup already succeeded, so its failure is recorded here
    /// rather than changing the exit code.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub post_command_error: Option<String>,
    /// D15: set when `--backup-type`'s generation copy itself failed (`execute_generation_backup`
    /// in `main.rs`). Unlike `webhook_error`/`post_command_error` above, this reflects the actual
    /// transfer failing, not a non-fatal side effect after a successful one — the plain-sync
    /// pipeline surfaces the equivalent failure via `EXIT_INGESTION_PROBLEM` plus stderr rather
    /// than a report field, since it doesn't need one (its `TransferReport` already carries partial
    /// per-file stats even on failure; the generation pipeline's naive copy engine does not).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub copy_error: Option<String>,
    /// P2: absent on the first run against a given `--report-path`, or if whatever was there
    /// wasn't a valid `IngestReport`. Set via `attach_previous_comparison`, not the constructor
    /// -- computing it requires the fully-built current report to diff against.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub previous_run_comparison: Option<RunComparison>,
}

impl IngestReport {
    pub fn new(
        args: &Args,
        inventory: &ScanSummary,
        robocopy: &CopyOutcome,
        baseline: Option<&CopyOutcome>,
        integrity: Option<IntegrityCheck>,
    ) -> Self {
        Self::with_timing(
            args,
            inventory,
            robocopy,
            baseline,
            integrity,
            PhaseTiming::default(),
        )
    }

    pub fn with_timing(
        args: &Args,
        inventory: &ScanSummary,
        robocopy: &CopyOutcome,
        baseline: Option<&CopyOutcome>,
        integrity: Option<IntegrityCheck>,
        timing: PhaseTiming,
    ) -> Self {
        let robocopy_transfer = TransferReport::from(robocopy);
        let baseline_transfer = baseline.map(TransferReport::from);
        let speedup = baseline_transfer.as_ref().and_then(|baseline| {
            speedup_factor(robocopy_transfer.elapsed_seconds, baseline.elapsed_seconds).map(round3)
        });

        Self {
            schema_version: SCHEMA_VERSION,
            timestamp: Utc::now(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            host_platform: std::env::consts::OS.to_string(),
            host_metadata: HostMetadata::default(),
            source: args.source().to_string_lossy().into_owned(),
            dest: args.dest().to_string_lossy().into_owned(),
            total_files: inventory.file_count(),
            total_bytes: inventory.total_bytes,
            robocopy_transfer,
            baseline_transfer,
            speedup_factor: speedup,
            integrity_check: integrity,
            phase_timing: timing,
            configuration: ConfigurationReport::from(args),
            log_lines_dropped: 0,
            encrypted: false,
            decrypted: false,
            webhook_error: None,
            post_command_error: None,
            copy_error: None,
            previous_run_comparison: None,
        }
    }

    /// Sets `previous_run_comparison` from `previous`, if given. Call sites read `previous` via
    /// `read_previous_report(&args.report_path)` immediately before this report's own
    /// `write_to` overwrites that same path -- ordering matters, this must run first.
    pub fn attach_previous_comparison(&mut self, previous: Option<IngestReport>) {
        self.previous_run_comparison =
            previous.map(|previous| RunComparison::between(self, &previous));
    }

    /// Pretty-printed JSON, newline terminated.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        Ok(json)
    }

    /// Write the report, creating parent directories if needed.
    pub fn write_to(&self, path: &Path) -> Result<(), IngestError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| IngestError::io(parent, error))?;
            }
        }
        let json = self
            .to_json()
            .map_err(|error| IngestError::io(path, std::io::Error::other(error)))?;
        std::fs::write(path, json).map_err(|error| IngestError::io(path, error))
    }

    /// One-paragraph summary printed on stdout at the end of the run.
    pub fn human_summary(&self) -> String {
        let mut lines = vec![
            format!("Source          : {}", self.source),
            format!("Destination     : {}", self.dest),
            format!(
                "Inventory       : {} file(s), {}",
                self.total_files,
                format_bytes(self.total_bytes)
            ),
            format!(
                "Robocopy        : {} in {:.2}s ({:.2} MB/s), exit code {}, {} retry attempt(s)",
                format_bytes(self.robocopy_transfer.bytes_copied),
                self.robocopy_transfer.elapsed_seconds,
                self.robocopy_transfer.throughput_mbps,
                self.robocopy_transfer
                    .exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
                self.robocopy_transfer.retry_attempts_used,
            ),
        ];

        if let Some(baseline) = &self.baseline_transfer {
            lines.push(format!(
                "Baseline (naive): {} in {:.2}s ({:.2} MB/s)",
                format_bytes(baseline.bytes_copied),
                baseline.elapsed_seconds,
                baseline.throughput_mbps,
            ));
        }
        if let Some(speedup) = self.speedup_factor {
            lines.push(format!("Speedup         : {speedup:.2}x vs naive baseline"));
        }
        if let Some(integrity) = &self.integrity_check {
            lines.push(format!(
                "Integrity       : {} ({} file(s) checked, {} mismatch(es), {} missing)",
                if integrity.passed() {
                    "PASSED"
                } else {
                    "FAILED"
                },
                integrity.files_checked,
                integrity.mismatches.len(),
                integrity.missing_in_dest.len(),
            ));
        }
        if let Some(cmp) = &self.previous_run_comparison {
            let percent = cmp
                .throughput_mbps_delta_percent
                .map(|p| format!(" ({p:+.1}%)"))
                .unwrap_or_default();
            lines.push(format!(
                "Vs previous run : {:+} file(s), {:+.2}s, {:+.2} MB/s{} (previous run: {})",
                cmp.files_copied_delta,
                cmp.elapsed_seconds_delta,
                cmp.throughput_mbps_delta,
                percent,
                cmp.previous_timestamp.to_rfc3339(),
            ));
        }
        if self.configuration.dry_run {
            lines.push("Mode            : DRY RUN, nothing was copied".to_string());
        }
        lines.join("\n")
    }
}

fn round3(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    (value * 1000.0).round() / 1000.0
}

/// Human readable size using decimal units, matching the MB/s throughput figures.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::{IntegrityStatus, Mismatch};
    use crate::scan::ScannedFile;
    use clap::Parser;
    use std::path::PathBuf;
    use std::time::Duration;

    fn args() -> Args {
        Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            "D:\\landing",
            "--dest",
            "E:\\warehouse",
            "--threads",
            "16",
            "--retries",
            "4",
            "--retry-wait-seconds",
            "7",
            "--verify-integrity",
            "--compare-baseline",
        ])
        .expect("parse")
    }

    fn inventory() -> ScanSummary {
        ScanSummary {
            files: vec![
                ScannedFile {
                    relative_path: PathBuf::from("a.csv"),
                    size_bytes: 600_000_000,
                    modified_timestamp: 0,
                },
                ScannedFile {
                    relative_path: PathBuf::from("b.csv"),
                    size_bytes: 400_000_000,
                    modified_timestamp: 0,
                },
            ],
            total_bytes: 1_000_000_000,
            total_files_hint: None,
        }
    }

    fn robocopy_outcome() -> CopyOutcome {
        CopyOutcome {
            engine: "robocopy",
            bytes_copied: 1_000_000_000,
            files_copied: 2,
            elapsed: Duration::from_secs(10),
            exit_code: Some(1),
            retry_attempts_used: 1,
            dry_run: false,
        }
    }

    fn baseline_outcome() -> CopyOutcome {
        CopyOutcome {
            engine: "naive-baseline",
            bytes_copied: 1_000_000_000,
            files_copied: 2,
            elapsed: Duration::from_secs(40),
            exit_code: None,
            retry_attempts_used: 0,
            dry_run: false,
        }
    }

    fn integrity_passed() -> IntegrityCheck {
        IntegrityCheck {
            files_checked: 2,
            bytes_hashed: 1_000_000_000,
            mismatches: Vec::new(),
            missing_in_dest: Vec::new(),
            unreadable: Vec::new(),
            status: IntegrityStatus::Passed,
            truncated: false,
            total_errors: 0,
            skipped_unchanged: 0,
        }
    }

    #[test]
    fn report_contains_every_required_field() {
        let report = IngestReport::new(
            &args(),
            &inventory(),
            &robocopy_outcome(),
            Some(&baseline_outcome()),
            Some(integrity_passed()),
        );
        let json: serde_json::Value =
            serde_json::from_str(&report.to_json().expect("serialize")).expect("valid json");

        assert_eq!(json["schema_version"], SCHEMA_VERSION);
        assert!(json["timestamp"]
            .as_str()
            .expect("timestamp")
            .starts_with("20"));
        assert_eq!(json["source"], "D:\\landing");
        assert_eq!(json["dest"], "E:\\warehouse");
        assert_eq!(json["total_files"], 2);
        assert_eq!(json["total_bytes"], 1_000_000_000u64);

        assert_eq!(json["robocopy_transfer"]["elapsed_seconds"], 10.0);
        assert_eq!(json["robocopy_transfer"]["throughput_mbps"], 100.0);
        assert_eq!(json["robocopy_transfer"]["exit_code"], 1);
        assert_eq!(json["robocopy_transfer"]["retry_attempts_used"], 1);
        assert!(json["robocopy_transfer"]["exit_code_meaning"]
            .as_str()
            .expect("meaning")
            .contains("files copied"));

        assert_eq!(json["baseline_transfer"]["elapsed_seconds"], 40.0);
        assert_eq!(json["baseline_transfer"]["throughput_mbps"], 25.0);
        assert_eq!(json["speedup_factor"], 4.0);

        assert_eq!(json["integrity_check"]["status"], "PASSED");
        assert_eq!(json["integrity_check"]["files_checked"], 2);

        assert_eq!(json["configuration"]["threads"], 16);
        assert_eq!(json["configuration"]["retries"], 4);
        assert_eq!(json["configuration"]["retry_wait_seconds"], 7);
        assert_eq!(json["configuration"]["pattern"], "*");
    }

    #[test]
    fn optional_sections_are_omitted_when_absent() {
        let report = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        let json: serde_json::Value =
            serde_json::from_str(&report.to_json().expect("serialize")).expect("valid json");

        assert!(json.get("baseline_transfer").is_none());
        assert!(json.get("speedup_factor").is_none());
        assert!(json.get("integrity_check").is_none());
    }

    #[test]
    fn report_round_trips_through_json() {
        let original = IngestReport::new(
            &args(),
            &inventory(),
            &robocopy_outcome(),
            Some(&baseline_outcome()),
            Some(integrity_passed()),
        );
        let decoded: IngestReport =
            serde_json::from_str(&original.to_json().expect("serialize")).expect("deserialize");
        assert_eq!(decoded, original);
    }

    #[test]
    fn failed_integrity_is_serialized_with_details() {
        let integrity = IntegrityCheck {
            files_checked: 2,
            bytes_hashed: 2048,
            mismatches: vec![Mismatch {
                path: "nested/b.csv".to_string(),
                kind: crate::integrity::MismatchKind::Hash,
                algorithm: "sha256".to_string(),
                source_digest: "aa".to_string(),
                dest_digest: "bb".to_string(),
            }],
            missing_in_dest: vec!["c.csv".to_string()],
            unreadable: Vec::new(),
            status: IntegrityStatus::Failed,
            truncated: false,
            total_errors: 2,
            skipped_unchanged: 0,
        };
        let report = IngestReport::new(
            &args(),
            &inventory(),
            &robocopy_outcome(),
            None,
            Some(integrity),
        );
        let json: serde_json::Value =
            serde_json::from_str(&report.to_json().expect("serialize")).expect("valid json");

        assert_eq!(json["integrity_check"]["status"], "FAILED");
        assert_eq!(
            json["integrity_check"]["mismatches"][0]["path"],
            "nested/b.csv"
        );
        assert_eq!(json["integrity_check"]["missing_in_dest"][0], "c.csv");
    }

    #[test]
    fn speedup_is_absent_when_it_cannot_be_computed() {
        let mut robocopy = robocopy_outcome();
        robocopy.elapsed = Duration::ZERO;
        let report = IngestReport::new(
            &args(),
            &inventory(),
            &robocopy,
            Some(&baseline_outcome()),
            None,
        );
        assert_eq!(report.speedup_factor, None);
    }

    #[test]
    fn writing_creates_parent_directories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("reports/nested/report.json");
        let report = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);

        report.write_to(&path).expect("write report");

        let written = std::fs::read_to_string(&path).expect("read report");
        assert!(
            written.ends_with("}\n"),
            "pretty JSON with trailing newline"
        );
        let decoded: IngestReport = serde_json::from_str(&written).expect("valid json on disk");
        assert_eq!(decoded.total_files, 2);
    }

    #[test]
    fn human_summary_mentions_all_sections() {
        let report = IngestReport::new(
            &args(),
            &inventory(),
            &robocopy_outcome(),
            Some(&baseline_outcome()),
            Some(integrity_passed()),
        );
        let summary = report.human_summary();
        assert!(summary.contains("Robocopy"));
        assert!(summary.contains("Baseline (naive)"));
        assert!(summary.contains("4.00x"));
        assert!(summary.contains("PASSED"));
    }

    #[test]
    fn byte_formatting_uses_decimal_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1_000), "1.00 KB");
        assert_eq!(format_bytes(52_428_800), "52.43 MB");
        assert_eq!(format_bytes(50_000_000_000), "50.00 GB");
    }

    #[test]
    fn rounding_keeps_three_decimals_and_tames_non_finite_values() {
        assert_eq!(round3(1.23456), 1.235);
        assert_eq!(round3(f64::NAN), 0.0);
        assert_eq!(round3(f64::INFINITY), 0.0);
        assert_eq!(round3(Duration::from_millis(1234).as_secs_f64()), 1.234);
    }

    #[test]
    fn read_previous_report_returns_none_for_a_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.json");
        assert!(read_previous_report(&path).is_none());
    }

    #[test]
    fn read_previous_report_returns_none_for_unparseable_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("garbage.json");
        std::fs::write(&path, b"this is not json").expect("write");
        assert!(read_previous_report(&path).is_none());
    }

    #[test]
    fn read_previous_report_parses_a_valid_previous_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("report.json");
        let previous = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        previous.write_to(&path).expect("write");

        let read_back = read_previous_report(&path).expect("should parse");
        assert_eq!(read_back, previous);
    }

    #[test]
    fn attach_previous_comparison_computes_deltas_against_the_prior_run() {
        let previous = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);

        let mut faster_bigger_outcome = robocopy_outcome();
        faster_bigger_outcome.files_copied = 5;
        faster_bigger_outcome.elapsed = Duration::from_secs(5); // half the previous run's 10s
        let mut current =
            IngestReport::new(&args(), &inventory(), &faster_bigger_outcome, None, None);

        current.attach_previous_comparison(Some(previous.clone()));

        let cmp = current
            .previous_run_comparison
            .as_ref()
            .expect("comparison attached");
        assert_eq!(cmp.previous_timestamp, previous.timestamp);
        // previous: 2 files copied; current: 5 files copied.
        assert_eq!(cmp.files_copied_delta, 3);
        // previous: 10s elapsed; current: 5s elapsed.
        assert_eq!(cmp.elapsed_seconds_delta, -5.0);
        // previous: 1_000_000_000 B / 10s = 100 MB/s; current: same bytes / 5s = 200 MB/s.
        assert_eq!(cmp.throughput_mbps_delta, 100.0);
        assert_eq!(cmp.throughput_mbps_delta_percent, Some(100.0));
    }

    #[test]
    fn attach_previous_comparison_is_none_when_there_is_no_previous_report() {
        let mut report = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        report.attach_previous_comparison(None);
        assert_eq!(report.previous_run_comparison, None);
    }

    #[test]
    fn attach_previous_comparison_omits_percent_when_previous_throughput_was_zero() {
        let mut zero_throughput_outcome = robocopy_outcome();
        zero_throughput_outcome.bytes_copied = 0;
        let previous =
            IngestReport::new(&args(), &inventory(), &zero_throughput_outcome, None, None);

        let mut current = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        current.attach_previous_comparison(Some(previous));

        let cmp = current
            .previous_run_comparison
            .expect("comparison attached");
        assert_eq!(cmp.throughput_mbps_delta_percent, None);
    }

    #[test]
    fn previous_run_comparison_is_omitted_from_json_when_absent() {
        let report = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        let json: serde_json::Value =
            serde_json::from_str(&report.to_json().expect("serialize")).expect("valid json");
        assert!(json.get("previous_run_comparison").is_none());
    }

    #[test]
    fn previous_run_comparison_round_trips_through_json_when_present() {
        let previous = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        let mut current = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        current.attach_previous_comparison(Some(previous));

        let decoded: IngestReport =
            serde_json::from_str(&current.to_json().expect("serialize")).expect("deserialize");
        assert_eq!(decoded, current);
    }

    #[test]
    fn human_summary_includes_previous_run_comparison_when_present() {
        let previous = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        let mut current = IngestReport::new(&args(), &inventory(), &robocopy_outcome(), None, None);
        current.attach_previous_comparison(Some(previous));

        let summary = current.human_summary();
        assert!(summary.contains("Vs previous run"));
    }
}
