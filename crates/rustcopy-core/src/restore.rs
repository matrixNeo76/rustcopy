//! Disaster Recovery and Restore subsystem for robocopy-ingest-cli.
//!
//! Reverses backup requests by reading report JSON configurations and initiating
//! restored copy transfers from Destination back to Source with verification.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cli::Args;
use crate::report::IngestReport;

/// Build the restored `Args` for `--restore-from`, starting from the arguments the user actually
/// typed on this invocation (`original`) rather than from clap defaults.
///
/// # History (why this takes `original` now)
///
/// The previous version built a **brand new** `Args` via `Args::try_parse_from(["--source", ...,
/// "--dest", ...])` and copied over only five fields (`pattern`, `threads`, `retries`,
/// `retry_wait_seconds`, `verify_integrity`) from the report. Every other flag the user actually
/// passed alongside `--restore-from` on the real command line — `--decrypt`, a custom
/// `--log-path`/`--report-path`, `--webhook-url`, `--hash-algo`, `--dry-run` — was silently
/// discarded, because that fresh `Args` never saw them. This was invisible until a black-box test
/// exercised `--restore-from` combined with `--decrypt` end-to-end (encrypt a backup, delete the
/// original, restore *and* decrypt in one command) and the recovered file came back still
/// encrypted: `main.rs` replaces `args` wholesale with this function's return value, so anything
/// this function didn't know to preserve was gone for the rest of the run.
///
/// The fix: clone `original` (which clap already parsed correctly, `--decrypt` and all) and only
/// override the fields that must come from the backup's own report — source/dest (reversed) and
/// the settings that describe *what was backed up* (pattern, thread count, retry policy, whether
/// integrity was verified). Everything else the user typed on this restore invocation survives
/// untouched.
///
/// (Separately, building via `Args::try_parse_from` rather than `Args::default()` is still
/// important on the `original` side, upstream in `main.rs`/`cli.rs`: clap's derived `Default`
/// zeroes every field — empty `report_path`, `log_path`, etc. — which is not the same as clap's
/// own `#[arg(default_value = ...)]` defaults, and used to send `logging::init("")` an empty path
/// before this module even got a chance to run.)
pub fn build_restore_args(
    original: &Args,
    report_path: &Path,
    target_override: Option<PathBuf>,
) -> Result<Args> {
    let content = std::fs::read_to_string(report_path).with_context(|| {
        format!(
            "cannot read backup report JSON from {}",
            report_path.display()
        )
    })?;
    let report: IngestReport = serde_json::from_str(&content)
        .with_context(|| format!("invalid report format in {}", report_path.display()))?;

    let restore_source = PathBuf::from(&report.dest);
    let restore_dest = target_override.unwrap_or_else(|| PathBuf::from(&report.source));

    println!("Initiating Restore Mode:");
    println!("  Restoring From: {}", restore_source.display());
    println!("  Restoring To  : {}", restore_dest.display());

    let mut args = original.clone();
    args.source = Some(restore_source);
    args.dest = Some(restore_dest);
    args.pattern = report.configuration.pattern;
    args.threads = report.configuration.threads;
    args.retries = report.configuration.retries;
    args.retry_wait_seconds = report.configuration.retry_wait_seconds;
    args.verify_integrity = report.configuration.verify_integrity;

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn restore_args_reverses_source_and_dest() {
        let json = r#"{
            "schema_version": 1,
            "timestamp": "2026-07-30T09:14:22Z",
            "tool_version": "1.0.0",
            "host_platform": "windows",
            "host_metadata": { "hostname": "srv", "os_name": "windows", "logical_cpus": 8 },
            "source": "D:/landing",
            "dest": "E:/warehouse",
            "total_files": 1,
            "total_bytes": 100,
            "robocopy_transfer": {
                "engine": "robocopy",
                "elapsed_seconds": 1.0,
                "throughput_mbps": 100.0,
                "bytes_copied": 100,
                "files_copied": 1,
                "retry_attempts_used": 0,
                "dry_run": false
            },
            "phase_timing": { "inventory_seconds": 0.1, "transfer_seconds": 1.0, "total_seconds": 1.1 },
            "configuration": {
                "threads": 8,
                "retries": 3,
                "retry_wait_seconds": 5,
                "pattern": "*.csv",
                "verify_integrity": true,
                "compare_baseline": false,
                "dry_run": false
            }
        }"#;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("report.json");
        std::fs::write(&path, json).expect("write");

        let original = Args::try_parse_from(["robocopy_ingest", "--restore-from", "report.json"])
            .expect("parse original invocation");
        let restore_args = build_restore_args(&original, &path, None).expect("restore args");
        assert_eq!(restore_args.source, Some(PathBuf::from("E:/warehouse")));
        assert_eq!(restore_args.dest, Some(PathBuf::from("D:/landing")));
        assert_eq!(restore_args.pattern, "*.csv");
        assert!(restore_args.verify_integrity);
    }

    /// Regression test for the bug the F25b black-box test caught: flags typed on the actual
    /// restore invocation (here `--decrypt`, plus a custom `--log-path`) used to be silently
    /// discarded because the old implementation built a brand new `Args` from scratch instead of
    /// starting from what the user actually typed.
    #[test]
    fn flags_from_the_real_invocation_survive_restore() {
        let json = r#"{
            "schema_version": 1,
            "timestamp": "2026-07-30T09:14:22Z",
            "tool_version": "1.0.0",
            "host_platform": "windows",
            "host_metadata": { "hostname": "srv", "os_name": "windows", "logical_cpus": 8 },
            "source": "D:/landing",
            "dest": "E:/warehouse",
            "total_files": 1,
            "total_bytes": 100,
            "robocopy_transfer": {
                "engine": "robocopy",
                "elapsed_seconds": 1.0,
                "throughput_mbps": 100.0,
                "bytes_copied": 100,
                "files_copied": 1,
                "retry_attempts_used": 0,
                "dry_run": false
            },
            "phase_timing": { "inventory_seconds": 0.1, "transfer_seconds": 1.0, "total_seconds": 1.1 },
            "configuration": {
                "threads": 8,
                "retries": 3,
                "retry_wait_seconds": 5,
                "pattern": "*.csv",
                "verify_integrity": true,
                "compare_baseline": false,
                "dry_run": false
            }
        }"#;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("report.json");
        std::fs::write(&path, json).expect("write");

        let original = Args::try_parse_from([
            "robocopy_ingest",
            "--restore-from",
            "report.json",
            "--decrypt",
            "my-secret-key",
            "--log-path",
            "custom-restore.log",
            "--webhook-url",
            "http://example.invalid/notify",
        ])
        .expect("parse original invocation");

        let restore_args = build_restore_args(&original, &path, None).expect("restore args");
        assert_eq!(restore_args.decrypt, Some("my-secret-key".to_string()));
        assert_eq!(restore_args.log_path, PathBuf::from("custom-restore.log"));
        assert_eq!(
            restore_args.webhook_url,
            Some("http://example.invalid/notify".to_string())
        );
        // Still correctly overridden from the report, same as before.
        assert_eq!(restore_args.source, Some(PathBuf::from("E:/warehouse")));
        assert_eq!(restore_args.dest, Some(PathBuf::from("D:/landing")));
    }
}
