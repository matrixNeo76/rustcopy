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

/// Settings that change what a resumed transfer does but are not part of [`ConfigurationReport`]
/// (D25). That type is shared with the report of a completed run, where growing it also grows what
/// an operator reads, so the checkpoint carries its own small companion instead.
///
/// Every field is a plain on/off switch that is only ever **turned on** by a resume, never off: a
/// resume starts from the arguments typed on the resuming command line and adds what the
/// interrupted run had. All fields default to `false`, so a checkpoint written before this struct
/// existed (which has no `extras` at all) resumes exactly as it always did.
///
/// What is deliberately **not** here, and why (each one is also pinned by a test):
/// - `--mirror`: a resume must never be more destructive than a fresh run. Restoring it would also
///   send every resume through the purge confirmation, which a console-launched run cannot give.
/// - `--keep-generations`: retention prunes older generations, a purge by another name.
/// - `--pre-command` / `--post-command`: a checkpoint is a file anyone with write access can edit;
///   restoring a shell command from it would make `--resume-from` a way to run one.
/// - `--webhook-url`, `--encrypt-aes256`: they are credentials, and a checkpoint is written next to
///   the report in plain text.
/// - `--compare-baseline`: a benchmark of the engine, not a property of the data being moved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeExtras {
    #[serde(default)]
    pub ignore_transient_missing: bool,
    #[serde(default)]
    pub no_prescan: bool,
    #[serde(default)]
    pub long_paths: bool,
    #[serde(default)]
    pub preserve_timestamps: bool,
    #[serde(default)]
    pub preserve_acl: bool,
}

impl From<&Args> for ResumeExtras {
    fn from(args: &Args) -> Self {
        Self {
            ignore_transient_missing: args.ignore_transient_missing,
            no_prescan: args.no_prescan,
            long_paths: args.long_paths,
            preserve_timestamps: args.preserve_timestamps,
            preserve_acl: args.preserve_acl,
        }
    }
}

/// State captured when a run is interrupted, enough to reconstruct the same invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub schema_version: u32,
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub dest: String,
    pub configuration: ConfigurationReport,
    /// D25: the switches [`ConfigurationReport`] does not carry. Absent in an older checkpoint.
    #[serde(default)]
    pub extras: ResumeExtras,
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
            extras: ResumeExtras::from(args),
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
    apply_configuration(&mut args, &checkpoint.configuration, &checkpoint.extras);
    Ok(args)
}

/// D25: puts the interrupted run's settings back, under one rule -- **a resume may restore what
/// restricts or is neutral, and must never be more destructive than a fresh run**.
///
/// - Scalars the original always overwrote (`pattern`, `threads`, `retries`, ...) keep doing so.
/// - Lists are merged: what the resuming command line typed stays, the interrupted run's entries
///   are added (an exclusion can only narrow the transfer).
/// - Where both sides set a limit, the stricter one wins (`bandwidth_limit_mbps` takes the lower).
/// - On/off switches are only ever switched **on** (`|=`): an interrupted `--dry-run` resumes as a
///   simulation instead of silently turning into a real copy, which is what it did before this.
/// - `--backup-type` is restored, because a plain sync into a generations destination would mix
///   two layouts in one folder. `--keep-generations` is not (see [`ResumeExtras`]).
///
/// `mirror` and everything listed in [`ResumeExtras`]'s "deliberately not here" is left exactly as
/// the resuming command line had it.
fn apply_configuration(args: &mut Args, saved: &ConfigurationReport, extras: &ResumeExtras) {
    args.pattern = saved.pattern.clone();
    args.threads = saved.threads;
    args.retries = saved.retries;
    args.retry_wait_seconds = saved.retry_wait_seconds;
    args.verify_integrity = saved.verify_integrity;
    args.hash_algo = saved.hash_algo;
    args.backup_type = saved.backup_type.or(args.backup_type);

    args.min_age_days = saved.min_age_days.or(args.min_age_days);
    args.max_age_days = saved.max_age_days.or(args.max_age_days);
    args.bandwidth_limit_mbps = match (saved.bandwidth_limit_mbps, args.bandwidth_limit_mbps) {
        (Some(saved), Some(typed)) => Some(saved.min(typed)),
        (saved, typed) => saved.or(typed),
    };

    extend_unique(&mut args.exclude_files, &saved.exclude_files);
    extend_unique(&mut args.exclude_dirs, &saved.exclude_dirs);

    args.dry_run |= saved.dry_run;
    args.fast_verify |= saved.fast_verify;
    args.exclude_junctions |= saved.exclude_junctions;
    args.vss_snapshot |= saved.vss_snapshot;

    args.ignore_transient_missing |= extras.ignore_transient_missing;
    args.no_prescan |= extras.no_prescan;
    args.long_paths |= extras.long_paths;
    args.preserve_timestamps |= extras.preserve_timestamps;
    args.preserve_acl |= extras.preserve_acl;
}

fn extend_unique(into: &mut Vec<String>, from: &[String]) {
    for entry in from {
        if !into.contains(entry) {
            into.push(entry.clone());
        }
    }
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

    // ----- D25: the interrupted run's settings survive a resume --------------------------------

    fn interrupted_run() -> Args {
        Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            "D:\\landing",
            "--dest",
            "E:\\warehouse",
            "--bandwidth-limit-mbps",
            "3",
            "--exclude-files",
            "*.tmp",
            "--exclude-dirs",
            "cache",
            "--min-age-days",
            "2",
            "--max-age-days",
            "90",
            "--hash-algo",
            "xxh3",
            "--fast-verify",
            "--verify-integrity",
            "--exclude-junctions",
            "--ignore-transient-missing",
            "--long-paths",
            "--preserve-acl",
            "--preserve-timestamps",
            "--no-prescan",
        ])
        .expect("parse interrupted run")
    }

    fn resume(original_run: &Args, resuming: &[&str]) -> Args {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("run.checkpoint.json");
        Checkpoint::new(original_run, "interrupted by Ctrl+C")
            .write_to(&path)
            .expect("write");
        let mut argv = vec!["robocopy_ingest", "--resume-from", "run.checkpoint.json"];
        argv.extend_from_slice(resuming);
        let resuming_invocation = Args::try_parse_from(argv).expect("parse resuming invocation");
        build_resume_args(&resuming_invocation, &path).expect("resume args")
    }

    /// The case that found D25: a run throttled to 3 MB/s resumed at full speed.
    #[test]
    fn a_resume_keeps_the_throttle_filters_and_switches_of_the_interrupted_run() {
        let resumed = resume(&interrupted_run(), &[]);
        assert_eq!(resumed.bandwidth_limit_mbps, Some(3));
        assert_eq!(resumed.exclude_files, vec!["*.tmp".to_string()]);
        assert_eq!(resumed.exclude_dirs, vec!["cache".to_string()]);
        assert_eq!(resumed.min_age_days, Some(2));
        assert_eq!(resumed.max_age_days, Some(90));
        assert_eq!(resumed.hash_algo, crate::integrity::HashAlgorithm::Xxh3);
        assert!(resumed.fast_verify);
        assert!(resumed.exclude_junctions);
        assert!(resumed.ignore_transient_missing);
        assert!(resumed.long_paths);
        assert!(resumed.preserve_acl);
        assert!(resumed.preserve_timestamps);
        assert!(resumed.no_prescan);
    }

    /// An interrupted simulation must not resume as a real copy.
    #[test]
    fn an_interrupted_dry_run_resumes_as_a_dry_run() {
        let original_run = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            "D:\\a",
            "--dest",
            "E:\\b",
            "--dry-run",
        ])
        .expect("parse");
        assert!(resume(&original_run, &[]).dry_run);
    }

    /// Hand-edited or future checkpoint claiming `mirror`: a resume still never mirrors.
    #[test]
    fn a_resume_never_restores_mirror_or_anything_that_runs_or_deletes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("run.checkpoint.json");
        let mut checkpoint = Checkpoint::new(&sample_args(), "interrupted by Ctrl+C");
        checkpoint.configuration.mirror = true;
        checkpoint.write_to(&path).expect("write");

        let resuming_invocation =
            Args::try_parse_from(["robocopy_ingest", "--resume-from", "run.checkpoint.json"])
                .expect("parse");
        let resumed = build_resume_args(&resuming_invocation, &path).expect("resume args");
        assert!(!resumed.mirror, "mirror must stay off");
        assert!(!resumed.force_purge);
        assert_eq!(resumed.keep_generations, None);
        assert_eq!(resumed.pre_command, None);
        assert_eq!(resumed.post_command, None);
        assert_eq!(resumed.webhook_url, None);
        assert_eq!(resumed.encrypt_aes256, None);
    }

    /// What the resuming command line typed is added to, never replaced.
    #[test]
    fn typed_exclusions_and_a_stricter_throttle_win_over_the_checkpoint() {
        let resumed = resume(
            &interrupted_run(),
            &["--exclude-files", "*.bak", "--bandwidth-limit-mbps", "1"],
        );
        assert_eq!(
            resumed.exclude_files,
            vec!["*.bak".to_string(), "*.tmp".to_string()]
        );
        assert_eq!(resumed.bandwidth_limit_mbps, Some(1));

        let resumed = resume(&interrupted_run(), &["--bandwidth-limit-mbps", "50"]);
        assert_eq!(
            resumed.bandwidth_limit_mbps,
            Some(3),
            "the lower limit wins"
        );
    }

    /// A checkpoint written before D25 has no `extras` and only the first seven configuration
    /// fields: it must still load, and resume as it always did.
    #[test]
    fn a_checkpoint_from_before_d25_still_loads_and_resumes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("old.checkpoint.json");
        std::fs::write(
            &path,
            r#"{
              "schema_version": 1,
              "timestamp": "2026-08-03T09:14:22Z",
              "source": "D:\\landing",
              "dest": "E:\\warehouse",
              "configuration": {
                "threads": 4, "retries": 3, "retry_wait_seconds": 5, "pattern": "*.csv",
                "verify_integrity": true, "compare_baseline": false, "dry_run": false
              },
              "reason": "interrupted by Ctrl+C"
            }"#,
        )
        .expect("write");

        let resuming_invocation =
            Args::try_parse_from(["robocopy_ingest", "--resume-from", "old.checkpoint.json"])
                .expect("parse");
        let resumed = build_resume_args(&resuming_invocation, &path).expect("resume args");
        assert_eq!(resumed.pattern, "*.csv");
        assert_eq!(resumed.threads, 4);
        assert!(resumed.verify_integrity);
        assert_eq!(resumed.bandwidth_limit_mbps, None);
        assert!(resumed.exclude_files.is_empty());
        assert!(!resumed.dry_run);
    }
}
