//! Black-box tests of the compiled binary.

use std::path::Path;
use std::process::Command;

use robocopy_ingest::testkit::fixture_tree;

const BIN: &str = env!("CARGO_BIN_EXE_robocopy_ingest");

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().expect("binary runs")
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn help_documents_every_flag() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "--help must succeed");

    let help = stdout_of(&output);
    for flag in [
        "--source",
        "--dest",
        "--pattern",
        "--threads",
        "--retries",
        "--retry-wait-seconds",
        "--verify-integrity",
        "--compare-baseline",
        "--report-path",
        "--log-path",
        "--dry-run",
    ] {
        assert!(help.contains(flag), "{flag} missing from --help:\n{help}");
    }
    assert!(
        help.contains("/MT:N"),
        "help should mention the robocopy mapping"
    );
}

#[test]
fn missing_required_arguments_are_rejected() {
    let output = run(&[]);
    assert!(!output.status.success());
    assert!(stderr_of(&output).contains("--source"));
}

#[test]
fn a_missing_source_directory_is_reported_clearly() {
    let output = run(&["--source", "/definitely/not/here", "--dest", "/tmp/out"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "unrecoverable errors exit with 2"
    );
    assert!(stderr_of(&output).contains("source directory does not exist"));
}

#[test]
fn an_invalid_thread_count_is_reported_clearly() {
    let source = fixture_tree(&[("a.csv", 10)]);
    let dest = tempfile::tempdir().expect("dest");
    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.path().to_str().expect("utf8"),
        "--threads",
        "500",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_of(&output).contains("between 1 and 128"));
}

#[cfg(not(windows))]
#[test]
fn on_linux_the_run_scans_logs_and_then_explains_that_robocopy_needs_windows() {
    let source = fixture_tree(&[("day1/a.csv", 1024), ("day1/b.csv", 2048), ("skip.txt", 4)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("warehouse");
    let log_path = workdir.path().join("ingest.log");
    let report_path = workdir.path().join("report.json");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        log_path.to_str().expect("utf8"),
        "--report-path",
        report_path.to_str().expect("utf8"),
        "--retries",
        "2",
        "--retry-wait-seconds",
        "30",
    ]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "robocopy is unavailable here"
    );

    // The inventory phase runs before any copy, and it must respect the pattern.
    let stdout = stdout_of(&output);
    assert!(stdout.contains("2 file(s) matching *.csv"), "got: {stdout}");
    assert!(
        stdout.contains("3.07 KB"),
        "byte total missing from: {stdout}"
    );

    let stderr = stderr_of(&output);
    assert!(stderr.contains("Windows"), "got: {stderr}");

    assert!(log_path.is_file(), "the log file must be created");
    let log = std::fs::read_to_string(&log_path).expect("read log");
    assert!(log.contains("ingestion starting"));
    assert!(log.contains("source inventory complete"));
    assert!(log.contains("ERROR"), "the failure must be logged");

    // A missing robocopy.exe is not transient, so no backoff was spent on retries.
    assert!(
        !log.contains("retrying"),
        "must not retry a non-transient failure"
    );
    assert!(
        !report_path.exists(),
        "no report when the transfer cannot even start"
    );
}

#[cfg(not(windows))]
#[test]
fn an_empty_source_tree_warns_before_failing() {
    let source = fixture_tree(&[("notes.txt", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        workdir.path().join("out").to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
    ]);

    let stdout = stdout_of(&output);
    assert!(stdout.contains("no file matching *.csv"), "got: {stdout}");
}

#[test]
fn the_binary_reports_its_version() {
    let output = run(&["--version"]);
    assert!(output.status.success());
    assert!(stdout_of(&output).contains(env!("CARGO_PKG_VERSION")));
}

#[cfg(windows)]
#[test]
fn a_real_dry_run_succeeds_on_windows() {
    let source = fixture_tree(&[("a.csv", 1024)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let report_path = workdir.path().join("report.json");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        workdir.path().join("out").to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        report_path.to_str().expect("utf8"),
        "--dry-run",
    ]);

    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    assert!(report_path.is_file());
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    assert_eq!(report["configuration"]["dry_run"], true);
}

/// Guards the assumption the Linux tests rely on: the fixture helper is deterministic.
#[test]
fn fixtures_are_created_where_expected() {
    let dir = fixture_tree(&[("nested/a.csv", 32)]);
    let path: &Path = &dir.path().join("nested/a.csv");
    assert_eq!(std::fs::metadata(path).expect("metadata").len(), 32);
}
