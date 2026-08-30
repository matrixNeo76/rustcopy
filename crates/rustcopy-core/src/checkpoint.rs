//! F31 (closes O5): minimal state written when a run is interrupted (Ctrl+C), so `--resume-from`
//! has something to read.
//!
//! This is deliberately **not** a mid-file resume mechanism: `engine::robocopy::build_args` never
//! passes `/Z` (restartable mode), by design — `ANALYSIS.md` documents that `/Z`/`/ZB` roughly
//! halve small-file throughput on SMB shares, and this crate treats that as a deliberate
//! performance trade-off, not an oversight. Adding true byte-offset resume for a single large file
//! would mean reversing that trade-off.
//!
//! What this *does* rely on: robocopy's own default behaviour (no `/IS`/`/IT`) already skips a
//! file at the destination whose size and timestamp match the source — so simply re-running the
//! same command after an interruption already avoids re-copying whatever fully landed. The actual
//! gap this closes is narrower: before this existed, `run()`'s `Ctrl+C` branch returned
//! immediately without writing anything, so there was no record of *what* the interrupted
//! invocation was even doing. `--resume-from <checkpoint>` reconstructs those arguments — source,
//! dest, pattern, thread/retry settings — the same way `--restore-from` reconstructs a restore
//! invocation from a completed run's report.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cli::Args;
use crate::errors::IngestError;
use crate::report::ConfigurationReport;

/// Schema version for the checkpoint file format, independent of `report::SCHEMA_VERSION` — a
/// checkpoint is a different, much smaller document than a completed run's report.
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

/// State captured when a run is interrupted, enough to reconstruct the same invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub schema_version: u32,
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub dest: String,
    pub configuration: ConfigurationReport,
    /// Why this checkpoint was written, e.g. `"interrupted by Ctrl+C"`.
    pub reason: String,
}

impl Checkpoint {
    pub fn new(args: &Args, reason: impl Into<String>) -> Self {
        Self {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            timestamp: Utc::now(),
            source: args.source().to_string_lossy().into_owned(),
            dest: args.dest().to_string_lossy().into_owned(),
            configuration: ConfigurationReport::from(args),
            reason: reason.into(),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        Ok(json)
    }

    /// Write the checkpoint, creating parent directories if needed.
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
}

/// Where `run()` writes the interruption checkpoint: next to `--report-path`, since that's
/// already the operator-chosen location for this run's artifacts, without needing a dedicated new
/// flag just for this.
pub fn checkpoint_path_for(report_path: &Path) -> PathBuf {
    let mut os = report_path.as_os_str().to_owned();
    os.push(".checkpoint.json");
    PathBuf::from(os)
}

/// Build the resumed `Args`, starting from the arguments the user actually typed on this
/// invocation (`original`) — mirrors `restore::build_restore_args`'s pattern and the exact lesson
/// that fix taught: building a fresh `Args` from scratch instead of cloning `original` silently
/// drops every flag typed alongside `--resume-from` (`--decrypt`, a custom `--log-path`, etc.).
///
/// Unlike `--restore-from`, source and dest are **not** reversed: resuming continues the same
/// source -> dest direction the interrupted run was doing.
pub fn build_resume_args(original: &Args, checkpoint_path: &Path) -> Result<Args, IngestError> {
    let content = std::fs::read_to_string(checkpoint_path)
        .map_err(|error| IngestError::io(checkpoint_path, error))?;
    let checkpoint: Checkpoint = serde_json::from_str(&content)
        .map_err(|error| IngestError::io(checkpoint_path, std::io::Error::other(error)))?;

    let mut args = original.clone();
    args.source = Some(PathBuf::from(checkpoint.source));
    args.dest = Some(PathBuf::from(checkpoint.dest));
    args.pattern = checkpoint.configuration.pattern;
    args.threads = checkpoint.configuration.threads;
    args.retries = checkpoint.configuration.retries;
    args.retry_wait_seconds = checkpoint.configuration.retry_wait_seconds;
    args.verify_integrity = checkpoint.configuration.verify_integrity;
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn sample_args() -> Args {
        Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            "D:\\landing",
            "--dest",
            "E:\\warehouse",
            "--pattern",
            "*.csv",
            "--threads",
            "16",
            "--verify-integrity",
        ])
        .expect("parse")
    }

    #[test]
    fn checkpoint_round_trips_through_json() {
        let args = sample_args();
        let checkpoint = Checkpoint::new(&args, "interrupted by Ctrl+C");
        let json = checkpoint.to_json().expect("serialize");
        let decoded: Checkpoint = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, checkpoint);
        assert_eq!(decoded.reason, "interrupted by Ctrl+C");
    }

    #[test]
    fn checkpoint_path_is_derived_from_the_report_path() {
        let path = checkpoint_path_for(Path::new("C:\\out\\report.json"));
        assert_eq!(path, PathBuf::from("C:\\out\\report.json.checkpoint.json"));
    }

    #[test]
    fn build_resume_args_reconstructs_source_dest_and_configuration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("run.checkpoint.json");
        let original_run = sample_args();
        Checkpoint::new(&original_run, "interrupted by Ctrl+C")
            .write_to(&path)
            .expect("write");

        // The resuming invocation only typed --resume-from; nothing else.
        let resuming_invocation =
            Args::try_parse_from(["robocopy_ingest", "--resume-from", "run.checkpoint.json"])
                .expect("parse resuming invocation");

        let resumed = build_resume_args(&resuming_invocation, &path).expect("resume args");
        assert_eq!(resumed.source, Some(PathBuf::from("D:\\landing")));
        assert_eq!(resumed.dest, Some(PathBuf::from("E:\\warehouse")));
        assert_eq!(resumed.pattern, "*.csv");
        assert_eq!(resumed.threads, 16);
        assert!(resumed.verify_integrity);
    }

    /// Regression-shaped test for the exact F25b lesson: flags typed on the real resume
    /// invocation (here `--quiet`, plus a custom `--log-path`) must survive, not be silently
    /// discarded because a fresh `Args` was built from scratch.
    #[test]
    fn flags_from_the_real_invocation_survive_resume() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("run.checkpoint.json");
        Checkpoint::new(&sample_args(), "interrupted by Ctrl+C")
            .write_to(&path)
            .expect("write");

        let resuming_invocation = Args::try_parse_from([
            "robocopy_ingest",
            "--resume-from",
            "run.checkpoint.json",
            "--quiet",
            "--log-path",
            "custom-resume.log",
        ])
        .expect("parse resuming invocation");

        let resumed = build_resume_args(&resuming_invocation, &path).expect("resume args");
        assert!(resumed.quiet);
        assert_eq!(resumed.log_path, PathBuf::from("custom-resume.log"));
        assert_eq!(resumed.source, Some(PathBuf::from("D:\\landing")));
    }
}
