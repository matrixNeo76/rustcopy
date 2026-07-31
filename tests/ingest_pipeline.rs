//! End-to-end tests of the portable parts of the pipeline: baseline copy, retry loop driven by a
//! mocked robocopy, integrity verification and JSON report. All of this runs on Linux.

use std::path::Path;
use std::time::Duration;

use clap::Parser;
use robocopy_ingest::cli::Args;
use robocopy_ingest::engine::naive::NaiveCopyEngine;
use robocopy_ingest::engine::robocopy::{self, RobocopyEngine};
use robocopy_ingest::engine::{run_with_retries, CopyEngine};
use robocopy_ingest::errors::IngestError;
use robocopy_ingest::integrity;
use robocopy_ingest::progress::{CountingProgress, ThroughputProgress};
use robocopy_ingest::report::IngestReport;
use robocopy_ingest::scan;
use robocopy_ingest::testkit::{fixture_tree, RecordingSleeper, ScriptedRunner};

fn args_for(source: &Path, dest: &Path, extra: &[&str]) -> Args {
    let mut argv = vec![
        "robocopy_ingest".to_string(),
        "--source".to_string(),
        source.to_string_lossy().into_owned(),
        "--dest".to_string(),
        dest.to_string_lossy().into_owned(),
    ];
    argv.extend(extra.iter().map(|s| s.to_string()));
    Args::try_parse_from(argv).expect("valid arguments")
}

/// Robocopy stdout for a run that copied `files`, rendered the way `/BYTES /NP` does.
fn robocopy_output(files: &[(&str, u64)]) -> Vec<String> {
    let total: u64 = files.iter().map(|(_, size)| size).sum();
    let mut lines = vec![
        "-------------------------------------------------------------------------------"
            .to_string(),
        "   ROBOCOPY     ::     Robust File Copy for Windows".to_string(),
        "  Started : Thursday, 30 July 2026 10:00:00".to_string(),
        "   Source : D:\\landing\\".to_string(),
        "     Dest : E:\\warehouse\\".to_string(),
        "    Files : *.csv".to_string(),
        "  Options : /BYTES /S /E /COPY:DAT /MT:8 /R:3 /W:5 /NP".to_string(),
    ];
    for (name, size) in files {
        lines.push(format!("\t    New File  \t\t{size:>12}\t{name}"));
    }
    lines
        .push("               Total    Copied   Skipped  Mismatch    FAILED    Extras".to_string());
    lines.push(format!(
        "   Files : {:>9} {:>9}         0         0         0         0",
        files.len(),
        files.len()
    ));
    lines.push(format!(
        "   Bytes : {total:>9} {total:>9}         0         0         0         0"
    ));
    lines
}

#[test]
fn baseline_copy_then_integrity_check_passes_and_report_is_written() {
    let source = fixture_tree(&[
        ("day1/a.csv", 200_000),
        ("day1/b.csv", 150_000),
        ("notes.txt", 10),
    ]);
    let dest = tempfile::tempdir().expect("dest");
    let report_dir = tempfile::tempdir().expect("report dir");
    let report_path = report_dir.path().join("report.json");

    let args = args_for(
        source.path(),
        dest.path(),
        &[
            "--pattern",
            "*.csv",
            "--verify-integrity",
            "--report-path",
            report_path.to_str().expect("utf8"),
        ],
    );

    let inventory = scan::scan(args.source(), &args.pattern).expect("scan");
    assert_eq!(inventory.file_count(), 2, "only CSV files are ingested");
    assert_eq!(inventory.total_bytes, 350_000);

    let progress = ThroughputProgress::hidden(inventory.total_bytes);
    let outcome = NaiveCopyEngine::new()
        .copy(&args.copy_request(args.dest().to_path_buf()), progress.as_ref())
        .expect("baseline copy");
    assert_eq!(outcome.files_copied, 2);
    assert_eq!(outcome.bytes_copied, 350_000);
    assert_eq!(progress.current_bytes(), 350_000);

    let check = integrity::verify(
        args.source(),
        args.dest(),
        &inventory.files,
        args.hash_algo,
        progress.as_ref(),
    );
    assert!(check.passed(), "checksums must match: {check:?}");

    let report = IngestReport::new(&args, &inventory, &outcome, None, Some(check));
    report.write_to(&report_path).expect("write report");

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    assert_eq!(json["total_files"], 2);
    assert_eq!(json["total_bytes"], 350_000);
    assert_eq!(json["integrity_check"]["status"], "PASSED");
    assert_eq!(json["configuration"]["pattern"], "*.csv");
}

#[test]
fn integrity_failure_is_reported_end_to_end() {
    let source = fixture_tree(&[("a.csv", 4096), ("b.csv", 4096)]);
    let dest = tempfile::tempdir().expect("dest");
    let args = args_for(source.path(), dest.path(), &["--verify-integrity"]);

    let inventory = scan::scan(args.source(), &args.pattern).expect("scan");
    let outcome = NaiveCopyEngine::new()
        .copy(
            &args.copy_request(args.dest().to_path_buf()),
            &CountingProgress::default(),
        )
        .expect("copy");

    // Simulate a silent corruption and a lost file in the destination.
    std::fs::write(dest.path().join("a.csv"), vec![7u8; 4096]).expect("corrupt");
    std::fs::remove_file(dest.path().join("b.csv")).expect("remove");

    let check = integrity::verify(
        args.source(),
        args.dest(),
        &inventory.files,
        args.hash_algo,
        &CountingProgress::default(),
    );
    assert!(!check.passed());

    let report = IngestReport::new(&args, &inventory, &outcome, None, Some(check));
    let json: serde_json::Value =
        serde_json::from_str(&report.to_json().expect("json")).expect("value");

    assert_eq!(json["integrity_check"]["status"], "FAILED");
    assert_eq!(json["integrity_check"]["mismatches"][0]["path"], "a.csv");
    assert_eq!(json["integrity_check"]["missing_in_dest"][0], "b.csv");
}

#[test]
fn mocked_robocopy_run_retries_then_produces_a_full_report() {
    let source = fixture_tree(&[("part-0001.csv", 1024), ("part-0002.csv", 2048)]);
    let dest = tempfile::tempdir().expect("dest");
    let args = args_for(
        source.path(),
        dest.path(),
        &[
            "--threads",
            "32",
            "--retries",
            "2",
            "--retry-wait-seconds",
            "4",
        ],
    );

    let inventory = scan::scan(args.source(), &args.pattern).expect("scan");

    // First attempt: exit code 9 (copied something, but some files failed) -> retried.
    // Second attempt: exit code 1 (files copied) -> success.
    let runner = ScriptedRunner::new(vec![
        (robocopy_output(&[("part-0001.csv", 1024)]), 9),
        (robocopy_output(&[("part-0002.csv", 2048)]), 1),
    ]);
    let recorded = runner.recorded();
    let engine = RobocopyEngine::with_runner(runner);
    let sleeper = RecordingSleeper::default();

    let outcome = run_with_retries(
        &engine,
        &args.copy_request(args.dest().to_path_buf()),
        &CountingProgress::default(),
        &args.retry_policy(),
        &sleeper,
    )
    .expect("second attempt succeeds");

    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(outcome.retry_attempts_used, 1);
    assert_eq!(sleeper.waits(), vec![Duration::from_secs(4)]);

    let invocations = recorded.lock().expect("lock");
    assert_eq!(invocations.len(), 2, "one invocation per attempt");
    let (program, flags) = &invocations[0];
    assert_eq!(program, robocopy::PROGRAM);
    assert!(flags.contains(&"/MT:32".to_string()));
    assert!(flags.contains(&"/R:2".to_string()));
    assert!(flags.contains(&"/W:4".to_string()));
    assert!(flags.contains(&"/BYTES".to_string()));

    let report = IngestReport::new(&args, &inventory, &outcome, None, None);
    let json: serde_json::Value =
        serde_json::from_str(&report.to_json().expect("json")).expect("value");
    assert_eq!(json["robocopy_transfer"]["exit_code"], 1);
    assert_eq!(json["robocopy_transfer"]["retry_attempts_used"], 1);
    assert_eq!(json["configuration"]["threads"], 32);
}

#[test]
fn mocked_robocopy_failure_is_reported_after_exhausting_retries() {
    let source = fixture_tree(&[("a.csv", 512)]);
    let dest = tempfile::tempdir().expect("dest");
    let args = args_for(
        source.path(),
        dest.path(),
        &["--retries", "1", "--retry-wait-seconds", "0"],
    );

    let engine = RobocopyEngine::with_runner(ScriptedRunner::new(vec![
        (robocopy_output(&[]), 8),
        (robocopy_output(&[]), 8),
    ]));

    let error = run_with_retries(
        &engine,
        &args.copy_request(args.dest().to_path_buf()),
        &CountingProgress::default(),
        &args.retry_policy(),
        &RecordingSleeper::default(),
    )
    .expect_err("both attempts fail");

    match error {
        IngestError::CopyFailed {
            code,
            attempts,
            description,
        } => {
            assert_eq!(code, 8);
            assert_eq!(attempts, 2);
            assert!(description.contains("could not be copied"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn robocopy_and_baseline_metrics_are_compared_in_the_report() {
    let source = fixture_tree(&[("a.csv", 1_000_000)]);
    let dest = tempfile::tempdir().expect("dest");
    let baseline_dest = tempfile::tempdir().expect("baseline dest");
    let args = args_for(source.path(), dest.path(), &["--compare-baseline"]);

    let inventory = scan::scan(args.source(), &args.pattern).expect("scan");

    let engine = RobocopyEngine::with_runner(ScriptedRunner::new(vec![(
        robocopy_output(&[("a.csv", 1_000_000)]),
        1,
    )]));
    let mut robocopy_outcome = engine
        .copy(
            &args.copy_request(args.dest().to_path_buf()),
            &CountingProgress::default(),
        )
        .expect("mocked robocopy");

    let mut baseline_outcome = NaiveCopyEngine::new()
        .copy(
            &args.copy_request(baseline_dest.path().to_path_buf()),
            &CountingProgress::default(),
        )
        .expect("baseline copy");

    // Pin the timings so the assertion is about the report maths, not about machine speed.
    robocopy_outcome.elapsed = Duration::from_secs(2);
    baseline_outcome.elapsed = Duration::from_secs(10);

    let report = IngestReport::new(
        &args,
        &inventory,
        &robocopy_outcome,
        Some(&baseline_outcome),
        None,
    );
    let json: serde_json::Value =
        serde_json::from_str(&report.to_json().expect("json")).expect("value");

    assert_eq!(json["robocopy_transfer"]["throughput_mbps"], 0.5);
    assert_eq!(json["baseline_transfer"]["engine"], "naive-baseline");
    assert_eq!(json["baseline_transfer"]["throughput_mbps"], 0.1);
    assert_eq!(json["speedup_factor"], 5.0);
    assert!(
        json["baseline_transfer"].get("exit_code").is_none(),
        "no process, no exit code"
    );
    assert!(report.human_summary().contains("5.00x"));
}

#[test]
fn dry_run_does_not_touch_the_destination() {
    let source = fixture_tree(&[("a.csv", 100), ("b.csv", 200)]);
    let dest_parent = tempfile::tempdir().expect("dest parent");
    let dest = dest_parent.path().join("untouched");
    let args = args_for(source.path(), &dest, &["--dry-run"]);

    let request = args.copy_request(args.dest().to_path_buf());
    assert!(request.dry_run);
    assert!(
        robocopy::build_args(&request).contains(&"/L".to_string()),
        "dry run must add robocopy's list-only flag"
    );

    let outcome = NaiveCopyEngine::new()
        .copy(&request, &CountingProgress::default())
        .expect("dry run");

    assert_eq!(outcome.files_copied, 2);
    assert_eq!(outcome.bytes_copied, 300);
    assert!(!dest.exists());
}

#[cfg(not(windows))]
#[test]
fn the_real_engine_reports_that_robocopy_needs_windows() {
    let source = fixture_tree(&[("a.csv", 10)]);
    let dest = tempfile::tempdir().expect("dest");
    let args = args_for(source.path(), dest.path(), &[]);

    let error = RobocopyEngine::new()
        .copy(
            &args.copy_request(args.dest().to_path_buf()),
            &CountingProgress::default(),
        )
        .expect_err("robocopy.exe cannot exist here");

    assert!(matches!(error, IngestError::RobocopyUnavailable));
    assert!(error.to_string().contains("Windows"));
    assert!(
        !error.is_transient(),
        "must not be retried on a non-Windows host"
    );
}
