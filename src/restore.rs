//! Disaster Recovery and Restore subsystem for robocopy-ingest-cli.
//!
//! Reverses backup requests by reading report JSON configurations and initiating
//! restored copy transfers from Destination back to Source with verification.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

use crate::cli::Args;
use crate::report::IngestReport;

/// Load a previous IngestReport JSON file and produce a restored Args request.
///
/// Built by parsing a minimal argv through clap (`Args::try_parse_from`) rather than
/// `Args::default()`: clap's derived `Default` zeroes every field (empty `report_path`,
/// `log_path`, etc.), which is not the same as clap's own `#[arg(default_value = ...)]`
/// defaults. Using `Args::default()` here used to send `logging::init("")` an empty path and
/// abort the restore run before it could start.
pub fn build_restore_args(report_path: &Path, target_override: Option<PathBuf>) -> Result<Args> {
    let content = std::fs::read_to_string(report_path)
        .with_context(|| format!("cannot read backup report JSON from {}", report_path.display()))?;
    let report: IngestReport = serde_json::from_str(&content)
        .with_context(|| format!("invalid report format in {}", report_path.display()))?;

    let restore_source = PathBuf::from(&report.dest);
    let restore_dest = target_override.unwrap_or_else(|| PathBuf::from(&report.source));

    println!("Initiating Restore Mode:");
    println!("  Restoring From: {}", restore_source.display());
    println!("  Restoring To  : {}", restore_dest.display());

    let mut args = Args::try_parse_from([
        "robocopy_ingest",
        "--source",
        &restore_source.to_string_lossy(),
        "--dest",
        &restore_dest.to_string_lossy(),
    ])
    .context("cannot build restore arguments")?;

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

        let restore_args = build_restore_args(&path, None).expect("restore args");
        assert_eq!(restore_args.source, Some(PathBuf::from("E:/warehouse")));
        assert_eq!(restore_args.dest, Some(PathBuf::from("D:/landing")));
        assert_eq!(restore_args.pattern, "*.csv");
        assert!(restore_args.verify_integrity);
    }
}
