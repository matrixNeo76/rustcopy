//! CLI entry point: orchestrates scan, transfer, optional baseline benchmark and verification.

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use tempfile::TempDir;

use robocopy_ingest::cli::Args;
use robocopy_ingest::crypto::CryptoManager;
use robocopy_ingest::engine::naive::NaiveCopyEngine;
use robocopy_ingest::engine::robocopy::RobocopyEngine;
use robocopy_ingest::engine::{self, CopyEngine, CopyOutcome, ThreadSleeper};
use robocopy_ingest::errors::IngestError;
use robocopy_ingest::integrity::{self, IntegrityCheck};
use robocopy_ingest::logging;
use robocopy_ingest::progress::{ProgressSink, ThroughputProgress};
use robocopy_ingest::report::{format_bytes, IngestReport};
use robocopy_ingest::scan::{self, ScanSummary};

/// How often the destination directory is sampled to estimate live throughput.
///
/// F2.1 fix: this used to be 500ms, which meant a full recursive walk of the destination
/// (potentially a large SMB share) every half second — often more expensive than the transfer
/// itself. Robocopy's own per-file stdout output (parsed in `engine::robocopy`) already drives
/// the bar for the common case; this poller only exists to keep it moving during long single-file
/// copies, so a long interval is sufficient.
const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Exit code when the ingestion ran but the outcome is not acceptable.
const EXIT_INGESTION_PROBLEM: u8 = 1;
/// Exit code for usage errors and unrecoverable environment problems.
const EXIT_UNRECOVERABLE: u8 = 2;
/// Exit code when `--mirror` was aborted because it would have purged destination files and
/// neither `--force-purge` nor an interactive confirmation was given.
const EXIT_MIRROR_ABORTED: u8 = 3;

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();

    match run(args).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(EXIT_INGESTION_PROBLEM),
        Err(error) => {
            if matches!(
                error.downcast_ref::<IngestError>(),
                Some(IngestError::MirrorPurgeAborted { .. })
            ) {
                eprintln!("error: {error:#}");
                ExitCode::from(EXIT_MIRROR_ABORTED)
            } else {
                eprintln!("error: {error:#}");
                ExitCode::from(EXIT_UNRECOVERABLE)
            }
        }
    }
}

/// Returns `Ok(true)` when the ingestion is fully acceptable.
async fn run(mut args: Args) -> Result<bool> {
    if let Some(restore_report) = args.restore_from.clone() {
        args = robocopy_ingest::restore::build_restore_args(&args, &restore_report, None)?;
    } else if let Some(config_path) = &args.config {
        let config = robocopy_ingest::config::IngestConfig::load_from(config_path)
            .with_context(|| format!("cannot load config file from {}", config_path.display()))?;
        args.merge_config(config);
    }

    args.validate()?;

    let log = logging::init(&args.log_path).context("cannot initialise the log file")?;
    tracing::info!(
        source = %args.source().display(),
        dest = %args.dest().display(),
        pattern = %args.pattern,
        threads = args.threads,
        retries = args.retries,
        dry_run = args.dry_run,
        "ingestion starting"
    );

    // Published with the actual robocopy.exe child's PID while it runs, so a Ctrl+C only
    // terminates *this* transfer instead of every robocopy.exe process on the host (the previous
    // `taskkill /IM robocopy.exe` behaviour).
    let child_pid = Arc::new(AtomicU32::new(0));

    let result = tokio::select! {
        res = execute(&args, Arc::clone(&child_pid)) => res,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nreceived Ctrl+C interrupt signal, terminating the active transfer...");
            tracing::warn!("ingestion interrupted by Ctrl+C signal");
            kill_active_child(&child_pid);
            log.flush().await;
            log.shutdown().await;
            return Ok(false);
        }
    };

    log.flush().await;

    let acceptable = match &result {
        Ok(outcome) => outcome.acceptable,
        Err(error) => {
            tracing::error!("ingestion aborted: {error:#}");
            false
        }
    };
    tracing::info!(acceptable, "ingestion finished");
    let dropped = log.dropped_lines();
    log.shutdown().await;
    if dropped > 0 {
        eprintln!("warning: {dropped} log line(s) were dropped (logger under load)");
    }

    let outcome = result?;
    println!("\n{}", outcome.summary);
    println!("\nJSON report: {}", outcome.report_path.display());
    println!("Log file   : {}", args.log_path.display());
    if !acceptable {
        eprintln!("\nthe ingestion completed with problems, see the report for details");
    }
    Ok(acceptable)
}

/// Terminate only the tracked child PID (if any), never every `robocopy.exe` on the host.
#[cfg(windows)]
fn kill_active_child(child_pid: &Arc<AtomicU32>) {
    let pid = child_pid.load(Ordering::SeqCst);
    if pid == 0 {
        return;
    }
    tracing::warn!(pid, "sending kill signal to the tracked robocopy.exe child process");
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(not(windows))]
fn kill_active_child(_child_pid: &Arc<AtomicU32>) {}

struct RunOutcome {
    acceptable: bool,
    summary: String,
    report_path: PathBuf,
}

async fn execute(args: &Args, child_pid: Arc<AtomicU32>) -> Result<RunOutcome> {
    let start_all = Instant::now();

    let start_inv = Instant::now();
    let inventory = inventory_source(args).await?;
    let inventory_seconds = start_inv.elapsed().as_secs_f64();

    if inventory.is_empty() {
        tracing::warn!(pattern = %args.pattern, "no file matches the pattern in the source tree");
        println!(
            "warning: no file matching {} found in {}",
            args.pattern,
            args.source().display()
        );
    }

    // F21 (fixed): a real mirror-purge safety check, run with the actual source inventory
    // available, instead of the previous no-op that only logged a message.
    check_mirror_safety(args, &inventory).await?;

    if !args.dry_run {
        tokio::fs::create_dir_all(args.dest())
            .await
            .map_err(|error| IngestError::io(args.dest(), error))
            .context("cannot create the destination directory")?;
    }

    let start_transfer = Instant::now();
    let (robocopy_outcome, copy_failure) = transfer(args, &inventory, child_pid).await?;
    let transfer_seconds = start_transfer.elapsed().as_secs_f64();

    let (baseline_outcome, baseline_seconds) = if args.compare_baseline && copy_failure.is_none() {
        let start_base = Instant::now();
        let outcome = baseline(args, &inventory).await?;
        (Some(outcome), Some(start_base.elapsed().as_secs_f64()))
    } else {
        if args.compare_baseline {
            tracing::warn!("skipping the baseline benchmark because the main transfer failed");
        }
        (None, None)
    };

    let (integrity_check, verification_seconds) = if args.verify_integrity && !args.dry_run && copy_failure.is_none() {
        if inventory.total_files_hint.is_some() {
            tracing::warn!(
                "skipping integrity verification: --no-prescan did not collect per-file paths"
            );
            println!("warning: --verify-integrity has no effect together with --no-prescan");
            (None, None)
        } else {
            let start_ver = Instant::now();
            let check = verify(args, &inventory).await?;
            (Some(check), Some(start_ver.elapsed().as_secs_f64()))
        }
    } else {
        if args.verify_integrity && args.dry_run {
            tracing::warn!("skipping integrity verification: nothing was copied in dry-run mode");
        }
        (None, None)
    };

    // Encrypt/decrypt destination files only after verification, so integrity checks still
    // compare same-form bytes on both sides (plaintext vs plaintext on a normal run, or
    // ciphertext vs ciphertext while restoring an encrypted backup). `validate()` already
    // rejects passing both --encrypt-aes256 and --decrypt together, so at most one of these runs.
    let encrypted_count = if let (Some(key_spec), None, false) =
        (&args.encrypt_aes256, &copy_failure, args.dry_run)
    {
        Some(encrypt_destination(args, &inventory, key_spec).await?)
    } else {
        None
    };
    let decrypted_count = if let (Some(key_spec), None, false) =
        (&args.decrypt, &copy_failure, args.dry_run)
    {
        Some(decrypt_destination(args, &inventory, key_spec).await?)
    } else {
        None
    };

    let timing = robocopy_ingest::report::PhaseTiming {
        inventory_seconds,
        transfer_seconds,
        verification_seconds,
        baseline_seconds,
        total_seconds: start_all.elapsed().as_secs_f64(),
    };

    let mut report = IngestReport::with_timing(
        args,
        &inventory,
        &robocopy_outcome,
        baseline_outcome.as_ref(),
        integrity_check.clone(),
        timing,
    );
    report.encrypted = encrypted_count.unwrap_or(0) > 0;
    report.decrypted = decrypted_count.unwrap_or(0) > 0;

    if let Some(webhook_url) = &args.webhook_url {
        match robocopy_ingest::notify::send_webhook(webhook_url, &report).await {
            Ok(()) => tracing::info!("completion webhook delivered"),
            Err(error) => {
                tracing::error!(error = %error, "completion webhook delivery failed");
                eprintln!("warning: completion webhook delivery failed: {error}");
                report.webhook_error = Some(error);
            }
        }
    }

    report
        .write_to(&args.report_path)
        .with_context(|| format!("cannot write the report to {}", args.report_path.display()))?;
    tracing::info!(path = %args.report_path.display(), "report written");

    if let Some(html_path) = &args.html_report_path {
        if let Err(e) = robocopy_ingest::html_report::generate_html_report(&report, html_path) {
            tracing::warn!(error = %e, "could not generate HTML report");
        } else {
            tracing::info!(path = %html_path.display(), "HTML report written successfully");
        }
    }

    let integrity_ok = integrity_check
        .as_ref()
        .map(IntegrityCheck::passed)
        .unwrap_or(true);
    if let Some(error) = &copy_failure {
        tracing::error!("transfer failed: {error}");
        eprintln!("transfer failed: {error}");
    }

    Ok(RunOutcome {
        acceptable: copy_failure.is_none() && integrity_ok,
        summary: report.human_summary(),
        report_path: args.report_path.clone(),
    })
}

/// Build the source inventory off the async runtime: walking a 50 GB tree is blocking work.
///
/// F2.4/F2.6 fix: `--no-prescan` used to be accepted but ignored (this function always did the
/// full walk regardless). It now actually switches to the lightweight `scan::inventory` walk,
/// which counts files/bytes without materialising every path in RAM — the whole point of the
/// flag on multi-million file trees.
async fn inventory_source(args: &Args) -> Result<ScanSummary> {
    let source = args.source().to_path_buf();
    let pattern = args.pattern.clone();
    let no_prescan = args.no_prescan;
    // F26d: follow junctions/symlinked directories exactly when robocopy itself will (i.e.
    // whenever /XJ is not passed), so the prescan and the actual transfer walk the same tree.
    let follow_links = !args.exclude_junctions;

    let inventory = if no_prescan {
        tokio::task::spawn_blocking(move || scan::inventory(&source, &pattern, follow_links))
            .await
            .context("the source scan task panicked")?
            .context("cannot scan the source directory")?
            .into_scan_summary()
    } else {
        tokio::task::spawn_blocking(move || scan::scan(&source, &pattern, follow_links))
            .await
            .context("the source scan task panicked")?
            .context("cannot scan the source directory")?
    };

    tracing::info!(
        files = inventory.file_count(),
        bytes = inventory.total_bytes,
        no_prescan,
        "source inventory complete"
    );
    println!(
        "Inventory: {} file(s) matching {}, {}",
        inventory.file_count(),
        args.pattern,
        format_bytes(inventory.total_bytes)
    );
    Ok(inventory)
}

/// Real mirror-purge safety check (F21).
///
/// With `--mirror` (robocopy `/MIR`), any destination file/dir that doesn't match the current
/// source+pattern is purged — including, non-obviously, files that simply don't match `--pattern`
/// even if they exist in the source tree under a different name. The previous implementation
/// only compared destination *byte size* against zero and logged a message; it never counted
/// what would actually be deleted and never blocked anything, so `--force-purge` had no effect to
/// disable.
///
/// This walks the destination (when it already exists) and diffs its relative paths against the
/// source inventory; the difference is exactly what `/MIR` would purge. If that's non-empty and
/// `--force-purge` wasn't given, the run aborts (or, on an interactive terminal, asks for
/// confirmation) rather than proceeding blind.
///
/// F26b fix: the destination walk used to run synchronously right here, inside `async fn
/// execute()` — every other blocking filesystem walk in this file (inventory, transfer, verify,
/// crypto, the dest poller) is wrapped in `tokio::task::spawn_blocking`, but this one wasn't. On
/// an SMB share with millions of files that froze the whole tokio executor, including `Ctrl+C`
/// handling and the progress bar, for the entire scan. Only the call site changed: `scan::scan`
/// itself stays a plain sync fn since it's also called from non-async code paths.
async fn check_mirror_safety(args: &Args, inventory: &ScanSummary) -> Result<()> {
    if !args.mirror || args.force_purge || !args.dest().exists() {
        return Ok(());
    }
    if inventory.total_files_hint.is_some() {
        // --no-prescan: we don't have the source's per-file list to diff against. Erring toward
        // caution, still require --force-purge explicitly rather than silently allowing purges
        // whose scope we can't compute.
        return Err(IngestError::MirrorPurgeAborted {
            count: usize::MAX,
        }
        .into());
    }

    let source_relative: HashSet<PathBuf> = inventory
        .files
        .iter()
        .map(|f| normalize_for_compare(&f.relative_path))
        .collect();

    let dest = args.dest().to_path_buf();
    // F26d: follow junctions in the destination scan exactly when the source scan did, so the
    // mirror-purge diff isn't computed against a differently-shaped tree.
    let follow_links = !args.exclude_junctions;
    let dest_all = tokio::task::spawn_blocking(move || scan::scan(&dest, "*", follow_links))
        .await
        .context("the mirror safety scan task panicked")?
        .context("cannot scan the destination for the mirror safety check")?;
    let extraneous: Vec<&Path> = dest_all
        .files
        .iter()
        .map(|f| f.relative_path.as_path())
        .filter(|p| !source_relative.contains(&normalize_for_compare(p)))
        .collect();

    if extraneous.is_empty() {
        return Ok(());
    }

    let count = extraneous.len();
    tracing::warn!(
        count,
        dest = %args.dest().display(),
        "mirror mode would purge destination files not present in the source"
    );

    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        eprintln!(
            "\n--mirror would delete {count} file(s) from {} that are not in the source \
             (first few: {}).",
            args.dest().display(),
            extraneous
                .iter()
                .take(5)
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        eprint!("Proceed with the purge? [y/N] ");
        use std::io::Write;
        std::io::stderr().flush().ok();
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer).ok();
        if answer.trim().eq_ignore_ascii_case("y") {
            return Ok(());
        }
    }

    Err(IngestError::MirrorPurgeAborted { count }.into())
}

fn normalize_for_compare(path: &Path) -> PathBuf {
    // Windows paths/patterns are case-insensitive; compare case-folded so `Report.CSV` and
    // `report.csv` are recognised as the same destination entry.
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

/// Encrypt every file the transfer touched, in place in the destination, with AES-256-GCM.
///
/// F25a fix: streams each file through [`CryptoManager::encrypt_file`] in fixed-size chunks
/// instead of reading it whole into RAM — the previous `std::fs::read`/`std::fs::write` pair
/// meant a single large file could OOM the process.
async fn encrypt_destination(args: &Args, inventory: &ScanSummary, key_spec: &str) -> Result<usize> {
    let key = robocopy_ingest::crypto::resolve_key(key_spec)?;
    let manager = CryptoManager::new(&key)?;
    let dest_root = args.dest().to_path_buf();
    let files = inventory.files.clone();

    let count = tokio::task::spawn_blocking(move || -> Result<usize, IngestError> {
        let mut encrypted = 0usize;
        for file in &files {
            let path = dest_root.join(&file.relative_path);
            if !path.is_file() {
                continue; // file missing at dest (copy skipped/failed): nothing to encrypt
            }
            manager.encrypt_file(&path)?;
            encrypted += 1;
        }
        Ok(encrypted)
    })
    .await
    .context("the encryption task panicked")??;

    if count > 0 {
        tracing::info!(count, "destination files encrypted with AES-256-GCM");
        println!("Encrypted {count} file(s) in the destination with AES-256-GCM.");
    }
    Ok(count)
}

/// Decrypt every file the transfer touched, in place in the destination, with AES-256-GCM — the
/// counterpart to [`encrypt_destination`]. Streams each file in fixed-size chunks for the same
/// anti-OOM reason (F25a). Typically run as part of `--restore-from`, where "the destination" is
/// the original source path the backup is being restored to.
async fn decrypt_destination(args: &Args, inventory: &ScanSummary, key_spec: &str) -> Result<usize> {
    let key = robocopy_ingest::crypto::resolve_key(key_spec)?;
    let manager = CryptoManager::new(&key)?;
    let dest_root = args.dest().to_path_buf();
    let files = inventory.files.clone();

    let count = tokio::task::spawn_blocking(move || -> Result<usize, IngestError> {
        let mut decrypted = 0usize;
        for file in &files {
            let path = dest_root.join(&file.relative_path);
            if !path.is_file() {
                continue; // file missing at dest (copy skipped/failed): nothing to decrypt
            }
            manager.decrypt_file(&path)?;
            decrypted += 1;
        }
        Ok(decrypted)
    })
    .await
    .context("the decryption task panicked")??;

    if count > 0 {
        tracing::info!(count, "destination files decrypted with AES-256-GCM");
        println!("Decrypted {count} file(s) in the destination.");
    }
    Ok(count)
}

/// Run the robocopy transfer with live progress and the outer retry loop.
///
/// An [`IngestError::CopyFailed`] is returned alongside a best-effort outcome so the JSON report is
/// still produced; any other error aborts the run.
async fn transfer(
    args: &Args,
    inventory: &ScanSummary,
    child_pid: Arc<AtomicU32>,
) -> Result<(CopyOutcome, Option<IngestError>)> {
    let progress = new_progress(args, inventory.total_bytes, "robocopy");
    let poller = spawn_dest_poller(args, Arc::clone(&progress));

    let request = args.copy_request(args.dest().to_path_buf());
    let policy = args.retry_policy();
    let sink = Arc::clone(&progress);
    let started = Instant::now();

    let result = tokio::task::spawn_blocking(move || {
        let engine = RobocopyEngine::new_with_pid_slot(child_pid);
        engine::run_with_retries(&engine, &request, sink.as_ref(), &policy, &ThreadSleeper)
    })
    .await
    .context("the robocopy task panicked")?;

    let elapsed = started.elapsed();
    if let Some(poller) = poller {
        poller.abort();
    }

    match result {
        Ok(outcome) => {
            progress.finish(format!(
                "done: {} at {:.1} MB/s",
                format_bytes(outcome.bytes_copied),
                progress.average_mbps()
            ));
            Ok((outcome, None))
        }
        Err(IngestError::CopyFailed {
            code,
            description,
            attempts,
        }) => {
            progress.finish("failed");
            // Report what actually reached the destination before giving up.
            //
            // The progress sink is reset at the start of every retry attempt (so a failing
            // attempt's partial bytes don't get added on top of the next one — see
            // engine::run_with_retries). That fixes double-counting across attempts, but it
            // means the sink only reflects the *last* attempt here, which badly undercounts a
            // run where most files succeeded on an earlier attempt and only a persistent,
            // non-transient per-file failure (e.g. a source file literally named a reserved
            // Windows device name like `NUL`, which can never be copied no matter how many
            // times robocopy retries) kept exhausting the retry budget. A real scan of what's
            // actually on disk at the destination is the only way to get a trustworthy total
            // in that case.
            let dest_for_count = args.dest().to_path_buf();
            let follow_links = !args.exclude_junctions;
            let observed =
                tokio::task::spawn_blocking(move || scan::inventory(&dest_for_count, "*", follow_links))
                    .await
                    .ok()
                    .and_then(Result::ok);

            let mut outcome = CopyOutcome::new(robocopy_ingest::engine::robocopy::ENGINE_NAME);
            outcome.exit_code = Some(code);
            outcome.retry_attempts_used = attempts.saturating_sub(1);
            outcome.elapsed = elapsed;
            match observed {
                Some(dest_inventory) => {
                    outcome.bytes_copied = dest_inventory.total_bytes;
                    outcome.files_copied = dest_inventory.total_files;
                }
                None => {
                    // Destination scan itself failed (e.g. share became unreachable): fall back
                    // to the sink's last-attempt numbers rather than reporting nothing.
                    outcome.bytes_copied = progress.current_bytes();
                    outcome.files_copied = progress.files();
                }
            }
            outcome.dry_run = args.dry_run;
            Ok((
                outcome,
                Some(IngestError::CopyFailed {
                    code,
                    description,
                    attempts,
                }),
            ))
        }
        Err(error) => {
            progress.finish("aborted");
            Err(error).context("the robocopy transfer could not be performed")
        }
    }
}

/// Time the naive baseline copy into a temporary directory next to the destination.
async fn baseline(args: &Args, inventory: &ScanSummary) -> Result<CopyOutcome> {
    let temp = baseline_dir(args.dest())?;
    let dest = temp.path().to_path_buf();
    tracing::info!(path = %dest.display(), "starting naive baseline copy");
    println!("\nRunning the naive baseline copy (single threaded, file by file)...");

    let progress = new_progress(args, inventory.total_bytes, "baseline");
    let request = args.copy_request(dest);
    let sink = Arc::clone(&progress);

    let outcome =
        tokio::task::spawn_blocking(move || NaiveCopyEngine::new().copy(&request, sink.as_ref()))
            .await
            .context("the baseline task panicked")?
            .context("the naive baseline copy failed")?;

    progress.finish(format!(
        "done: {} at {:.1} MB/s",
        format_bytes(outcome.bytes_copied),
        progress.average_mbps()
    ));

    // Dropping the TempDir removes the baseline copy; it exists only to be timed.
    drop(temp);
    Ok(outcome)
}

/// Prefer a temporary directory on the destination volume, so the comparison is fair.
fn baseline_dir(dest: &Path) -> Result<TempDir> {
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        if std::fs::create_dir_all(parent).is_ok() {
            if let Ok(dir) = TempDir::with_prefix_in("robocopy-ingest-baseline-", parent) {
                return Ok(dir);
            }
        }
    }
    tracing::warn!("falling back to the system temp directory for the baseline copy");
    TempDir::with_prefix("robocopy-ingest-baseline-")
        .context("cannot create the temporary directory for the baseline copy")
}

async fn verify(args: &Args, inventory: &ScanSummary) -> Result<IntegrityCheck> {
    println!("\nVerifying integrity with {:?}...", args.hash_algo);
    let progress = new_progress(args, inventory.total_bytes, "verify");

    let source = args.source().to_path_buf();
    let dest = args.dest().to_path_buf();
    let files = inventory.files.clone();
    let algo = args.hash_algo;
    let sink = Arc::clone(&progress);

    let check = tokio::task::spawn_blocking(move || {
        integrity::verify(&source, &dest, &files, algo, sink.as_ref())
    })
    .await
    .context("the integrity task panicked")?;

    // F26a (closes half of D2): treat transient files (.log/.tmp/.git/objects) that vanished
    // between the copy and this verification pass as expected rather than a failure.
    let check = if args.ignore_transient_missing {
        integrity::ignore_transient_missing(check)
    } else {
        check
    };

    progress.finish(if check.passed() {
        "integrity PASSED"
    } else {
        "integrity FAILED"
    });
    tracing::info!(
        files_checked = check.files_checked,
        mismatches = check.mismatches.len(),
        missing = check.missing_in_dest.len(),
        "integrity verification complete"
    );
    Ok(check)
}

fn new_progress(args: &Args, total_bytes: u64, label: &str) -> Arc<ThroughputProgress> {
    if args.dry_run {
        ThroughputProgress::hidden(total_bytes)
    } else {
        ThroughputProgress::new(total_bytes, label)
    }
}

/// Sample the destination size so the bar keeps moving even while robocopy buffers its output.
fn spawn_dest_poller(
    args: &Args,
    progress: Arc<ThroughputProgress>,
) -> Option<tokio::task::JoinHandle<()>> {
    if args.dry_run {
        return None;
    }
    let dest = args.dest().to_path_buf();
    let follow_links = !args.exclude_junctions;
    let already_present = scan::directory_size(&dest, follow_links);

    Some(tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        loop {
            ticker.tick().await;
            let sampled = dest.clone();
            match tokio::task::spawn_blocking(move || scan::directory_size(&sampled, follow_links)).await {
                Ok(size) => progress.observe_total_bytes(size.saturating_sub(already_present)),
                Err(_) => break,
            }
        }
    }))
}
