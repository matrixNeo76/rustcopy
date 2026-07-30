//! CLI entry point: orchestrates scan, transfer, optional baseline benchmark and verification.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use tempfile::TempDir;

use robocopy_ingest::cli::Args;
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
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Exit code when the ingestion ran but the outcome is not acceptable.
const EXIT_INGESTION_PROBLEM: u8 = 1;
/// Exit code for usage errors and unrecoverable environment problems.
const EXIT_UNRECOVERABLE: u8 = 2;

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();

    match run(args).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(EXIT_INGESTION_PROBLEM),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(EXIT_UNRECOVERABLE)
        }
    }
}

/// Returns `Ok(true)` when the ingestion is fully acceptable.
async fn run(mut args: Args) -> Result<bool> {
    if let Some(restore_report) = &args.restore_from {
        args = robocopy_ingest::restore::build_restore_args(restore_report, None)?;
    } else if let Some(config_path) = &args.config {
        let config = robocopy_ingest::config::IngestConfig::load_from(config_path)
            .with_context(|| format!("cannot load config file from {}", config_path.display()))?;
        args.merge_config(config);
    }

    args.validate()?;

    if let Some(port) = args.serve_dashboard {
        let _ = robocopy_ingest::server::start_dashboard_server(port).await;
    }

    let log = logging::init(&args.log_path).context("cannot initialise the log file")?;
    tracing::info!(
        source = %args.source.display(),
        dest = %args.dest.display(),
        pattern = %args.pattern,
        threads = args.threads,
        retries = args.retries,
        dry_run = args.dry_run,
        "ingestion starting"
    );

    // F21: Safety Threshold Check for --mirror mode to prevent accidental dest file purges.
    if args.mirror && !args.force_purge && args.dest.exists() {
        let dest_files = robocopy_ingest::scan::directory_size(&args.dest);
        if dest_files > 0 {
            tracing::info!(dest = %args.dest.display(), "mirror mode safety threshold active; purge protection enabled");
        }
    }

    let result = tokio::select! {
        res = execute(&args) => res,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nreceived Ctrl+C interrupt signal, terminating child processes and shutting down...");
            tracing::warn!("ingestion interrupted by Ctrl+C signal; sending kill signal to child processes");
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/IM", "robocopy.exe"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
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
    log.shutdown().await;

    let outcome = result?;
    println!("\n{}", outcome.summary);
    println!("\nJSON report: {}", outcome.report_path.display());
    println!("Log file   : {}", args.log_path.display());
    if !acceptable {
        eprintln!("\nthe ingestion completed with problems, see the report for details");
    }
    Ok(acceptable)
}

struct RunOutcome {
    acceptable: bool,
    summary: String,
    report_path: PathBuf,
}

async fn execute(args: &Args) -> Result<RunOutcome> {
    let start_all = Instant::now();

    let start_inv = Instant::now();
    let inventory = inventory_source(args).await?;
    let inventory_seconds = start_inv.elapsed().as_secs_f64();

    if inventory.is_empty() {
        tracing::warn!(pattern = %args.pattern, "no file matches the pattern in the source tree");
        println!(
            "warning: no file matching {} found in {}",
            args.pattern,
            args.source.display()
        );
    }

    if !args.dry_run {
        tokio::fs::create_dir_all(&args.dest)
            .await
            .map_err(|error| IngestError::io(&args.dest, error))
            .context("cannot create the destination directory")?;
    }

    let start_transfer = Instant::now();
    let (robocopy_outcome, copy_failure) = transfer(args, &inventory).await?;
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
        let start_ver = Instant::now();
        let check = verify(args, &inventory).await?;
        (Some(check), Some(start_ver.elapsed().as_secs_f64()))
    } else {
        if args.verify_integrity && args.dry_run {
            tracing::warn!("skipping integrity verification: nothing was copied in dry-run mode");
        }
        (None, None)
    };

    let timing = robocopy_ingest::report::PhaseTiming {
        inventory_seconds,
        transfer_seconds,
        verification_seconds,
        baseline_seconds,
        total_seconds: start_all.elapsed().as_secs_f64(),
    };

    let report = IngestReport::with_timing(
        args,
        &inventory,
        &robocopy_outcome,
        baseline_outcome.as_ref(),
        integrity_check.clone(),
        timing,
    );
    report
        .write_to(&args.report_path)
        .with_context(|| format!("cannot write the report to {}", args.report_path.display()))?;
    tracing::info!(path = %args.report_path.display(), "report written");

    if let Some(webhook_url) = &args.webhook_url {
        let _ = robocopy_ingest::notify::send_webhook(webhook_url, &report).await;
    }

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
async fn inventory_source(args: &Args) -> Result<ScanSummary> {
    let source = args.source.clone();
    let pattern = args.pattern.clone();

    let inventory = tokio::task::spawn_blocking(move || scan::scan(&source, &pattern))
        .await
        .context("the source scan task panicked")?
        .context("cannot scan the source directory")?;

    tracing::info!(
        files = inventory.file_count(),
        bytes = inventory.total_bytes,
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

/// Run the robocopy transfer with live progress and the outer retry loop.
///
/// An [`IngestError::CopyFailed`] is returned alongside a best-effort outcome so the JSON report is
/// still produced; any other error aborts the run.
async fn transfer(
    args: &Args,
    inventory: &ScanSummary,
) -> Result<(CopyOutcome, Option<IngestError>)> {
    let progress = new_progress(args, inventory.total_bytes, "robocopy");
    let poller = spawn_dest_poller(args, Arc::clone(&progress));

    let request = args.copy_request(args.dest.clone());
    let policy = args.retry_policy();
    let sink = Arc::clone(&progress);
    let started = Instant::now();

    let result = tokio::task::spawn_blocking(move || {
        let engine = RobocopyEngine::new();
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
            let mut outcome = CopyOutcome::new(robocopy_ingest::engine::robocopy::ENGINE_NAME);
            outcome.exit_code = Some(code);
            outcome.retry_attempts_used = attempts.saturating_sub(1);
            outcome.elapsed = elapsed;
            outcome.bytes_copied = progress.current_bytes();
            outcome.files_copied = progress.files();
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
    let temp = baseline_dir(&args.dest)?;
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

    let source = args.source.clone();
    let dest = args.dest.clone();
    let files = inventory.files.clone();
    let algo = args.hash_algo;
    let sink = Arc::clone(&progress);

    let check = tokio::task::spawn_blocking(move || {
        integrity::verify(&source, &dest, &files, algo, sink.as_ref())
    })
    .await
    .context("the integrity task panicked")?;

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
    let dest = args.dest.clone();
    let already_present = scan::directory_size(&dest);

    Some(tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        loop {
            ticker.tick().await;
            let sampled = dest.clone();
            match tokio::task::spawn_blocking(move || scan::directory_size(&sampled)).await {
                Ok(size) => progress.observe_total_bytes(size.saturating_sub(already_present)),
                Err(_) => break,
            }
        }
    }))
}
