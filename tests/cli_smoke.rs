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
    assert_eq!(decrypted, plaintext, "must decrypt back to the original bytes");
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

    assert_eq!(output.status.code(), Some(1), "some items could not be copied");
    assert!(report_path.is_file());
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read")).expect("json");
    // b.csv itself exists at the destination (as the pre-created, still-locked 0-byte stub), so
    // all 3 destination entries are present — but its *content* was never overwritten by
    // robocopy (locked the whole time), so only a.csv (10B) + c.csv (30B) = 40B are real,
    // correctly-copied bytes. Before the fix, this failure path reported whatever the *last*
    // retry attempt's reset progress sink happened to hold (near-zero for a run this short),
    // not a real reflection of what's actually sitting on disk.
    let bytes_copied = report["robocopy_transfer"]["bytes_copied"].as_u64().expect("bytes_copied");
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
    std::fs::write(original.join("important.csv"), b"irreplaceable,data\n1,2\n").expect("seed file");

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
        sandbox.path().join("restore_report.json").to_str().expect("utf8"),
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
        sandbox.path().join("restore_report.json").to_str().expect("utf8"),
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

    assert_eq!(output.status.code(), Some(2), "must be an unrecoverable usage error");
    assert!(
        stderr_of(&output).contains("cannot both be given"),
        "got: {}",
        stderr_of(&output)
    );
}

/// F26a black-box test (closes half of D2): `scan.rs`'s prescan doesn't apply `--exclude-files`
/// (only robocopy's own `/XF` does, at copy time), so a file matched by the pattern but excluded
/// from the actual transfer is a deterministic, non-racy way to reproduce "file present in the
/// prescan inventory but missing at the destination when `--verify-integrity` runs" — exactly the
/// scenario `--ignore-transient-missing` exists for, without needing a real timing race.
#[cfg(windows)]
#[test]
fn ignore_transient_missing_turns_an_excluded_log_into_a_pass() {
    let source = fixture_tree(&[("a.csv", 10), ("stale.log", 20)]);
    let workdir = tempfile::tempdir().expect("workdir");

    // Without --ignore-transient-missing: stale.log is in the prescan inventory (matched by the
    // default "*" pattern) but never reaches the destination (excluded via --exclude-files), so
    // verification must fail.
    let dest_a = workdir.path().join("out-a");
    let output_a = run(&[
        "--source",
        source.path().to_str().expect("utf8"),
        "--dest",
        dest_a.to_str().expect("utf8"),
        "--log-path",
        workdir.path().join("a.log").to_str().expect("utf8"),
        "--report-path",
        workdir.path().join("a-report.json").to_str().expect("utf8"),
        "--exclude-files",
        "*.log",
        "--verify-integrity",
    ]);
    assert_eq!(
        output_a.status.code(),
        Some(1),
        "stale.log missing at dest must fail verification without --ignore-transient-missing; stderr: {}",
        stderr_of(&output_a)
    );

    // With --ignore-transient-missing: the same missing stale.log must be tolerated.
    let dest_b = workdir.path().join("out-b");
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
        "--exclude-files",
        "*.log",
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
    assert!(status.success(), "mklink /J must succeed to exercise this test");

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
        serde_json::from_str(&std::fs::read_to_string(&report_default).expect("read")).expect("json");
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
    assert!(output_excl.status.success(), "stderr: {}", stderr_of(&output_excl));
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
    std::fs::write(backup_dest.join("important.csv"), b"irreplaceable,data\n1,2\n")
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
        sandbox.path().join("restore_report.json").to_str().expect("utf8"),
    ]);
    assert!(
        restore_output.status.success(),
        "a pre-rename Mismatch shape must not break --restore-from; stderr: {}",
        stderr_of(&restore_output)
    );
    let restored = std::fs::read_to_string(original.join("important.csv")).expect("file must be restored");
    assert_eq!(restored, "irreplaceable,data\n1,2\n");
}

/// Guards the assumption the Linux tests rely on: the fixture helper is deterministic.
#[test]
fn fixtures_are_created_where_expected() {
    let dir = fixture_tree(&[("nested/a.csv", 32)]);
    let path: &Path = &dir.path().join("nested/a.csv");
    assert_eq!(std::fs::metadata(path).expect("metadata").len(), 32);
}
