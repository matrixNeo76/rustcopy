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

/// Backdates `path`'s last-write time by `days`, via a PowerShell one-liner (same "shell out for
/// a real OS-level effect" pattern already used elsewhere in this file, e.g. `mklink /J`). Used to
/// make a file old enough for `--max-age-days`/young enough for `--min-age-days` to exercise real
/// age filtering (D17) — since `scan.rs`'s prescan now applies these too (D17, closes the last
/// instance of the gap `--exclude-files`/`--exclude-dirs` had until 5 August 2026), this can no
/// longer be used as a "deterministic missing file" trick the way it briefly was: the prescan and
/// robocopy now agree on which files an age filter excludes, so a backdated+filtered file is
/// simply never expected at the destination, not reported as missing. See
/// `min_and_max_age_days_are_applied_consistently_end_to_end` for the real age-filter test, and
/// the two "missing at dest" tests below for the (unrelated) technique that replaced this one for
/// that purpose: a destination path segment that is a plain file where robocopy needs a directory.
#[cfg(windows)]
fn backdate_file(path: &Path, days: i64) {
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "(Get-Item -LiteralPath '{}').LastWriteTime = (Get-Date).AddDays(-{days})",
                path.display()
            ),
        ])
        .status()
        .expect("run powershell to backdate the file");
    assert!(status.success(), "failed to backdate {}", path.display());
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
        "--log-level",
        "--quiet",
        "--log-max-bytes",
        "--log-max-backups",
        "--exclude-junctions",
        "--ignore-transient-missing",
        "--hash-algo",
        "--fast-verify",
        "--vss-snapshot",
        "--resume-from",
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

/// F27/D18 black-box test (closes D9): the default log level (now INFO, D18) already omits the
/// per-file DEBUG lines that drove the multi-GB logs observed in the field on large trees (see
/// `ANALYSIS.md` D9/D18) — DEBUG is opt-in via `--log-level debug`, not the standing default.
/// `--quiet` goes further still, dropping INFO too (only warnings/errors survive).
#[cfg(windows)]
#[test]
fn log_level_controls_per_file_debug_detail_in_the_real_log() {
    let source = fixture_tree(&[("a.csv", 10), ("b.csv", 20)]);
    let workdir = tempfile::tempdir().expect("workdir");

    let run_with = |dest_name: &str, log_name: &str, extra: &[&str]| -> String {
        let dest = workdir.path().join(dest_name);
        let log_path = workdir.path().join(log_name);
        let mut argv = vec![
            "--source".to_string(),
            source.path().to_str().expect("utf8").to_string(),
            "--dest".to_string(),
            dest.to_str().expect("utf8").to_string(),
            "--log-path".to_string(),
            log_path.to_str().expect("utf8").to_string(),
            "--report-path".to_string(),
            workdir
                .path()
                .join(format!("{dest_name}-report.json"))
                .to_str()
                .expect("utf8")
                .to_string(),
        ];
        argv.extend(extra.iter().map(|s| s.to_string()));
        // RUST_LOG wins over the CLI-derived filter (logging.rs::build), so this test's
        // level-specific assertions would be at the mercy of whatever the *test runner's own*
        // environment happens to export -- cleared explicitly so this only ever exercises
        // --log-level/--quiet, not an ambient RUST_LOG.
        let output = Command::new(BIN)
            .args(&argv)
            .env_remove("RUST_LOG")
            .output()
            .expect("binary runs");
        assert!(output.status.success(), "stderr: {}", stderr_of(&output));
        std::fs::read_to_string(&log_path).expect("read log")
    };

    let default_log = run_with("out-default", "default.log", &[]);
    assert!(
        !default_log.contains("DEBUG"),
        "the new INFO default must not include per-file DEBUG detail; got: {default_log}"
    );
    assert!(
        default_log.contains("INFO"),
        "the default must still record INFO-level progress; got: {default_log}"
    );

    let debug_log = run_with("out-debug", "debug.log", &["--log-level", "debug"]);
    assert!(
        debug_log.contains("DEBUG"),
        "explicit --log-level debug must still record per-file detail; got: {debug_log}"
    );

    let quiet_log = run_with("out-quiet", "quiet.log", &["--quiet"]);
    assert!(
        !quiet_log.contains("DEBUG") && !quiet_log.contains("INFO"),
        "--quiet must suppress DEBUG and INFO alike, keeping only warnings/errors; got: {quiet_log}"
    );
}

/// F27 black-box test (closes D9): a real run against an already-oversized `--log-path`, driven
/// through the compiled binary, rotates the previous content aside instead of appending forever.
#[cfg(windows)]
#[test]
fn oversized_log_is_rotated_by_a_real_run() {
    let source = fixture_tree(&[("a.csv", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    let log_path = workdir.path().join("ingest.log");
    // D18: max_bytes must sit strictly between this run's own INFO-level output (a handful of
    // lines, but with full temp-dir paths embedded — comfortably under ~2 KB) and the seeded
    // content below, or the run's own output would itself cross the threshold and trigger a
    // *second*, mid-run rotation (D18) on top of the startup one this test actually checks —
    // exactly the miscalibration that broke this test when the log-level default changed.
    std::fs::write(&log_path, "x".repeat(20_000)).expect("seed an oversized log");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        log_path.to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report.json").to_str().expect("utf8"),
        "--log-max-bytes",
        "10000",
        "--log-max-backups",
        "2",
    ]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));

    let mut rotated_path = log_path.clone().into_os_string();
    rotated_path.push(".1");
    let rotated = std::fs::read_to_string(&rotated_path).expect("rotated backup must exist");
    assert_eq!(
        rotated,
        "x".repeat(20_000),
        "old content must be preserved in the rotated file"
    );

    let fresh = std::fs::read_to_string(&log_path).expect("fresh log must exist");
    assert!(
        fresh.contains("ingestion starting"),
        "the new run must log into the freshly-rotated file; got: {fresh}"
    );
}

/// F28 black-box test: on a second run against an unchanged source, `--fast-verify` skips
/// re-hashing every file (all size+mtime matches from the `.ingest_cache` written by the first
/// run), rather than always hashing everything as `--verify-integrity` alone does.
#[cfg(windows)]
#[test]
fn fast_verify_skips_unchanged_files_on_a_second_run() {
    let source = fixture_tree(&[("a.csv", 4096), ("b.csv", 8192)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");

    let base_args = |report: &Path| -> Vec<String> {
        vec![
            "--source".into(),
            source.path().to_str().expect("utf8").to_string(),
            "--dest".into(),
            dest.to_str().expect("utf8").to_string(),
            "--log-path".into(),
            workdir
                .path()
                .join("ingest.log")
                .to_str()
                .expect("utf8")
                .to_string(),
            "--report-path".into(),
            report.to_str().expect("utf8").to_string(),
            "--verify-integrity".into(),
            "--fast-verify".into(),
        ]
    };

    let report1 = workdir.path().join("report1.json");
    let owned1 = base_args(&report1);
    let argv1: Vec<&str> = owned1.iter().map(String::as_str).collect();
    let output1 = run(&argv1);
    assert!(output1.status.success(), "stderr: {}", stderr_of(&output1));
    let json1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report1).expect("read")).expect("json");
    assert_eq!(
        json1["integrity_check"]["skipped_unchanged"], 0,
        "nothing is cached yet on the first run"
    );
    assert_eq!(json1["integrity_check"]["files_checked"], 2);

    let report2 = workdir.path().join("report2.json");
    let owned2 = base_args(&report2);
    let argv2: Vec<&str> = owned2.iter().map(String::as_str).collect();
    let output2 = run(&argv2);
    assert!(output2.status.success(), "stderr: {}", stderr_of(&output2));
    let json2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report2).expect("read")).expect("json");
    assert_eq!(
        json2["integrity_check"]["skipped_unchanged"], 2,
        "both unchanged files must be skipped on the second run: {json2}"
    );
    assert_eq!(json2["integrity_check"]["files_checked"], 0);
    assert_eq!(json2["integrity_check"]["status"], "PASSED");
}

/// F28 black-box test: `--fast-verify` correctly re-checks only the file whose *source* changed
/// between two runs, still trusting the untouched one from the cache.
#[cfg(windows)]
#[test]
fn fast_verify_recatches_only_the_file_whose_source_changed() {
    let source = fixture_tree(&[("a.csv", 4096), ("b.csv", 8192)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");

    let base_args = |report: &Path| -> Vec<String> {
        vec![
            "--source".into(),
            source.path().to_str().expect("utf8").to_string(),
            "--dest".into(),
            dest.to_str().expect("utf8").to_string(),
            "--log-path".into(),
            workdir
                .path()
                .join("ingest.log")
                .to_str()
                .expect("utf8")
                .to_string(),
            "--report-path".into(),
            report.to_str().expect("utf8").to_string(),
            "--verify-integrity".into(),
            "--fast-verify".into(),
        ]
    };

    let report1 = workdir.path().join("report1.json");
    let owned1 = base_args(&report1);
    let argv1: Vec<&str> = owned1.iter().map(String::as_str).collect();
    assert!(run(&argv1).status.success());

    // Change only a.csv's content at the source (which also bumps its mtime).
    std::fs::write(
        source.path().join("a.csv"),
        b"changed,content,here\n1,2,3\n",
    )
    .expect("modify a.csv");

    let report2 = workdir.path().join("report2.json");
    let owned2 = base_args(&report2);
    let argv2: Vec<&str> = owned2.iter().map(String::as_str).collect();
    let output2 = run(&argv2);
    assert!(output2.status.success(), "stderr: {}", stderr_of(&output2));
    let json2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report2).expect("read")).expect("json");
    assert_eq!(
        json2["integrity_check"]["skipped_unchanged"], 1,
        "only b.csv (untouched) should be trusted from the cache: {json2}"
    );
    assert_eq!(
        json2["integrity_check"]["files_checked"], 1,
        "a.csv changed at the source and must be re-hashed: {json2}"
    );
    assert_eq!(json2["integrity_check"]["status"], "PASSED");
}

/// F28 black-box test: a file that fails verification must never be cached as trusted — otherwise
/// a genuinely broken file would silently stop being reported after the first run.
///
/// Neither `--exclude-files` (fixed 5 August 2026) nor `--max-age-days` (fixed 21 August 2026,
/// D17) can be used as a "deterministic missing file" trick any more, since `scan.rs`'s prescan
/// now applies both the same way robocopy's own transfer does — an excluded/age-filtered file is
/// no longer "missing", it's just never expected at the destination in the first place. This test
/// instead makes `sub/stale.log` genuinely unreachable via a real filesystem conflict: `dest/sub`
/// is pre-created as a plain **file**, so when robocopy tries to create it as a directory to hold
/// `stale.log`, that one file fails (robocopy reports it as a "mismatch", not a fatal error, and
/// continues with everything else — verified against the real binary: exit code 5 = `BIT_COPIED`
/// + `BIT_MISMATCH`, still `is_success()` per `exit_code.rs`). `a.csv` at the root is unaffected.
#[cfg(windows)]
#[test]
fn fast_verify_never_caches_a_failed_file_so_it_keeps_being_reported() {
    let source = fixture_tree(&[("a.csv", 10), ("sub/stale.log", 20)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    std::fs::create_dir_all(&dest).expect("create dest");
    std::fs::write(
        dest.join("sub"),
        b"blocks robocopy from creating sub/ as a directory",
    )
    .expect("pre-create the blocking file");

    let base_args = |report: &Path| -> Vec<String> {
        vec![
            "--source".into(),
            source.path().to_str().expect("utf8").to_string(),
            "--dest".into(),
            dest.to_str().expect("utf8").to_string(),
            "--log-path".into(),
            workdir
                .path()
                .join("ingest.log")
                .to_str()
                .expect("utf8")
                .to_string(),
            "--report-path".into(),
            report.to_str().expect("utf8").to_string(),
            "--verify-integrity".into(),
            "--fast-verify".into(),
        ]
    };

    let report1 = workdir.path().join("report1.json");
    let owned1 = base_args(&report1);
    let argv1: Vec<&str> = owned1.iter().map(String::as_str).collect();
    let output1 = run(&argv1);
    assert_eq!(
        output1.status.code(),
        Some(4),
        "stale.log can never land at dest (sub/ is blocked), so verification must fail; stderr: {}",
        stderr_of(&output1)
    );
    let json1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report1).expect("read")).expect("json");
    assert_eq!(
        json1["integrity_check"]["missing_in_dest"]
            .as_array()
            .expect("array")
            .len(),
        1
    );

    let report2 = workdir.path().join("report2.json");
    let owned2 = base_args(&report2);
    let argv2: Vec<&str> = owned2.iter().map(String::as_str).collect();
    let output2 = run(&argv2);
    assert_eq!(
        output2.status.code(),
        Some(4),
        "the same missing file must be reported again, not silently forgiven by a bad cache entry"
    );
    let json2: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report2).expect("read")).expect("json");
    assert_eq!(
        json2["integrity_check"]["skipped_unchanged"], 1,
        "only a.csv (which passed) should be trusted from the cache; stale.log must have been re-checked: {json2}"
    );
    assert_eq!(
        json2["integrity_check"]["missing_in_dest"]
            .as_array()
            .expect("array")
            .len(),
        1
    );
}

/// D17 black-box test: `--min-age-days`/`--max-age-days` must now be honoured by the prescan
/// (`scan.rs`) exactly the way real `robocopy.exe`'s own `/MINAGE`/`/MAXAGE` are — before this
/// fix, the prescan ignored both flags entirely, so `--verify-integrity` would report an
/// age-excluded file as spuriously `missing_in_dest` even though robocopy correctly skipped it on
/// purpose (this is the scenario the two tests above used to exploit as free test scaffolding).
/// Runs a real transfer with `--max-age-days`, so both the prescan's expectations and robocopy's
/// actual copy decision come from the same real binary, not a unit-level assumption about either.
#[cfg(windows)]
#[test]
fn min_and_max_age_days_are_applied_consistently_end_to_end() {
    let source = fixture_tree(&[("old.txt", 10), ("new.txt", 20)]);
    backdate_file(&source.path().join("old.txt"), 10);
    let workdir = tempfile::tempdir().expect("workdir");

    // --max-age-days 5: old.txt (10 days) must be excluded, new.txt (0 days) must survive. If the
    // prescan and robocopy agree (the D17 fix), --verify-integrity finds nothing missing.
    let dest = workdir.path().join("out");
    let report = workdir.path().join("report.json");
    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        report.to_str().expect("utf8"),
        "--max-age-days",
        "5",
        "--verify-integrity",
    ]);
    assert!(
        output.status.success(),
        "prescan and robocopy must agree on which files --max-age-days excludes; stderr: {}",
        stderr_of(&output)
    );
    assert!(dest.join("new.txt").is_file(), "new.txt must be copied");
    assert!(
        !dest.join("old.txt").exists(),
        "old.txt must be excluded by --max-age-days, not copied"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).expect("read")).expect("json");
    assert_eq!(
        json["integrity_check"]["missing_in_dest"]
            .as_array()
            .expect("array")
            .len(),
        0,
        "the prescan must not expect old.txt either, once --max-age-days is threaded into scan.rs: {json}"
    );

    // --min-age-days 5, the opposite direction: only old.txt (>= 5 days) survives, new.txt (0
    // days) is excluded.
    let dest2 = workdir.path().join("out2");
    let output2 = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest2.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest2.log").to_str().expect("utf8"),
        "--min-age-days",
        "5",
    ]);
    assert!(output2.status.success(), "stderr: {}", stderr_of(&output2));
    assert!(
        dest2.join("old.txt").is_file(),
        "old.txt (10 days) must survive --min-age-days 5"
    );
    assert!(
        !dest2.join("new.txt").exists(),
        "new.txt (0 days) must be excluded by --min-age-days 5"
    );
}

/// F31 black-box test (closes O5): a hand-written checkpoint — the exact shape `run()`'s Ctrl+C
/// branch writes — drives `--resume-from` through the compiled binary: source/dest/pattern are
/// reconstructed and the transfer actually runs, unlike unit-testing `build_resume_args` alone
/// (the F24/F25b lesson this project keeps re-learning: a unit test that skips clap and the real
/// binary can hide a wiring bug a black-box run would have caught immediately).
#[cfg(windows)]
#[test]
fn resume_from_reconstructs_and_runs_the_interrupted_invocation() {
    let source = fixture_tree(&[("a.csv", 10), ("b.csv", 20)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    let checkpoint_path = workdir.path().join("run.checkpoint.json");

    let checkpoint_json = format!(
        r#"{{
            "schema_version": 1,
            "timestamp": "2026-08-03T09:14:22Z",
            "source": {source:?},
            "dest": {dest:?},
            "configuration": {{
                "threads": 4,
                "retries": 3,
                "retry_wait_seconds": 5,
                "pattern": "*.csv",
                "verify_integrity": true,
                "compare_baseline": false,
                "dry_run": false
            }},
            "reason": "interrupted by Ctrl+C"
        }}"#,
        source = source.path().to_str().expect("utf8"),
        dest = dest.to_str().expect("utf8"),
    );
    std::fs::write(&checkpoint_path, checkpoint_json).expect("write checkpoint");

    let report_path = workdir.path().join("report.json");
    let output = run(&[
        "--resume-from",
        checkpoint_path.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("resume.log").to_str().expect("utf8"),
        "--report-path",
        report_path.to_str().expect("utf8"),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));

    assert!(dest.join("a.csv").is_file());
    assert!(dest.join("b.csv").is_file());
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    assert_eq!(report["configuration"]["pattern"], "*.csv");
    // The checkpoint's verify_integrity: true must have survived into the resumed run.
    assert_eq!(report["integrity_check"]["status"], "PASSED");
}

/// F31 black-box test: `--restore-from` and `--resume-from` together must be rejected at parse
/// time (they mean opposite things: reversed direction vs. same direction).
#[cfg(windows)]
#[test]
fn restore_from_and_resume_from_together_are_rejected_by_the_real_binary() {
    let output = run(&[
        "--restore-from",
        "report.json",
        "--resume-from",
        "checkpoint.json",
    ]);
    assert!(!output.status.success());
}

/// F36 black-box test: an `--install-schedule` value that doesn't match any accepted grammar
/// (`daily@HH:MM` / `hourly@N` / `weekly@DAY,...@HH:MM`) must be rejected with a clear error,
/// before ever touching `schtasks.exe` — this must work identically on every platform since the
/// rejection happens in pure parsing.
#[test]
fn install_schedule_with_an_invalid_spec_is_rejected_by_the_real_binary() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let dest = tempfile::tempdir().expect("dest");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.path().to_str().expect("utf8"),
        "--install-schedule",
        "monthly@1",
    ]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        stderr_of(&output).contains("invalid --install-schedule spec"),
        "stderr: {}",
        stderr_of(&output)
    );
}

/// F36 black-box test: `--install-schedule` and `--uninstall-schedule` together must be rejected
/// by clap itself — they mean opposite things (create vs. remove a scheduled task).
#[test]
fn install_schedule_and_uninstall_schedule_together_are_rejected() {
    let output = run(&[
        "--install-schedule",
        "daily@02:00",
        "--uninstall-schedule",
        "somejob",
    ]);
    assert!(!output.status.success());
}

/// F37 black-box test: `--install-service` and `--uninstall-service` together must be rejected by
/// clap — they mean opposite things (register vs. remove the Windows service).
#[test]
fn install_service_and_uninstall_service_together_are_rejected() {
    let output = run(&["--install-service", "--uninstall-service"]);
    assert!(!output.status.success());
}

/// F37 black-box test: `--install-service`/`--uninstall-service` don't require --source/--dest —
/// confirms the clap `required_unless_present_any` exemption and `validate()`'s early return
/// actually work, without needing the real Administrator elevation a genuine
/// `CreateService`/`DeleteService` round trip would require (see `service.rs`'s doc comment for
/// why that round trip isn't covered by an automated test here).
#[test]
fn install_service_does_not_require_source_or_dest() {
    let output = run(&["--install-service"]);
    assert!(
        !stderr_of(&output).contains("--source and --dest must be set"),
        "stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn uninstall_service_does_not_require_source_or_dest() {
    let output = run(&["--uninstall-service"]);
    assert!(
        !stderr_of(&output).contains("--source and --dest must be set"),
        "stderr: {}",
        stderr_of(&output)
    );
}

/// Deletes the named Windows service on drop, best-effort — cleans up after the elevated
/// round-trip test below even if an assertion panics partway through.
#[cfg(windows)]
struct WindowsServiceGuard(&'static str);

#[cfg(windows)]
impl Drop for WindowsServiceGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("sc.exe")
            .args(["delete", self.0])
            .output();
    }
}

/// F37 elevated round-trip test: `--install-service` actually registers `RustcopyIngestService`
/// with the real Service Control Manager, and `--uninstall-service` actually removes it — a real
/// round trip, not a mock. **Requires Administrator elevation** (`CreateService`/`DeleteService`
/// need it) and mutates real machine state (the Windows service database), so it is marked
/// `#[ignore]` and never runs as part of the normal `cargo test` suite — this is exactly the
/// limitation declared in `service.rs`'s doc comment and `ROADMAP.md`'s F37 row. Run it manually
/// from an elevated prompt with:
///   cargo test --test cli_smoke -- --ignored install_and_uninstall_service_round_trip
#[cfg(windows)]
#[test]
#[ignore = "requires Administrator elevation; run manually with `cargo test -- --ignored`"]
fn install_and_uninstall_service_round_trip() {
    const SERVICE_NAME: &str = "RustcopyIngestService";
    let _guard = WindowsServiceGuard(SERVICE_NAME);

    let install_output = run(&["--install-service"]);
    assert!(
        install_output.status.success(),
        "install must succeed when run elevated; stderr: {}",
        stderr_of(&install_output)
    );

    let query = std::process::Command::new("sc.exe")
        .args(["query", SERVICE_NAME])
        .output()
        .expect("run sc.exe query");
    assert!(
        query.status.success(),
        "sc query must find the freshly installed service; stderr: {}",
        String::from_utf8_lossy(&query.stderr)
    );

    let uninstall_output = run(&["--uninstall-service"]);
    assert!(
        uninstall_output.status.success(),
        "uninstall must succeed when run elevated; stderr: {}",
        stderr_of(&uninstall_output)
    );

    let query_after = std::process::Command::new("sc.exe")
        .args(["query", SERVICE_NAME])
        .output()
        .expect("run sc.exe query");
    assert!(
        !query_after.status.success(),
        "sc query must no longer find the service after uninstall"
    );
}

/// Deletes the named Task Scheduler entry on drop, best-effort — cleans up after the round-trip
/// test below even if an assertion panics partway through.
#[cfg(windows)]
struct ScheduledTaskGuard(String);

#[cfg(windows)]
impl Drop for ScheduledTaskGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("schtasks.exe")
            .args(["/Delete", "/TN", &self.0, "/F"])
            .output();
    }
}

/// F36 black-box test: `--install-schedule` actually creates a real Task Scheduler entry via
/// `schtasks.exe` (not just parses the flag and does nothing), and `--uninstall-schedule` removes
/// it again — a real round trip against the real Windows Task Scheduler, not a mock.
#[cfg(windows)]
#[test]
fn install_and_uninstall_schedule_round_trip_via_real_schtasks() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let dest = tempfile::tempdir().expect("dest");
    let task_name = format!("rustcopy-cli-smoke-test-{}", std::process::id());
    let _guard = ScheduledTaskGuard(task_name.clone());

    let install_output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.path().to_str().expect("utf8"),
        "--install-schedule",
        "daily@03:00",
        "--schedule-name",
        &task_name,
    ]);
    assert!(
        install_output.status.success(),
        "stderr: {}",
        stderr_of(&install_output)
    );
    assert!(
        stdout_of(&install_output).contains(&task_name),
        "stdout: {}",
        stdout_of(&install_output)
    );

    let uninstall_output = run(&["--uninstall-schedule", &task_name]);
    assert!(
        uninstall_output.status.success(),
        "stderr: {}",
        stderr_of(&uninstall_output)
    );
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

    // The inventory phase runs before any copy, and it must respect the pattern. The default
    // pattern is "*" (not "*.csv" — changed long before this test last actually ran, since it's
    // #[cfg(not(windows))] and this is the first time CI has run the suite on Linux), so all 3
    // fixture files match, not just the 2 .csv ones.
    let stdout = stdout_of(&output);
    assert!(stdout.contains("3 file(s) matching *"), "got: {stdout}");
    assert!(
        stdout.contains("3.08 KB"),
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
    // The default pattern is "*" (matches everything), so the source tree must be genuinely
    // empty — a single non-matching file used to be enough back when the default was "*.csv", but
    // this test is #[cfg(not(windows))] and this is the first time CI has ever run it on Linux, so
    // that stale assumption was never caught until now.
    let source = fixture_tree(&[]);
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
    assert!(stdout.contains("no file matching * found"), "got: {stdout}");
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

#[test]
fn mirror_without_force_purge_aborts_instead_of_deleting_extraneous_files() {
    let source = fixture_tree(&[("a.csv", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    std::fs::create_dir_all(&dest).expect("create dest");
    let extraneous = dest.join("do-not-delete-me.csv");
    std::fs::write(&extraneous, b"precious data").expect("seed dest");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report.json").to_str().expect("utf8"),
        "--mirror",
    ]);

    assert_eq!(
        output.status.code(),
        Some(3),
        "mirror-purge-aborted must exit with the dedicated code; stderr: {}",
        stderr_of(&output)
    );
    assert!(
        extraneous.exists(),
        "the file must not have been purged when the run was aborted"
    );
    assert!(stderr_of(&output).contains("force-purge"));
}

#[test]
fn mirror_with_force_purge_proceeds() {
    let source = fixture_tree(&[("a.csv", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    std::fs::create_dir_all(&dest).expect("create dest");
    std::fs::write(dest.join("stale.csv"), b"old data").expect("seed dest");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report.json").to_str().expect("utf8"),
        "--mirror",
        "--force-purge",
    ]);

    assert_ne!(
        output.status.code(),
        Some(3),
        "--force-purge must bypass the mirror safety abort; stderr: {}",
        stderr_of(&output)
    );
}

#[cfg(windows)]
#[test]
fn encrypt_aes256_actually_encrypts_destination_files() {
    let source = fixture_tree(&[("a.csv", 1024)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report.json").to_str().expect("utf8"),
        "--encrypt-aes256",
        "test-passphrase",
    ]);

    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    let encrypted = std::fs::read(dest.join("a.csv")).expect("read encrypted file");
    let plaintext = std::fs::read(source.path().join("a.csv")).expect("read source file");
    assert_ne!(
        encrypted, plaintext,
        "destination content must not be the plaintext"
    );

    let manager =
        robocopy_ingest::crypto::CryptoManager::new("test-passphrase").expect("build manager");
    let mut decrypted = Vec::new();
    manager
        .decrypt_stream(std::io::Cursor::new(&encrypted), &mut decrypted)
        .expect("decrypt with the right key");
    assert_eq!(
        decrypted, plaintext,
        "must decrypt back to the original bytes"
    );
}

#[cfg(windows)]
#[test]
fn permanently_uncopyable_file_does_not_undercount_the_rest_of_the_transfer() {
    let source = fixture_tree(&[("a.csv", 10), ("b.csv", 20), ("c.csv", 30)]);

    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    std::fs::create_dir_all(&dest).expect("create dest");
    // Pre-create b.csv at the destination and hold it open with share_mode(0) (deny all
    // sharing): as long as this handle lives, robocopy gets "used by another process" on every
    // single retry attempt for exactly this file, while a.csv and c.csv copy normally. This must
    // not make the report undercount the files that genuinely made it to the destination.
    use std::os::windows::fs::OpenOptionsExt;
    let _locked_handle = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .share_mode(0)
        .open(dest.join("b.csv"))
        .expect("lock b.csv exclusively");
    let report_path = workdir.path().join("report.json");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        report_path.to_str().expect("utf8"),
        "--retries",
        "1",
        "--retry-wait-seconds",
        "0",
    ]);

    // F29b: exit 1 means the transfer itself failed (robocopy couldn't copy everything), distinct
    // from exit 4 (EXIT_INTEGRITY_FAILED, see ignore_transient_missing_turns_an_excluded_log_into_a_pass
    // below) which means the transfer succeeded but --verify-integrity found a problem afterwards.
    assert_eq!(
        output.status.code(),
        Some(1),
        "some items could not be copied"
    );
    assert!(report_path.is_file());
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    // b.csv itself exists at the destination (as the pre-created, still-locked 0-byte stub), so
    // all 3 destination entries are present — but its *content* was never overwritten by
    // robocopy (locked the whole time), so only a.csv (10B) + c.csv (30B) = 40B are real,
    // correctly-copied bytes. Before the fix, this failure path reported whatever the *last*
    // retry attempt's reset progress sink happened to hold (near-zero for a run this short),
    // not a real reflection of what's actually sitting on disk.
    let bytes_copied = report["robocopy_transfer"]["bytes_copied"]
        .as_u64()
        .expect("bytes_copied");
    assert_eq!(
        bytes_copied, 40,
        "a.csv (10B) + c.csv (30B) must be counted from a real destination scan, not the last retry's leftover sink state; report: {report}"
    );
}

#[cfg(windows)]
#[test]
fn unreachable_webhook_does_not_fail_the_backup() {
    let source = fixture_tree(&[("a.csv", 10)]);
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
        // Port 0 is never a valid connection target: the webhook delivery must fail, but the
        // ingestion itself must still complete successfully (a missing/unreachable notify-server
        // must never take down the backup it is only supposed to report on).
        "--webhook-url",
        "http://127.0.0.1:0/notify",
    ]);

    assert!(
        output.status.success(),
        "an unreachable webhook must not fail the backup; stderr: {}",
        stderr_of(&output)
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    assert!(
        report["webhook_error"].is_string(),
        "webhook_error must be populated when delivery fails; report: {report}"
    );
}

/// F39 black-box test: a failing `--pre-command` aborts the run before anything is copied — no
/// destination directory, no report, no robocopy invocation at all.
#[test]
fn pre_command_failure_aborts_the_run_before_any_copy() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report.json").to_str().expect("utf8"),
        "--pre-command",
        "exit 7",
    ]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        stderr_of(&output).contains("pre-command"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        !dest.exists(),
        "nothing should have been copied or even a destination dir created"
    );
}

/// F39 black-box test: `--post-command` failing must NOT fail an otherwise-successful backup —
/// the backup already succeeded by the time the post-command runs — but the failure must be
/// recorded in the report for the operator to notice.
#[cfg(windows)]
#[test]
fn post_command_failure_does_not_fail_the_run_but_is_recorded_in_the_report() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");
    let report_path = workdir.path().join("report.json");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        report_path.to_str().expect("utf8"),
        "--post-command",
        "exit 9",
    ]);

    assert!(
        output.status.success(),
        "a failed post-command must not fail the run; stderr: {}",
        stderr_of(&output)
    );
    assert!(
        dest.join("a.csv").is_file(),
        "the file must still have been copied"
    );

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    let error = report["post_command_error"]
        .as_str()
        .expect("post_command_error must be recorded");
    assert!(error.contains('9'), "error: {error}");
}

/// F39 black-box test: successful `--pre-command`/`--post-command` both actually run around the
/// backup (not just get parsed and ignored) and leave the report's error fields empty.
#[cfg(windows)]
#[test]
fn pre_and_post_commands_run_successfully_around_the_backup() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");
    let report_path = workdir.path().join("report.json");
    let pre_marker = workdir.path().join("pre-ran.txt");
    let post_marker = workdir.path().join("post-ran.txt");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        report_path.to_str().expect("utf8"),
        "--pre-command",
        // No spaces in a tempdir path, so no quoting needed — quoting a path already containing
        // a `/C`-nested command string trips over cmd.exe's own quirky quote-stripping rules.
        &format!("echo hi > {}", pre_marker.display()),
        "--post-command",
        &format!("echo hi > {}", post_marker.display()),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    assert!(
        pre_marker.is_file(),
        "the pre-command must actually have run"
    );
    assert!(
        post_marker.is_file(),
        "the post-command must actually have run"
    );

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    assert!(
        report.get("post_command_error").is_none(),
        "report: {report}"
    );
}

#[cfg(windows)]
#[test]
fn restore_from_runs_end_to_end_without_source_or_dest() {
    // F24 black-box test: everything lives under one tempdir, so nothing outside this test's own
    // sandbox can ever be touched (no real user paths, no network shares — see D1/F24 in
    // ANALYSIS.md/ROADMAP.md for why this is exactly the kind of scenario a unit test parsing
    // clap args in isolation cannot catch: the previous "fix" passed its own test while the real
    // binary remained unusable).
    let sandbox = tempfile::tempdir().expect("sandbox");
    let original = sandbox.path().join("original");
    let backup_dest = sandbox.path().join("backup");
    let report_path = sandbox.path().join("backup_report.json");

    // 1. Seed the "original" data and back it up to "backup" (a normal, forward run).
    std::fs::create_dir_all(&original).expect("create original");
    std::fs::write(original.join("important.csv"), b"irreplaceable,data\n1,2\n")
        .expect("seed file");

    let backup_output = run(&[
        "--source",
        original.to_str().expect("utf8"),
        "--dest",
        backup_dest.to_str().expect("utf8"),
        "--verify-integrity",
        "--report-path",
        report_path.to_str().expect("utf8"),
        "--log-path",
        sandbox.path().join("backup.log").to_str().expect("utf8"),
    ]);
    assert!(
        backup_output.status.success(),
        "seed backup must succeed; stderr: {}",
        stderr_of(&backup_output)
    );
    assert!(backup_dest.join("important.csv").is_file());

    // 2. Simulate data loss: remove the file from "original".
    std::fs::remove_file(original.join("important.csv")).expect("simulate data loss");
    assert!(!original.join("important.csv").exists());

    // 3. Restore mode: `--restore-from` alone, with NEITHER `--source` NOR `--dest` on the
    // command line. This is the exact invocation the README documents and the one that used to
    // fail immediately with "a value is required for '--source <PATH>'" before clap ever got to
    // read the report — the whole point of this test is that the compiled binary, not just
    // clap's own arg parser in isolation, accepts and executes this.
    let restore_output = run(&[
        "--restore-from",
        report_path.to_str().expect("utf8"),
        "--log-path",
        sandbox.path().join("restore.log").to_str().expect("utf8"),
        "--report-path",
        sandbox
            .path()
            .join("restore_report.json")
            .to_str()
            .expect("utf8"),
    ]);
    assert!(
        restore_output.status.success(),
        "restore must succeed; stderr: {}",
        stderr_of(&restore_output)
    );

    // 4. The "lost" file must be back, with its original content, restored FROM the backup
    // destination BACK TO the original source path (the direction is reversed, by design).
    let restored_content =
        std::fs::read_to_string(original.join("important.csv")).expect("file must be restored");
    assert_eq!(restored_content, "irreplaceable,data\n1,2\n");
}

#[cfg(windows)]
#[test]
fn encrypted_backup_restores_and_decrypts_end_to_end() {
    // F25b black-box test: the full disaster-recovery story for an encrypted backup, all inside
    // one tempdir sandbox (no real user paths touched). Encrypting a backup that can never be
    // decrypted again is exactly the failure mode D4 described — this proves the whole loop
    // (encrypt on backup -> data loss -> --restore-from + --decrypt) produces the original
    // plaintext back, using the compiled binary end-to-end, not CryptoManager calls in isolation.
    let sandbox = tempfile::tempdir().expect("sandbox");
    let original = sandbox.path().join("original");
    let backup_dest = sandbox.path().join("backup");
    let report_path = sandbox.path().join("backup_report.json");
    let key = "correct-horse-battery-staple";

    // 1. Seed data and back it up WITH encryption.
    std::fs::create_dir_all(&original).expect("create original");
    std::fs::write(original.join("secret.csv"), b"classified,data\n42,99\n").expect("seed file");

    let backup_output = run(&[
        "--source",
        original.to_str().expect("utf8"),
        "--dest",
        backup_dest.to_str().expect("utf8"),
        "--verify-integrity",
        "--encrypt-aes256",
        key,
        "--report-path",
        report_path.to_str().expect("utf8"),
        "--log-path",
        sandbox.path().join("backup.log").to_str().expect("utf8"),
    ]);
    assert!(
        backup_output.status.success(),
        "encrypted backup must succeed; stderr: {}",
        stderr_of(&backup_output)
    );
    let encrypted_at_dest = std::fs::read(backup_dest.join("secret.csv")).expect("read backup");
    assert_ne!(
        encrypted_at_dest, b"classified,data\n42,99\n",
        "the backup destination must hold ciphertext, not plaintext"
    );

    // 2. Simulate data loss.
    std::fs::remove_file(original.join("secret.csv")).expect("simulate data loss");
    assert!(!original.join("secret.csv").exists());

    // 3. Restore AND decrypt in one command: --restore-from (no --source/--dest, per F24) plus
    // --decrypt with the same key.
    let restore_output = run(&[
        "--restore-from",
        report_path.to_str().expect("utf8"),
        "--decrypt",
        key,
        "--log-path",
        sandbox.path().join("restore.log").to_str().expect("utf8"),
        "--report-path",
        sandbox
            .path()
            .join("restore_report.json")
            .to_str()
            .expect("utf8"),
    ]);
    assert!(
        restore_output.status.success(),
        "restore+decrypt must succeed; stderr: {}",
        stderr_of(&restore_output)
    );

    // 4. The recovered file must be back in PLAINTEXT (decrypted), not still ciphertext.
    let recovered = std::fs::read(original.join("secret.csv")).expect("file must be restored");
    assert_eq!(
        recovered, b"classified,data\n42,99\n",
        "recovered file must be the original plaintext, not ciphertext"
    );
}

#[test]
fn encrypt_and_decrypt_together_are_rejected() {
    let source = fixture_tree(&[("a.csv", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        workdir.path().join("out").to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--encrypt-aes256",
        "key-a",
        "--decrypt",
        "key-b",
    ]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "must be an unrecoverable usage error"
    );
    assert!(
        stderr_of(&output).contains("cannot both be given"),
        "got: {}",
        stderr_of(&output)
    );
}

/// F29a black-box test: `--hash-algo xxh3` runs end-to-end through the compiled binary (real
/// robocopy transfer + real verification pass), not just the `xxh3_file`/`verify` unit tests.
#[cfg(windows)]
#[test]
fn hash_algo_xxh3_verifies_successfully_end_to_end() {
    let source = fixture_tree(&[("a.csv", 4096), ("nested/b.csv", 8192)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    let report_path = workdir.path().join("report.json");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        report_path.to_str().expect("utf8"),
        "--verify-integrity",
        "--hash-algo",
        "xxh3",
    ]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    assert_eq!(report["integrity_check"]["status"], "PASSED");
    assert_eq!(report["integrity_check"]["files_checked"], 2);
}

/// F26a black-box test (closes half of D2): a file matched by the pattern but genuinely unable to
/// land at the destination is a deterministic, non-racy way to reproduce "file present in the
/// prescan inventory but missing at the destination when `--verify-integrity` runs" — exactly the
/// scenario `--ignore-transient-missing` exists for, without needing a real timing race. Uses the
/// same `dest/sub`-is-a-file filesystem conflict as
/// `fast_verify_never_caches_a_failed_file_so_it_keeps_being_reported` — see that test's doc
/// comment for why `--max-age-days` can no longer be used for this (D17 fixed `scan.rs`'s prescan
/// to apply it too, so it no longer produces a prescan/destination mismatch).
#[cfg(windows)]
#[test]
fn ignore_transient_missing_turns_an_excluded_log_into_a_pass() {
    let source = fixture_tree(&[("a.csv", 10), ("sub/stale.log", 20)]);
    let workdir = tempfile::tempdir().expect("workdir");

    // Without --ignore-transient-missing: stale.log is in the prescan inventory (matched by the
    // default "*" pattern) but can never reach the destination (sub/ is blocked), so verification
    // must fail.
    let dest_a = workdir.path().join("out-a");
    std::fs::create_dir_all(&dest_a).expect("create dest_a");
    std::fs::write(
        dest_a.join("sub"),
        b"blocks robocopy from creating sub/ as a directory",
    )
    .expect("pre-create the blocking file");
    let output_a = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest_a.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("a.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("a-report.json").to_str().expect("utf8"),
        "--verify-integrity",
    ]);
    assert_eq!(
        output_a.status.code(),
        Some(4),
        "stale.log missing at dest must fail verification (F29b: exit 4, integrity-only failure) \
         without --ignore-transient-missing; stderr: {}",
        stderr_of(&output_a)
    );

    // With --ignore-transient-missing: the same missing stale.log must be tolerated.
    let dest_b = workdir.path().join("out-b");
    std::fs::create_dir_all(&dest_b).expect("create dest_b");
    std::fs::write(
        dest_b.join("sub"),
        b"blocks robocopy from creating sub/ as a directory",
    )
    .expect("pre-create the blocking file");
    let report_b = workdir.path().join("b-report.json");
    let output_b = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest_b.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("b.log").to_str().expect("utf8"),
        "--report-path",
        report_b.to_str().expect("utf8"),
        "--verify-integrity",
        "--ignore-transient-missing",
    ]);
    assert!(
        output_b.status.success(),
        "--ignore-transient-missing must turn the same failure into a pass; stderr: {}",
        stderr_of(&output_b)
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_b).expect("read")).expect("json");
    assert_eq!(report["integrity_check"]["status"], "PASSED");
    assert_eq!(
        report["integrity_check"]["missing_in_dest"].as_array().expect("array").len(),
        0,
        "stale.log must be filtered out of missing_in_dest, not just ignored for status; report: {report}"
    );
}

/// F26d black-box test (closes D7): before this fix, `scan.rs` never followed junctions
/// (`follow_links(false)` unconditionally) while robocopy followed them by default (no `/XJ`
/// ever passed) — the prescan and the actual transfer walked different trees. Uses a real NTFS
/// directory junction (`mklink /J`, which needs no elevated privilege, unlike symlinks) so this
/// exercises the real compiled binary end-to-end, not just `scan::scan` in isolation.
#[cfg(windows)]
#[test]
fn exclude_junctions_flag_actually_changes_what_the_binary_copies() {
    let source = fixture_tree(&[("real/a.csv", 10)]);
    let target = source.path().join("real");
    let link = source.path().join("link");
    let status = std::process::Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &link.display().to_string(),
            &target.display().to_string(),
        ])
        .status()
        .expect("run mklink");
    assert!(
        status.success(),
        "mklink /J must succeed to exercise this test"
    );

    let workdir = tempfile::tempdir().expect("workdir");

    // Default (no --exclude-junctions): robocopy's own default is to follow the junction, and the
    // prescan now agrees, so a.csv is copied twice (once directly, once through the junction).
    let dest_default = workdir.path().join("out-default");
    let report_default = workdir.path().join("default-report.json");
    let output_default = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest_default.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("default.log").to_str().expect("utf8"),
        "--report-path",
        report_default.to_str().expect("utf8"),
    ]);
    assert!(
        output_default.status.success(),
        "stderr: {}",
        stderr_of(&output_default)
    );
    assert!(dest_default.join("real").join("a.csv").is_file());
    assert!(
        dest_default.join("link").join("a.csv").is_file(),
        "without --exclude-junctions, robocopy must follow the junction like it does by default"
    );
    let report_default: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_default).expect("read"))
            .expect("json");
    assert_eq!(
        report_default["total_files"], 2,
        "the prescan must agree with what robocopy actually copies"
    );

    // --exclude-junctions: /XJ is passed, robocopy must not descend into the junction, and the
    // prescan must agree (no more "1 counted, 2 copied" mismatch).
    let dest_excl = workdir.path().join("out-excl");
    let report_excl = workdir.path().join("excl-report.json");
    let output_excl = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest_excl.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("excl.log").to_str().expect("utf8"),
        "--report-path",
        report_excl.to_str().expect("utf8"),
        "--exclude-junctions",
    ]);
    assert!(
        output_excl.status.success(),
        "stderr: {}",
        stderr_of(&output_excl)
    );
    assert!(dest_excl.join("real").join("a.csv").is_file());
    assert!(
        !dest_excl.join("link").exists(),
        "--exclude-junctions must stop robocopy from descending into (or even creating) the junction"
    );
    let report_excl: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_excl).expect("read")).expect("json");
    assert_eq!(report_excl["total_files"], 1);
}

/// F26c black-box test (closes D6): a hand-written "legacy" report — the exact shape a report
/// written before the `Mismatch` field rename would have (only `path`, no `kind`/`algorithm`/
/// `source_digest`/`dest_digest`) — must still drive `--restore-from` through the compiled binary
/// instead of failing at the JSON-parsing step inside `build_restore_args`.
#[cfg(windows)]
#[test]
fn restore_from_accepts_a_legacy_report_with_pre_rename_mismatch_shape() {
    let sandbox = tempfile::tempdir().expect("sandbox");
    let original = sandbox.path().join("original");
    let backup_dest = sandbox.path().join("backup");
    let report_path = sandbox.path().join("legacy_report.json");

    std::fs::create_dir_all(&backup_dest).expect("create backup dest");
    std::fs::write(
        backup_dest.join("important.csv"),
        b"irreplaceable,data\n1,2\n",
    )
    .expect("seed backup file");

    let legacy_json = format!(
        r#"{{
            "schema_version": 1,
            "timestamp": "2026-07-30T09:14:22Z",
            "tool_version": "1.0.0",
            "host_platform": "windows",
            "host_metadata": {{ "hostname": "srv", "os_name": "windows", "logical_cpus": 8 }},
            "source": {source:?},
            "dest": {dest:?},
            "total_files": 1,
            "total_bytes": 25,
            "robocopy_transfer": {{
                "engine": "robocopy",
                "elapsed_seconds": 1.0,
                "throughput_mbps": 1.0,
                "bytes_copied": 25,
                "files_copied": 1,
                "retry_attempts_used": 0,
                "dry_run": false
            }},
            "phase_timing": {{ "inventory_seconds": 0.1, "transfer_seconds": 1.0, "total_seconds": 1.1 }},
            "configuration": {{
                "threads": 8,
                "retries": 3,
                "retry_wait_seconds": 5,
                "pattern": "*",
                "verify_integrity": false,
                "compare_baseline": false,
                "dry_run": false
            }},
            "integrity_check": {{
                "files_checked": 1,
                "bytes_hashed": 25,
                "mismatches": [ {{ "path": "important.csv" }} ],
                "missing_in_dest": [],
                "unreadable": [],
                "status": "FAILED"
            }}
        }}"#,
        source = original.to_str().expect("utf8"),
        dest = backup_dest.to_str().expect("utf8"),
    );
    std::fs::write(&report_path, legacy_json).expect("write legacy report");

    let restore_output = run(&[
        "--restore-from",
        report_path.to_str().expect("utf8"),
        "--log-path",
        sandbox.path().join("restore.log").to_str().expect("utf8"),
        "--report-path",
        sandbox
            .path()
            .join("restore_report.json")
            .to_str()
            .expect("utf8"),
    ]);
    assert!(
        restore_output.status.success(),
        "a pre-rename Mismatch shape must not break --restore-from; stderr: {}",
        stderr_of(&restore_output)
    );
    let restored =
        std::fs::read_to_string(original.join("important.csv")).expect("file must be restored");
    assert_eq!(restored, "irreplaceable,data\n1,2\n");
}

/// F33 black-box test: a `[[jobs]]` config file actually runs every declared job, not just the
/// first — each job gets its own destination and (since neither job sets its own `report_path`)
/// its own namespaced report file, so one job's report can't silently clobber the other's.
#[cfg(windows)]
#[test]
fn a_jobs_array_config_runs_every_job_with_its_own_report() {
    let source_a = fixture_tree(&[("a.csv", 16)]);
    let source_b = fixture_tree(&[("b.csv", 32)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let to_toml_path = |p: &Path| p.to_str().expect("utf8").replace('\\', "/");

    let config_path = workdir.path().join("jobs.toml");
    let report_path = workdir.path().join("report.json");
    let log_path = workdir.path().join("ingest.log");
    let dest_alpha = workdir.path().join("out_alpha");
    let dest_beta = workdir.path().join("out_beta");

    std::fs::write(
        &config_path,
        format!(
            r#"
dry_run = true
report_path = "{report}"
log_path = "{log}"

[[jobs]]
name = "alpha"
source = "{source_a}"
dest = "{dest_alpha}"

[[jobs]]
name = "beta"
source = "{source_b}"
dest = "{dest_beta}"
"#,
            report = to_toml_path(&report_path),
            log = to_toml_path(&log_path),
            source_a = to_toml_path(source_a.path()),
            dest_alpha = to_toml_path(&dest_alpha),
            source_b = to_toml_path(source_b.path()),
            dest_beta = to_toml_path(&dest_beta),
        ),
    )
    .expect("write config");

    let output = run(&["--config", config_path.to_str().expect("utf8")]);

    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("job 'alpha'"), "stdout: {stdout}");
    assert!(stdout.contains("job 'beta'"), "stdout: {stdout}");

    let report_alpha = workdir.path().join("report.alpha.json");
    let report_beta = workdir.path().join("report.beta.json");
    assert!(report_alpha.is_file(), "alpha's own report must exist");
    assert!(report_beta.is_file(), "beta's own report must exist");

    let alpha: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_alpha).expect("read")).expect("json");
    let beta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_beta).expect("read")).expect("json");
    // Each job's report must point at that job's own destination, not the other job's.
    assert!(
        alpha["dest"]
            .as_str()
            .unwrap_or_default()
            .contains("out_alpha"),
        "alpha report: {alpha}"
    );
    assert!(
        beta["dest"]
            .as_str()
            .unwrap_or_default()
            .contains("out_beta"),
        "beta report: {beta}"
    );
}

/// D12 black-box regression test: two jobs in the same `[[jobs]]` batch sharing the same `dest`
/// with `--backup-type` used to merge their generation histories into one
/// `.rustcopy_generations.json`, since it was derived purely from `dest` with no job identity —
/// a second job's `latest()` would then diff against the *first* job's unrelated source tree.
/// Each job's manifest (and fast-verify cache, same underlying fix) must now be namespaced by job
/// name and stay fully independent even though both jobs write into the exact same directory.
#[cfg(windows)]
#[test]
fn two_jobs_sharing_a_dest_with_backup_type_get_independent_generation_manifests() {
    let source_alpha = fixture_tree(&[("alpha.csv", 8)]);
    let source_beta = fixture_tree(&[("beta.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let to_toml_path = |p: &Path| p.to_str().expect("utf8").replace('\\', "/");

    let config_path = workdir.path().join("jobs.toml");
    let report_path = workdir.path().join("report.json");
    let log_path = workdir.path().join("ingest.log");
    let shared_dest = workdir.path().join("shared_dest");

    std::fs::write(
        &config_path,
        format!(
            r#"
report_path = "{report}"
log_path = "{log}"

[[jobs]]
name = "alpha"
source = "{source_alpha}"
dest = "{dest}"
backup_type = "full"

[[jobs]]
name = "beta"
source = "{source_beta}"
dest = "{dest}"
backup_type = "full"
"#,
            report = to_toml_path(&report_path),
            log = to_toml_path(&log_path),
            source_alpha = to_toml_path(source_alpha.path()),
            source_beta = to_toml_path(source_beta.path()),
            dest = to_toml_path(&shared_dest),
        ),
    )
    .expect("write config");

    let output = run(&["--config", config_path.to_str().expect("utf8")]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));

    let manifest_alpha = shared_dest.join(".rustcopy_generations.alpha.json");
    let manifest_beta = shared_dest.join(".rustcopy_generations.beta.json");
    let manifest_default = shared_dest.join(".rustcopy_generations.json");
    assert!(
        manifest_alpha.is_file(),
        "alpha must get its own namespaced manifest"
    );
    assert!(
        manifest_beta.is_file(),
        "beta must get its own namespaced manifest"
    );
    assert!(
        !manifest_default.is_file(),
        "the unnamespaced manifest filename must not be written by a multi-job run"
    );

    let alpha: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_alpha).expect("read"))
            .expect("json");
    let beta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_beta).expect("read"))
            .expect("json");

    let alpha_files = alpha["generations"][0]["files"]
        .as_array()
        .expect("alpha generation files");
    let beta_files = beta["generations"][0]["files"]
        .as_array()
        .expect("beta generation files");
    assert!(
        alpha_files.iter().any(|f| f["relative_path"]
            .as_str()
            .unwrap_or_default()
            .contains("alpha.csv")),
        "alpha manifest: {alpha}"
    );
    assert!(
        beta_files.iter().any(|f| f["relative_path"]
            .as_str()
            .unwrap_or_default()
            .contains("beta.csv")),
        "beta manifest: {beta}"
    );
    // The bug this guards against: before the fix, beta's manifest (being the same shared file)
    // would have included alpha's inventory too.
    assert!(
        !beta_files.iter().any(|f| f["relative_path"]
            .as_str()
            .unwrap_or_default()
            .contains("alpha.csv")),
        "beta manifest must not contain alpha's files: {beta}"
    );
}

/// D13 black-box regression test: `run_jobs` shares one log file across every job in the batch
/// (deliberate, see its own doc comment). Before this fix, only the "starting job" boundary line
/// carried the job's name — every other event logged while a job actually ran (transfer start,
/// warnings, the actual robocopy invocation and its per-file output, the final "ingestion
/// finished") was indistinguishable from another job's, so two jobs failing in close succession
/// couldn't be told apart from the log file alone. Each job's work is now wrapped in a
/// `tracing::info_span!("job", job = ..)`, and every `tokio::task::spawn_blocking` call site in
/// `main.rs` goes through `spawn_blocking_with_span` (which re-enters the captured span on the
/// blocking thread) instead of the bare `tokio::task::spawn_blocking` — otherwise the blocking
/// thread wouldn't inherit the span and the robocopy transfer's own log lines (arguably the most
/// useful ones to attribute) would stay untagged. The default `tracing_subscriber` formatter
/// prints active span context (`job{job=name}:`) on every line logged within it.
#[cfg(windows)]
#[test]
fn log_lines_are_tagged_with_the_owning_job_name_in_a_multi_job_batch() {
    let source_alpha = fixture_tree(&[("alpha.csv", 8)]);
    let source_beta = fixture_tree(&[("beta.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let to_toml_path = |p: &Path| p.to_str().expect("utf8").replace('\\', "/");

    let config_path = workdir.path().join("jobs.toml");
    let report_path = workdir.path().join("report.json");
    let log_path = workdir.path().join("ingest.log");
    let dest_alpha = workdir.path().join("out_alpha");
    let dest_beta = workdir.path().join("out_beta");

    std::fs::write(
        &config_path,
        format!(
            r#"
report_path = "{report}"
log_path = "{log}"

[[jobs]]
name = "alpha"
source = "{source_a}"
dest = "{dest_alpha}"

[[jobs]]
name = "beta"
source = "{source_b}"
dest = "{dest_beta}"
"#,
            report = to_toml_path(&report_path),
            log = to_toml_path(&log_path),
            source_a = to_toml_path(source_alpha.path()),
            dest_alpha = to_toml_path(&dest_alpha),
            source_b = to_toml_path(source_beta.path()),
            dest_beta = to_toml_path(&dest_beta),
        ),
    )
    .expect("write config");

    let output = run(&["--config", config_path.to_str().expect("utf8")]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));

    let log_contents = std::fs::read_to_string(&log_path).expect("read log");
    let alpha_starting = log_contents
        .lines()
        .find(|l| l.contains("ingestion starting") && l.contains("job{job=alpha}"));
    let beta_starting = log_contents
        .lines()
        .find(|l| l.contains("ingestion starting") && l.contains("job{job=beta}"));
    assert!(
        alpha_starting.is_some(),
        "alpha's 'ingestion starting' line must be tagged with its own job span: {log_contents}"
    );
    assert!(
        beta_starting.is_some(),
        "beta's 'ingestion starting' line must be tagged with its own job span: {log_contents}"
    );

    let alpha_finished = log_contents
        .lines()
        .find(|l| l.contains("ingestion finished") && l.contains("job{job=alpha}"));
    let beta_finished = log_contents
        .lines()
        .find(|l| l.contains("ingestion finished") && l.contains("job{job=beta}"));
    assert!(
        alpha_finished.is_some(),
        "alpha's 'ingestion finished' line must be tagged too, not just the boundary line: {log_contents}"
    );
    assert!(
        beta_finished.is_some(),
        "beta's 'ingestion finished' line must be tagged too, not just the boundary line: {log_contents}"
    );

    // The line that matters most: the actual robocopy invocation runs inside
    // `spawn_blocking_with_span` (main.rs::transfer), on a different OS thread than the one that
    // entered the job span. Without span propagation across that hop, this exact line is the one
    // that would stay untagged and unattributable to either job.
    let alpha_invoking = log_contents
        .lines()
        .find(|l| l.contains("invoking robocopy") && l.contains("job{job=alpha}"));
    let beta_invoking = log_contents
        .lines()
        .find(|l| l.contains("invoking robocopy") && l.contains("job{job=beta}"));
    assert!(
        alpha_invoking.is_some(),
        "alpha's robocopy invocation (on a spawn_blocking thread) must inherit the job span: {log_contents}"
    );
    assert!(
        beta_invoking.is_some(),
        "beta's robocopy invocation (on a spawn_blocking thread) must inherit the job span: {log_contents}"
    );
}

/// F34 black-box test: `--backup-type full` then `--backup-type incremental` against the same
/// destination actually produce two generation subfolders, a manifest recording both, and the
/// incremental run copies only the file that changed (and the new one) — not the unchanged file.
#[cfg(windows)]
#[test]
fn incremental_backup_copies_only_changed_files_since_the_last_generation() {
    let source = fixture_tree(&[("a.csv", 8), ("b.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");
    let report1 = workdir.path().join("report1.json");
    let report2 = workdir.path().join("report2.json");

    let full_output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        report1.to_str().expect("utf8"),
        "--backup-type",
        "full",
    ]);
    assert!(
        full_output.status.success(),
        "stderr: {}",
        stderr_of(&full_output)
    );

    let manifest_path = dest.join(".rustcopy_generations.json");
    assert!(
        manifest_path.is_file(),
        "manifest must exist after the full backup"
    );
    let manifest_after_full: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("read"))
            .expect("json");
    assert_eq!(
        manifest_after_full["generations"].as_array().unwrap().len(),
        1
    );

    // A real filesystem mtime tick matters here: changed_since compares whole seconds.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(source.path().join("a.csv"), b"changed content!").expect("modify a.csv");
    std::fs::write(source.path().join("c.csv"), b"brand new file").expect("add c.csv");

    let inc_output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        report2.to_str().expect("utf8"),
        "--backup-type",
        "incremental",
    ]);
    assert!(
        inc_output.status.success(),
        "stderr: {}",
        stderr_of(&inc_output)
    );
    let stdout = stdout_of(&inc_output);
    assert!(
        stdout.contains("(2 of 3 file(s) to copy)"),
        "expected exactly a.csv+c.csv to be flagged as changed; stdout: {stdout}"
    );

    let manifest_after_inc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("read"))
            .expect("json");
    let generations = manifest_after_inc["generations"].as_array().unwrap();
    assert_eq!(generations.len(), 2, "both generations must be recorded");
    assert_eq!(generations[1]["backup_type"], "incremental");
    assert_eq!(generations[1]["files_copied"], 2);

    let incremental_folder = generations[1]["id"].as_str().expect("id");
    let incremental_dir = dest.join(incremental_folder);
    assert!(
        incremental_dir.join("a.csv").is_file(),
        "changed file must be copied"
    );
    assert!(
        incremental_dir.join("c.csv").is_file(),
        "new file must be copied"
    );
    assert!(
        !incremental_dir.join("b.csv").exists(),
        "unchanged file must NOT be copied into the incremental generation"
    );
}

/// F34 black-box test: `--backup-type incremental` with no prior generation at the destination
/// must fail clearly instead of silently doing a full copy or crashing.
#[cfg(windows)]
#[test]
fn incremental_backup_without_a_prior_generation_fails_clearly() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        workdir.path().join("dest").to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report.json").to_str().expect("utf8"),
        "--backup-type",
        "incremental",
    ]);

    assert!(!output.status.success());
    assert!(
        stderr_of(&output).contains("no prior generation"),
        "stderr: {}",
        stderr_of(&output)
    );
}

/// F34 black-box test: `--backup-type differential` always diffs against the last `full`
/// generation, not the immediately preceding one — unlike `incremental`. Runs full, then two
/// differentials each changing a different file; the second differential must still include the
/// file changed by the first differential, because both compare against the same full baseline.
#[cfg(windows)]
#[test]
fn differential_backup_always_diffs_against_the_last_full_generation() {
    let source = fixture_tree(&[("a.csv", 8), ("b.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");

    let full_output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report1.json").to_str().expect("utf8"),
        "--backup-type",
        "full",
    ]);
    assert!(
        full_output.status.success(),
        "stderr: {}",
        stderr_of(&full_output)
    );

    // A real filesystem mtime tick matters here: changed_since compares whole seconds.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(source.path().join("a.csv"), b"changed by diff 1").expect("modify a.csv");

    let diff1_output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report2.json").to_str().expect("utf8"),
        "--backup-type",
        "differential",
    ]);
    assert!(
        diff1_output.status.success(),
        "stderr: {}",
        stderr_of(&diff1_output)
    );
    assert!(
        stdout_of(&diff1_output).contains("(1 of 2 file(s) to copy)"),
        "first differential must copy only a.csv; stdout: {}",
        stdout_of(&diff1_output)
    );

    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(source.path().join("b.csv"), b"changed by diff 2").expect("modify b.csv");

    let diff2_output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report3.json").to_str().expect("utf8"),
        "--backup-type",
        "differential",
    ]);
    assert!(
        diff2_output.status.success(),
        "stderr: {}",
        stderr_of(&diff2_output)
    );
    let stdout = stdout_of(&diff2_output);
    assert!(
        stdout.contains("(2 of 2 file(s) to copy)"),
        "second differential must still include a.csv (changed since the full, not since diff 1); stdout: {stdout}"
    );

    let manifest_path = dest.join(".rustcopy_generations.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("read"))
            .expect("json");
    let generations = manifest["generations"].as_array().unwrap();
    assert_eq!(
        generations.len(),
        3,
        "full + two differentials must all be recorded"
    );
    assert_eq!(generations[1]["backup_type"], "differential");
    assert_eq!(generations[1]["files_copied"], 1);
    assert_eq!(generations[2]["backup_type"], "differential");
    assert_eq!(generations[2]["files_copied"], 2);

    let diff2_folder = generations[2]["id"].as_str().expect("id");
    let diff2_dir = dest.join(diff2_folder);
    assert!(
        diff2_dir.join("a.csv").is_file(),
        "a.csv must be in the second differential too"
    );
    assert!(
        diff2_dir.join("b.csv").is_file(),
        "b.csv must be in the second differential"
    );
}

/// F34 black-box test: `--backup-type differential` with no prior `full` generation at the
/// destination must fail clearly, even if an `incremental` generation already exists there.
#[cfg(windows)]
#[test]
fn differential_backup_without_a_prior_full_generation_fails_clearly() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        workdir.path().join("dest").to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("ingest.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("report.json").to_str().expect("utf8"),
        "--backup-type",
        "differential",
    ]);

    assert!(!output.status.success());
    assert!(
        stderr_of(&output).contains("no prior full generation"),
        "stderr: {}",
        stderr_of(&output)
    );
}

/// D15 black-box regression test (hypothesis #7 from `NEXT_SESSION_PROMPT.md` — exit-code
/// consistency between the plain-sync and `--backup-type` pipelines): before this fix, a copy
/// failure in `execute_generation_backup` propagated as a fatal `anyhow::Error` all the way to
/// `async_main()`, which mapped it to `EXIT_UNRECOVERABLE` (2) — the same code used for a bad
/// `--pattern` or a missing source directory — instead of `EXIT_INGESTION_PROBLEM` (1), the code
/// the plain-sync pipeline's `transfer()` already uses for "the copy itself failed". Worse: no
/// JSON report was written at all on that path, unlike the plain-sync pipeline, which always
/// writes one. This forces a source file open failure (share_mode(0), deny-all-sharing, the same
/// technique `permanently_uncopyable_file_does_not_undercount_the_rest_of_the_transfer` above uses
/// against a destination file) so `engine::naive::copy_one` fails deterministically inside a real
/// `--backup-type full` run, and checks both halves of the fix: exit code 1, and a written report
/// carrying the new `copy_error` field.
#[cfg(windows)]
#[test]
fn a_failed_generation_backup_reports_exit_code_1_not_2_and_still_writes_a_report() {
    use std::os::windows::fs::OpenOptionsExt;

    let source = fixture_tree(&[("a.csv", 8)]);
    let locked_source_file = source.path().join("a.csv");
    let _locked_handle = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&locked_source_file)
        .expect("lock the source file exclusively");

    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");

    let output = run_generation_backup(
        source.path(),
        &dest,
        workdir.path(),
        "full",
        "report.json",
        &[],
    );
    drop(_locked_handle);

    assert!(!output.status.success());
    assert_eq!(
        output.status.code(),
        Some(1),
        "a failed generation copy must map to EXIT_INGESTION_PROBLEM (1), not EXIT_UNRECOVERABLE (2); stderr: {}",
        stderr_of(&output)
    );

    let report_path = workdir.path().join("report.json");
    assert!(
        report_path.is_file(),
        "a report must still be written for a failed generation backup, same as the plain-sync pipeline"
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    assert!(
        report["copy_error"].as_str().is_some_and(|s| !s.is_empty()),
        "the report must record why the generation copy failed: {report}"
    );

    // The bug this guards against, second half: a failed generation must not be recorded in the
    // manifest as if it succeeded (it already wasn't, but confirm the manifest wasn't even created
    // at all rather than existing empty/inconsistent).
    let manifest = dest.join(".rustcopy_generations.json");
    assert!(
        !manifest.is_file(),
        "a failed first generation must not leave a manifest behind, empty or otherwise"
    );
}

/// D14 black-box regression test: `GenerationManifest::save` and `IngestCache::save_to` used to
/// write via a bare `std::fs::write`, so a crash mid-write of a large manifest (~174 MB for the
/// real-world 1.34M-file profile in `_ops_reports/full-profile-test.json`) could leave a
/// truncated, unparseable `.rustcopy_generations.json` — fatal for every future
/// incremental/differential/retention run against that destination. Both now go through
/// `robocopy_ingest::atomic_write` (temp file + rename). This test doesn't simulate a real crash
/// (covered by the pure unit tests in `lib.rs`) — it exercises the real binary end-to-end and
/// confirms no stray `.rustcopy-tmp` sibling file is ever left behind after a normal successful
/// run, i.e. the temp-file plumbing is actually wired up in the real save path, not just present
/// in the helper function.
#[cfg(windows)]
#[test]
fn a_successful_backup_leaves_no_atomic_write_temp_files_behind() {
    let source = fixture_tree(&[("a.csv", 8), ("b.csv", 16)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");

    let output = run_generation_backup(
        source.path(),
        &dest,
        workdir.path(),
        "full",
        "report.json",
        &[],
    );
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));

    let manifest_tmp = dest.join(".rustcopy_generations.json.rustcopy-tmp");
    assert!(
        !manifest_tmp.exists(),
        "no stray manifest temp file must remain after a successful run"
    );

    let manifest = dest.join(".rustcopy_generations.json");
    assert!(
        manifest.is_file(),
        "the real manifest must exist after a successful run"
    );
}

/// Runs one `--backup-type` backup against a shared source/dest/log, writing its report to
/// `<workdir>/<report_name>`. Shared helper for the F35 retention tests below.
#[cfg(windows)]
fn run_generation_backup(
    source: &Path,
    dest: &Path,
    workdir: &Path,
    backup_type: &str,
    report_name: &str,
    extra: &[&str],
) -> std::process::Output {
    let report = workdir.join(report_name);
    let log = workdir.join("ingest.log");
    let mut cmd_args = vec![
        "--source",
        source.to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--log-path",
        log.to_str().expect("utf8"),
        "--report-path",
        report.to_str().expect("utf8"),
        "--backup-type",
        backup_type,
    ];
    cmd_args.extend_from_slice(extra);
    run(&cmd_args)
}

/// F35 black-box test: `--keep-generations N` keeps only the N most recent *cycles* (a full plus
/// the incremental/differential generations that follow it) and deletes the older cycles' folders
/// from disk as well as their entries in the manifest — not just individual old generations,
/// which could otherwise strand an incremental/differential without the full it depends on.
#[cfg(windows)]
#[test]
fn retention_prunes_older_cycles_but_keeps_the_most_recent_ones() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");

    assert!(
        run_generation_backup(source.path(), &dest, workdir.path(), "full", "r1.json", &[])
            .status
            .success()
    );
    assert!(run_generation_backup(
        source.path(),
        &dest,
        workdir.path(),
        "incremental",
        "r2.json",
        &[]
    )
    .status
    .success());
    assert!(
        run_generation_backup(source.path(), &dest, workdir.path(), "full", "r3.json", &[])
            .status
            .success()
    );
    let last = run_generation_backup(
        source.path(),
        &dest,
        workdir.path(),
        "incremental",
        "r4.json",
        &["--keep-generations", "1", "--force-purge"],
    );
    assert!(last.status.success(), "stderr: {}", stderr_of(&last));

    let manifest_path = dest.join(".rustcopy_generations.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("read"))
            .expect("json");
    let generations = manifest["generations"].as_array().unwrap();
    let ids: Vec<&str> = generations
        .iter()
        .map(|g| g["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids.len(),
        2,
        "only the most recent cycle should remain in the manifest: {ids:?}"
    );
    assert!(ids[0].ends_with("_full"), "ids: {ids:?}");
    assert!(ids[1].ends_with("_incremental"), "ids: {ids:?}");

    // The older cycle's folders must actually be gone from disk, not just the manifest.
    let entries: Vec<String> = std::fs::read_dir(&dest)
        .expect("read dest")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries.len(),
        2,
        "only 2 generation folders should remain on disk: {entries:?}"
    );
}

/// F35 black-box test: without `--force-purge` and with no interactive terminal to confirm, the
/// retention purge must abort with a dedicated exit code and delete nothing — including not
/// deleting the folders of the generation that was just successfully copied and recorded in this
/// same run (only the *pruning* step is aborted, not the backup itself).
#[cfg(windows)]
#[test]
fn retention_purge_is_aborted_without_force_purge_and_nothing_is_deleted() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("dest");

    assert!(
        run_generation_backup(source.path(), &dest, workdir.path(), "full", "r1.json", &[])
            .status
            .success()
    );
    assert!(run_generation_backup(
        source.path(),
        &dest,
        workdir.path(),
        "incremental",
        "r2.json",
        &[]
    )
    .status
    .success());
    assert!(
        run_generation_backup(source.path(), &dest, workdir.path(), "full", "r3.json", &[])
            .status
            .success()
    );

    let output = run_generation_backup(
        source.path(),
        &dest,
        workdir.path(),
        "incremental",
        "r4.json",
        &["--keep-generations", "1"],
    );
    assert_eq!(
        output.status.code(),
        Some(5),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        stderr_of(&output).contains("--keep-generations would delete"),
        "stderr: {}",
        stderr_of(&output)
    );

    let manifest_path = dest.join(".rustcopy_generations.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("read"))
            .expect("json");
    assert_eq!(
        manifest["generations"].as_array().unwrap().len(),
        4,
        "the 4th generation's copy+manifest save already succeeded before pruning was aborted"
    );
}

/// F35 black-box test: `--keep-generations` without `--backup-type` has nothing to rotate and
/// must be rejected by the real binary, not just at the `Args::validate()` unit-test level.
#[test]
fn keep_generations_without_backup_type_is_rejected_by_the_real_binary() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let dest = tempfile::tempdir().expect("dest");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.path().to_str().expect("utf8"),
        "--keep-generations",
        "2",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_of(&output).contains("--keep-generations requires --backup-type"));
}

/// F34 black-box test: `--backup-type` and `--mirror` together must be rejected by the real
/// binary, not just at the `Args::validate()` unit-test level.
#[test]
fn backup_type_and_mirror_together_are_rejected_by_the_real_binary() {
    let source = fixture_tree(&[("a.csv", 8)]);
    let dest = tempfile::tempdir().expect("dest");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.path().to_str().expect("utf8"),
        "--backup-type",
        "full",
        "--mirror",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_of(&output).contains("--backup-type and --mirror"));
}

/// Guards the assumption the Linux tests rely on: the fixture helper is deterministic.
#[test]
fn fixtures_are_created_where_expected() {
    let dir = fixture_tree(&[("nested/a.csv", 32)]);
    let path: &Path = &dir.path().join("nested/a.csv");
    assert_eq!(std::fs::metadata(path).expect("metadata").len(), 32);
}

/// P2 black-box test (PIANO_MIGLIORAMENTI.md): running the real binary twice against the same
/// fixed `--report-path` must produce a first report with no `previous_run_comparison` and a
/// second report whose `previous_run_comparison` reflects the first run -- this is the actual
/// wiring in `main.rs` (read the file at `--report-path` before `write_to` overwrites it), not
/// just `report.rs`'s own unit tests of `RunComparison::between` in isolation.
///
/// `#[cfg(windows)]`, same reason as `quiet_suppresses_per_file_debug_lines_in_the_real_log`
/// above: a real (non-`--compare-baseline`) transfer needs `robocopy.exe`, which only exists on
/// Windows -- on Linux/macOS `run(&args)` fails outright before any report is even written.
#[cfg(windows)]
#[test]
fn a_second_run_against_the_same_report_path_gets_a_comparison_against_the_first() {
    let source = fixture_tree(&[("a.csv", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    let report_path = workdir.path().join("report.json");

    let args = [
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--report-path",
        report_path.to_str().expect("utf8"),
    ];

    let first = run(&args);
    assert!(first.status.success(), "stderr: {}", stderr_of(&first));
    let first_report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    assert!(
        first_report.get("previous_run_comparison").is_none(),
        "the first run against a fresh --report-path must have nothing to compare against"
    );
    let first_timestamp = first_report["timestamp"]
        .as_str()
        .expect("timestamp")
        .to_string();

    let second = run(&args);
    assert!(second.status.success(), "stderr: {}", stderr_of(&second));
    let second_report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    let comparison = &second_report["previous_run_comparison"];
    assert!(
        !comparison.is_null(),
        "the second run must carry a comparison against the first: {second_report}"
    );
    assert_eq!(comparison["previous_timestamp"], first_timestamp);
    // Robocopy's own default behaviour skips a destination file that already matches (same
    // size+timestamp): the first run copies the 1 fixture file, the second copies 0 (nothing
    // changed) -- so the real, correct delta is -1, not 0. Asserting this exact non-zero value
    // (rather than just "is present") is what actually proves the field is computed from the
    // real previous run, not a coincidental default.
    assert_eq!(comparison["files_copied_delta"], -1);
}

/// P1 black-box test (PIANO_MIGLIORAMENTI.md): `{timestamp}` in `--report-path` must be resolved
/// by the real binary to an actual timestamp, not left as literal text -- this is the wiring in
/// `main.rs::run` (right before `validate()`), not just `lib.rs`'s own unit tests of
/// `resolve_report_path_timestamp` in isolation.
///
/// `#[cfg(windows)]`, same reason as `a_second_run_against_the_same_report_path_...` above (P2's
/// equivalent test): a real (non-`--compare-baseline`) transfer needs `robocopy.exe`, which only
/// exists on Windows -- on Linux/macOS `run(&args)` fails outright before any report is written.
#[cfg(windows)]
#[test]
fn timestamp_placeholder_in_report_path_is_resolved_by_the_real_binary() {
    let source = fixture_tree(&[("a.csv", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("out");
    let report_path_template = workdir.path().join("report-{timestamp}.json");

    let output = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest.to_str().expect("utf8"),
        "--report-path",
        report_path_template.to_str().expect("utf8"),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));

    // The literal template path must never exist: the placeholder is always resolved before
    // anything writes to report_path, including this run's own report.
    assert!(
        !report_path_template.exists(),
        "the unresolved {{timestamp}} path must not have been written to"
    );

    let written: Vec<_> = std::fs::read_dir(workdir.path())
        .expect("read workdir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("report-") && name.ends_with(".json"))
        .collect();
    assert_eq!(
        written.len(),
        1,
        "expected exactly one resolved report file, found: {written:?}"
    );
    // yyyyMMdd_HHmmss -- 15 digits/underscore between "report-" and ".json", never the literal
    // "{timestamp}" text.
    let resolved_name = &written[0];
    let timestamp_part = &resolved_name["report-".len()..resolved_name.len() - ".json".len()];
    assert_eq!(
        timestamp_part.len(),
        15,
        "unexpected shape: {resolved_name}"
    );
    assert!(
        timestamp_part
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_'),
        "unexpected shape: {resolved_name}"
    );
}

/// P1 black-box test: in multi-job (`[[jobs]]`) mode, each job's `report_path` must end up with
/// *both* its own resolved timestamp *and* the per-job namespace (F33/D12) -- proving the two
/// independent path transformations (P1's placeholder resolution, F33's `namespaced_path`)
/// compose correctly instead of one clobbering the other, which is exactly the interaction
/// `PIANO_MIGLIORAMENTI.md`'s P1 analysis flagged as needing an explicit check.
///
/// `#[cfg(windows)]`, same reason as the test above: needs a real `robocopy.exe` transfer.
#[cfg(windows)]
#[test]
fn timestamp_placeholder_composes_with_per_job_namespacing() {
    let source_alpha = fixture_tree(&[("alpha.csv", 8)]);
    let source_beta = fixture_tree(&[("beta.csv", 8)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let to_toml_path = |p: &Path| p.to_str().expect("utf8").replace('\\', "/");

    let config_path = workdir.path().join("jobs.toml");
    let report_path_template = workdir.path().join("report-{timestamp}.json");
    let dest_alpha = workdir.path().join("out_alpha");
    let dest_beta = workdir.path().join("out_beta");

    std::fs::write(
        &config_path,
        format!(
            r#"
dry_run = true
report_path = "{report}"

[[jobs]]
name = "alpha"
source = "{source_a}"
dest = "{dest_alpha}"

[[jobs]]
name = "beta"
source = "{source_b}"
dest = "{dest_beta}"
"#,
            report = to_toml_path(&report_path_template),
            source_a = to_toml_path(source_alpha.path()),
            dest_alpha = to_toml_path(&dest_alpha),
            source_b = to_toml_path(source_beta.path()),
            dest_beta = to_toml_path(&dest_beta),
        ),
    )
    .expect("write config");

    let output = run(&["--config", config_path.to_str().expect("utf8")]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));

    let written: Vec<_> = std::fs::read_dir(workdir.path())
        .expect("read workdir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("report-") && name.ends_with(".json"))
        .collect();
    assert_eq!(
        written.len(),
        2,
        "expected one resolved+namespaced report per job, found: {written:?}"
    );
    assert!(
        written.iter().any(|name| name.contains(".alpha.")),
        "missing alpha's report: {written:?}"
    );
    assert!(
        written.iter().any(|name| name.contains(".beta.")),
        "missing beta's report: {written:?}"
    );
    for name in &written {
        assert!(
            !name.contains("{timestamp}"),
            "placeholder left unresolved: {name}"
        );
    }
}
