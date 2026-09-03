//! CLI entry point: orchestrates scan, transfer, optional baseline benchmark and verification.

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicU32;
#[cfg(windows)]
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use tempfile::TempDir;
use tracing::Instrument;

use robocopy_ingest::cache::{self, IngestCache};
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
use robocopy_ingest::scan::{self, ScanSummary, ScannedFile};

/// How often the destination directory is sampled to estimate live throughput.
///
/// F2.1 fix: this used to be 500ms, which meant a full recursive walk of the destination
/// (potentially a large SMB share) every half second — often more expensive than the transfer
/// itself. Robocopy's own per-file stdout output (parsed in `engine::robocopy`) already drives
/// the bar for the common case; this poller only exists to keep it moving during long single-file
/// copies, so a long interval is sufficient.
const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Exit code when the transfer itself failed (robocopy exhausted its retries on some item).
const EXIT_INGESTION_PROBLEM: u8 = 1;
/// Exit code for usage errors and unrecoverable environment problems.
const EXIT_UNRECOVERABLE: u8 = 2;
/// Exit code when `--mirror` was aborted because it would have purged destination files and
/// neither `--force-purge` nor an interactive confirmation was given.
const EXIT_MIRROR_ABORTED: u8 = 3;
/// F29b (closes half of D-none/O7): exit code when the transfer itself succeeded but
/// `--verify-integrity` found a mismatch/missing/unreadable file. Previously this collapsed into
/// the same `EXIT_INGESTION_PROBLEM` (1) as an actual robocopy transfer failure, which a scheduler
/// can't tell apart from "some files couldn't be copied at all" — a materially different failure
/// mode (data landed but doesn't match, vs. data never landed).
const EXIT_INTEGRITY_FAILED: u8 = 4;
/// F35: exit code when `--keep-generations` was aborted because it would have deleted old
/// generation folders and neither `--force-purge` nor an interactive confirmation was given.
const EXIT_RETENTION_PURGE_ABORTED: u8 = 5;

/// F37: a plain (non-`#[tokio::main]`) entry point on purpose. `windows_service::service_dispatcher`
/// hands control to SCM's dispatch loop by blocking the calling OS thread until the service stops
/// — it must run on a plain thread, not a tokio runtime worker, and ideally before any tokio
/// runtime exists at all. So this checks for the internal service-launch marker directly against
/// raw argv, entirely before clap parsing or building a `Runtime`, and only builds the tokio
/// runtime for the normal (non-service) path.
fn main() -> ExitCode {
    if robocopy_ingest::service::is_service_launch() {
        return match robocopy_ingest::service::run_service_dispatcher() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error:#}");
                ExitCode::from(EXIT_UNRECOVERABLE)
            }
        };
    }

    let runtime = tokio::runtime::Runtime::new().expect("failed to build the tokio async runtime");
    runtime.block_on(async_main())
}

async fn async_main() -> ExitCode {
    let args = Args::parse();

    match run(args).await {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            match error.downcast_ref::<IngestError>() {
                Some(IngestError::MirrorPurgeAborted { .. }) => ExitCode::from(EXIT_MIRROR_ABORTED),
                Some(IngestError::RetentionPurgeAborted { .. }) => {
                    ExitCode::from(EXIT_RETENTION_PURGE_ABORTED)
                }
                _ => ExitCode::from(EXIT_UNRECOVERABLE),
            }
        }
    }
}

/// Returns the process exit code: `0` on a fully acceptable run, or one of the `EXIT_*`
/// constants above otherwise.
async fn run(mut args: Args) -> Result<u8> {
    // F36: a pure meta-operation — remove a previously installed Task Scheduler entry and exit,
    // without touching --restore-from/--resume-from/--config or requiring --source/--dest at all
    // (clap's required_unless_present_any already exempts it; this early return skips the rest of
    // this function's args-mutation chain entirely, same idea as the restore/resume branches
    // below but even earlier since it needs none of their machinery).
    if let Some(name) = args.uninstall_schedule.clone() {
        robocopy_ingest::schedule::uninstall(&name)
            .with_context(|| format!("cannot uninstall the scheduled task {name:?}"))?;
        println!("removed scheduled task '{name}'");
        return Ok(0);
    }

    // F37: same pure-meta-operation idea as --uninstall-schedule above — registers/removes the
    // Windows service and exits, no --source/--dest needed (this registers the binary itself, not
    // any particular backup invocation).
    if args.install_service {
        robocopy_ingest::service::install().context("cannot install the Windows service")?;
        println!(
            "Windows service installed (start type: OnDemand). Start it with:\n  sc start RustcopyIngestService\nor via services.msc."
        );
        return Ok(0);
    }
    if args.uninstall_service {
        robocopy_ingest::service::uninstall().context("cannot uninstall the Windows service")?;
        println!("Windows service removed.");
        return Ok(0);
    }

    // Fase 1 of VALUTAZIONE_AI.md: read-only analysis of this destination's run history. Placed
    // with the other meta-operations above rather than inside execute(): like them it needs no
    // --source, takes no lock, spawns no child process and copies nothing. It cannot fail a
    // backup because it never runs alongside one.
    // F56: credential management, intercepted with the other meta-operations. Touches no path,
    // copies nothing, and exits — so none of the transfer machinery below is involved.
    if let Some(name) = args.set_credential.clone() {
        #[cfg(windows)]
        {
            use std::io::Read;
            // From stdin, never from an argument: a secret on the command line is visible in the
            // process list to any user on the machine, which is precisely the exposure the literal
            // form of --encrypt-aes256 warns about.
            let mut secret = String::new();
            std::io::stdin()
                .read_to_string(&mut secret)
                .context("cannot read the secret from stdin")?;
            // Only the terminal line ending, not every trailing space: a passphrase may legally
            // end in whitespace, and silently altering a secret is the worst shape this bug could
            // take -- it would surface as a decryption failure later, far from the cause.
            let secret = secret.strip_suffix('\n').unwrap_or(&secret);
            let secret = secret.strip_suffix('\r').unwrap_or(secret);
            if secret.is_empty() {
                anyhow::bail!(
                    "no secret on stdin. Pipe it in:  echo <secret> | robocopy_ingest --set-credential {name}"
                );
            }
            robocopy_ingest::crypto::write_credential(&name, secret)?;
            println!("stored credential '{name}'. Use it as: keyring:{name}");
            return Ok(0);
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            anyhow::bail!(
                "--set-credential needs the Windows Credential Manager, which this platform does                  not have. Use env:NAME or file:PATH instead."
            );
        }
    }
    if let Some(name) = args.delete_credential.clone() {
        #[cfg(windows)]
        {
            robocopy_ingest::crypto::delete_credential(&name)?;
            println!("removed credential '{name}'");
            return Ok(0);
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            anyhow::bail!("--delete-credential needs the Windows Credential Manager.");
        }
    }

    if args.advise {
        // Keyed off --report-path, because that is where the index lives (see
        // `RunHistory::path_for`): pass the same --report-path the runs used. With the default
        // report path, plain `--advise` finds the history in the current directory.
        let report_path = args.report_path.clone();
        let job_name = args.job_name.clone();
        let history = robocopy_ingest::history::RunHistory::load_recent(
            &report_path,
            job_name.as_deref(),
            robocopy_ingest::history::DEFAULT_HISTORY_WINDOW,
        )
        .with_context(|| {
            format!(
                "cannot read the run history beside {}",
                report_path.display()
            )
        })?;
        let advice = robocopy_ingest::advise::analyse(&history);
        print!("{}", robocopy_ingest::advise::render(&advice, &history));
        return Ok(0);
    }

    if let Some(restore_report) = args.restore_from.clone() {
        args = robocopy_ingest::restore::build_restore_args(&args, &restore_report, None)?;
    } else if let Some(checkpoint_path) = args.resume_from.clone() {
        args = robocopy_ingest::checkpoint::build_resume_args(&args, &checkpoint_path)
            .with_context(|| {
                format!(
                    "cannot resume from checkpoint {}",
                    checkpoint_path.display()
                )
            })?;
    } else if let Some(config_path) = &args.config {
        let config = robocopy_ingest::config::IngestConfig::load_from(config_path)
            .with_context(|| format!("cannot load config file from {}", config_path.display()))?;
        // F33: a `[[jobs]]` array switches into multi-job mode, handled entirely separately —
        // each job gets its own `Args` built from a fresh clone of the original CLI invocation
        // (see `run_jobs`). An empty/absent `jobs` array is the pre-F33 single-job path.
        if config.jobs.as_ref().is_some_and(|jobs| !jobs.is_empty()) {
            return run_jobs(args, config).await;
        }
        args.merge_config(config);
    }

    // P1: resolved once, here, right before validate() -- after the restore/resume/config
    // branches above have all had their say on report_path, and before anything downstream
    // (checkpoint_path_for on Ctrl+C in run_one, the final write_to, P2's read_previous_report)
    // reads it. A path with no `{timestamp}` placeholder is untouched.
    args.report_path =
        robocopy_ingest::resolve_report_path_timestamp(&args.report_path, chrono::Utc::now());

    args.validate()?;

    // Once, here, and not inside `validate()` — which `run_jobs` calls per job. A stop file
    // created while a batch is running is a legitimate signal that the current job must handle as
    // an interruption; checking it per job turned that signal into a configuration error for
    // every remaining job. Here it does what it was for: catching one left behind by an earlier
    // run, before anything is copied.
    args.validate_cancel_file_absent()?;

    // F36: install the current invocation (minus the scheduling flags themselves) as a recurring
    // Task Scheduler entry, then exit without running a backup now. Runs after validate() (unlike
    // --uninstall-schedule above) because installing a schedule is only useful for a genuinely
    // runnable invocation — if --source/--dest/etc. don't validate, better to fail now than
    // silently schedule a command that will fail every time it fires.
    if let Some(spec_raw) = args.install_schedule.clone() {
        let spec = robocopy_ingest::schedule::parse_schedule_spec(&spec_raw)?;
        let name = args
            .schedule_name
            .clone()
            .unwrap_or_else(|| "rustcopy".to_string());
        let exe_path =
            std::env::current_exe().context("cannot determine the current executable path")?;
        let raw_args: Vec<String> = std::env::args().skip(1).collect();
        let filtered_args = robocopy_ingest::schedule::strip_schedule_flags(&raw_args);
        let task_run = robocopy_ingest::schedule::build_task_run_command(&exe_path, &filtered_args);
        robocopy_ingest::schedule::install(&name, &spec, &task_run)
            .with_context(|| format!("cannot install the scheduled task {name:?}"))?;
        println!("installed scheduled task '{name}' ({spec_raw})\n  runs: {task_run}");
        return Ok(0);
    }

    let log = logging::init(&args.log_path, &args.log_config())
        .context("cannot initialise the log file")?;
    let child_pid = Arc::new(AtomicU32::new(0));

    let exit_code = match run_one(&args, &log, Arc::clone(&child_pid)).await? {
        JobRunResult::Completed(code) => code,
        JobRunResult::Interrupted(_) => EXIT_INGESTION_PROBLEM,
    };

    let dropped = log.dropped_lines();
    log.shutdown().await;
    if dropped > 0 {
        eprintln!("warning: {dropped} log line(s) were dropped (logger under load)");
    }
    Ok(exit_code)
}

/// F33: run every job declared in a `[[jobs]]` config file sequentially, in one process.
///
/// Each job gets its own `Args`, rebuilt from a fresh clone of the *original* CLI invocation (not
/// the previous job's already-merged `Args`) so `apply_job_config`'s "CLI still holds clap's own
/// default" checks behave identically to the single-job path for every job, not just the first.
/// Logging is a whole-process resource — `logging::init` only actually installs a subscriber on
/// its first call (see its doc comment) — so all jobs in a batch necessarily share one log file,
/// picked from the file's top-level defaults (or the CLI default if unset). Each job still needs
/// its own report (and, on Ctrl+C, its own checkpoint): if a job doesn't set its own
/// `report_path`, one is namespaced with the job's name to avoid jobs silently overwriting each
/// other's report.
///
/// A job that fails validation (e.g. missing source/dest) is reported and skipped rather than
/// aborting the whole batch — one misconfigured job in a multi-job file shouldn't stop the others
/// from running. A Ctrl+C, however, aborts the remaining jobs immediately.
async fn run_jobs(base_args: Args, config: robocopy_ingest::config::IngestConfig) -> Result<u8> {
    let jobs = config.jobs.clone().unwrap_or_default();

    let mut log_args = base_args.clone();
    log_args.apply_job_config(&config.defaults);
    let log = logging::init(&log_args.log_path, &log_args.log_config())
        .context("cannot initialise the log file")?;

    println!(
        "running {} job(s) from {}",
        jobs.len(),
        log_args
            .config
            .as_deref()
            .unwrap_or(Path::new("<config>"))
            .display()
    );

    let child_pid = Arc::new(AtomicU32::new(0));
    let mut worst_exit_code: u8 = 0;

    for (idx, job) in jobs.iter().enumerate() {
        let resolved = job.merged_over(&config.defaults);
        let job_name = resolved
            .name
            .clone()
            .unwrap_or_else(|| format!("job{}", idx + 1));

        let mut job_args = base_args.clone();
        job_args.apply_job_config(&resolved);

        if job_args.source.is_none() || job_args.dest.is_none() {
            eprintln!(
                "error: job '{job_name}' is missing source/dest (set them on the job itself or as top-level config defaults) — skipping"
            );
            worst_exit_code = worst_exit_code.max(EXIT_UNRECOVERABLE);
            continue;
        }
        // Namespace off the *job's own* `report_path`, not `resolved`'s: `merged_over` already
        // folded the top-level default's `report_path` into `resolved` even when the job itself
        // never set one, which would otherwise defeat this check every time (every job "resolves"
        // to a report_path as long as the file has one at all) and send every job's report to the
        // exact same path.
        if job.report_path.is_none() {
            job_args.report_path =
                robocopy_ingest::namespaced_path(&job_args.report_path, &job_name);
        }
        // Same treatment, same reason: an HTML dashboard inherited from the shared defaults would
        // otherwise be written to one path by every job, so only the last one would survive. The
        // `job.html_report_path.is_none()` check mirrors the one above -- namespace only what the
        // job did not set for itself.
        if job.html_report_path.is_none() {
            if let Some(html) = &job_args.html_report_path {
                job_args.html_report_path = Some(robocopy_ingest::namespaced_path(html, &job_name));
            }
        }
        // Cache (`.ingest_cache`) and the generations manifest (`.rustcopy_generations.json`) have
        // no user-facing config field to namespace explicitly, unlike report_path above — always
        // namespace them with the job name in a multi-job run, otherwise jobs sharing a `dest`
        // would silently read/write each other's fast-verify cache and generation history (D12).
        job_args.job_name = Some(job_name.clone());
        // P1: resolved per job, after namespacing above (order between the two doesn't matter --
        // they touch disjoint parts of the filename) and before validate(), each job getting its
        // own timestamp captured at its own start rather than one shared across the whole batch,
        // since jobs run sequentially and a later job in a long batch can start meaningfully
        // later than the first.
        job_args.report_path = robocopy_ingest::resolve_report_path_timestamp(
            &job_args.report_path,
            chrono::Utc::now(),
        );
        if let Err(error) = job_args.validate() {
            eprintln!("error: job '{job_name}' failed validation: {error} — skipping");
            worst_exit_code = worst_exit_code.max(EXIT_UNRECOVERABLE);
            continue;
        }

        println!("\n=== job '{job_name}' ({}/{}) ===", idx + 1, jobs.len());
        // Every event logged while this job runs (not just the "starting job" line below) must
        // carry the job's identity: `run_one` shares one log file across the whole batch (see its
        // own doc comment), and without a span wrapping it, two jobs failing in close succession
        // would be indistinguishable from the log file alone.
        let job_span = tracing::info_span!("job", job = %job_name);
        let result = async {
            tracing::info!("starting job");
            run_one(&job_args, &log, Arc::clone(&child_pid)).await
        }
        .instrument(job_span)
        .await?;

        match result {
            JobRunResult::Completed(code) => worst_exit_code = worst_exit_code.max(code),
            JobRunResult::Interrupted(reason) => {
                worst_exit_code = worst_exit_code.max(EXIT_INGESTION_PROBLEM);
                eprintln!("aborting remaining jobs: {reason}");
                break;
            }
        }
    }

    let dropped = log.dropped_lines();
    log.shutdown().await;
    if dropped > 0 {
        eprintln!("warning: {dropped} log line(s) were dropped (logger under load)");
    }
    Ok(worst_exit_code)
}

/// Outcome of a single job run: either it completed (with the exit code the caller should surface)
/// or Ctrl+C interrupted it (a checkpoint was written, if possible).
enum JobRunResult {
    Completed(u8),
    /// Carries why, because there is more than one way in now: reporting "after Ctrl+C" for a
    /// stop that came from `--cancel-file` would contradict the checkpoint written beside it.
    Interrupted(String),
}

/// Runs one already-validated, already-logged-in job: the transfer itself, its Ctrl+C handling,
/// and its summary output. Shared by the single-job path in `run` and F33's multi-job path in
/// `run_jobs`, both of which own the `LogHandle`'s init/flush/shutdown lifecycle themselves since
/// a batch of jobs shares one log file across multiple calls to this function.
async fn run_one(
    args: &Args,
    log: &logging::LogHandle,
    child_pid: Arc<AtomicU32>,
) -> Result<JobRunResult> {
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
    let result = tokio::select! {
        res = execute(args, Arc::clone(&child_pid)) => res,
        // Both arms of an interruption share the body below rather than each having their
        // own, which is the point: the checkpoint guarantee is written once. `interrupted`
        // is plain Ctrl+C when `--cancel-file` was not given, so a run without the flag
        // behaves exactly as it did before the flag existed.
        reason = interrupted(args.cancel_file.as_deref()) => {
            eprintln!("\n{reason}, terminating the active transfer...");
            tracing::warn!(%reason, "ingestion interrupted");
            kill_active_child(&child_pid);

            // F31: nothing was written to record an interrupted run before this existed, so
            // --resume-from had nothing to read. Best-effort: a failure to write the checkpoint
            // must not mask the Ctrl+C itself, only be reported.
            let checkpoint_path = robocopy_ingest::checkpoint::checkpoint_path_for(&args.report_path);
            let checkpoint = robocopy_ingest::checkpoint::Checkpoint::new(args, &reason);
            match checkpoint.write_to(&checkpoint_path) {
                Ok(()) => eprintln!(
                    "Checkpoint written to {} — resume with --resume-from {}",
                    checkpoint_path.display(),
                    checkpoint_path.display()
                ),
                Err(error) => {
                    tracing::warn!(error = %error, "could not write the interruption checkpoint");
                    eprintln!("warning: could not write the interruption checkpoint: {error}");
                }
            }

            log.flush().await;
            return Ok(JobRunResult::Interrupted(reason));
        }
    };

    log.flush().await;

    match &result {
        Ok(outcome) => tracing::info!(exit_code = outcome.exit_code, "ingestion finished"),
        Err(error) => tracing::error!("ingestion aborted: {error:#}"),
    }

    let outcome = result?;
    println!("\n{}", outcome.summary);
    println!("\nJSON report: {}", outcome.report_path.display());
    println!("Log file   : {}", args.log_path.display());
    if outcome.exit_code != 0 {
        eprintln!("\nthe ingestion completed with problems, see the report for details");
    }
    Ok(JobRunResult::Completed(outcome.exit_code))
}

/// Resolves when the run should stop, describing why.
///
/// Two ways in. Ctrl+C is the one a person at a terminal uses. The other is `--cancel-file`, for a
/// supervisor with no terminal to send Ctrl+C from: on Windows `GenerateConsoleCtrlEvent` requires
/// the caller to be attached to a console and the child to sit in its own process group, and a GUI
/// built with `windows_subsystem = "windows"` has no console at all — while killing the process
/// outright would skip the checkpoint, which is exactly what makes an interruption resumable.
///
/// Both resolve into the *same* caller branch. Deliberately: a second path that also had to
/// remember to write a checkpoint is a second path that can forget to.
async fn interrupted(cancel_file: Option<&Path>) -> String {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
        "received Ctrl+C interrupt signal".to_string()
    };

    match cancel_file {
        None => ctrl_c.await,
        Some(path) => {
            let path = path.to_path_buf();
            tokio::select! {
                reason = ctrl_c => reason,
                _ = wait_for_cancel_file(path.clone()) => {
                    format!("stop requested via {}", path.display())
                }
            }
        }
    }
}

/// Polls for the cancel file.
///
/// Polling rather than a filesystem watcher: one `metadata` call twice a second against a path the
/// supervisor chose is unmeasurable next to a backup, and a watcher would add a dependency and a
/// platform-specific failure mode to a mechanism whose whole appeal is that you can see it work by
/// looking at a folder.
async fn wait_for_cancel_file(path: PathBuf) {
    loop {
        if tokio::fs::metadata(&path).await.is_ok() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Terminate only the tracked child PID (if any), never every `robocopy.exe` on the host.
#[cfg(windows)]
fn kill_active_child(child_pid: &Arc<AtomicU32>) {
    let pid = child_pid.load(Ordering::SeqCst);
    if pid == 0 {
        return;
    }
    tracing::warn!(
        pid,
        "sending kill signal to the tracked robocopy.exe child process"
    );
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(not(windows))]
fn kill_active_child(_child_pid: &Arc<AtomicU32>) {}

/// D13: `tokio::task::spawn_blocking` runs its closure on a dedicated blocking-pool thread, which
/// does *not* inherit the calling task's active `tracing` span — every blocking filesystem/process
/// operation in this file (the robocopy/naive transfer itself, integrity checks, VSS, hooks, ...)
/// would otherwise log without the job identity that `run_jobs` (F33) attaches via its `job` span,
/// making a multi-job batch's shared log file impossible to attribute per job for exactly the lines
/// that matter most (the actual robocopy invocation and its per-file output). This captures the
/// current span before handing off to the blocking thread and re-enters it there, so every event
/// logged inside `f` still carries it. A drop-in replacement for `tokio::task::spawn_blocking`,
/// same bounds — every call site in this file should go through this instead of the bare
/// `tokio::task::spawn_blocking`.
fn spawn_blocking_with_span<F, T>(f: F) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(f))
}

/// F30: holds a VSS shadow copy for the lifetime of the run and deletes it on drop.
///
/// A plain synchronous `Drop` impl (rather than an async cleanup step at the end of `execute()`)
/// is deliberate: it is the only way cleanup still runs when `execute()`'s future is *cancelled*
/// rather than completed — which is exactly what happens on Ctrl+C, since `run()`'s
/// `tokio::select!` drops the losing branch's future outright instead of driving it to
/// completion. Dropping a Rust future still runs `Drop` for its live locals at the point of
/// cancellation, so a `VssGuard` held directly in `execute()`'s local scope (never moved into a
/// `spawn_blocking` closure, which *would* detach it from the future and defeat this) is cleaned
/// up on every exit path: normal completion, an early `?` return, or Ctrl+C.
struct VssGuard {
    /// Only read on Windows (inside `Drop::drop`'s `#[cfg(windows)]` block below) — the
    /// non-Windows `create_vss_snapshot` stub never constructs a real shadow copy at all.
    #[cfg_attr(not(windows), allow(dead_code))]
    shadow_id: String,
    /// The path robocopy/scan should actually read from instead of the live volume.
    remapped_source: PathBuf,
}

impl Drop for VssGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Err(error) = robocopy_ingest::vss::delete_shadow_copy(&self.shadow_id) {
            tracing::warn!(
                shadow_id = %self.shadow_id,
                error = %error,
                "failed to delete VSS shadow copy; it may need manual cleanup: `vssadmin delete shadows /shadow={}`",
                self.shadow_id,
            );
            eprintln!(
                "warning: could not delete VSS shadow copy {} — run `vssadmin delete shadows /shadow={}` manually",
                self.shadow_id, self.shadow_id
            );
        }
    }
}

/// F30: create the shadow copy `--vss-snapshot` asked for, if any. Windows only — on any other
/// platform (or when the flag isn't set) this is a no-op, matching how the rest of this binary
/// treats "real transfers need Windows" as informational rather than a hard error at parse time.
#[cfg(windows)]
async fn create_vss_snapshot(args: &Args) -> Result<Option<VssGuard>> {
    if !args.vss_snapshot {
        return Ok(None);
    }
    let source = args.source().to_path_buf();
    let volume = robocopy_ingest::vss::volume_of(&source)
        .context("cannot determine the source's volume for --vss-snapshot")?;

    let shadow = {
        let volume = volume.clone();
        spawn_blocking_with_span(move || robocopy_ingest::vss::create_shadow_copy(&volume))
            .await
            .context("the VSS snapshot task panicked")?
            .context(
                "cannot create a VSS shadow copy (are you running as Administrator?); \
                 aborting rather than silently reading the live volume",
            )?
    };

    let remapped_source = robocopy_ingest::vss::remap_to_shadow(&source, &volume, &shadow);
    println!(
        "VSS snapshot created: {} (volume {volume}) — reading from the snapshot instead of the live volume",
        shadow.shadow_id
    );
    tracing::info!(shadow_id = %shadow.shadow_id, volume = %volume, remapped = %remapped_source.display(), "VSS snapshot created");

    Ok(Some(VssGuard {
        shadow_id: shadow.shadow_id,
        remapped_source,
    }))
}

#[cfg(not(windows))]
async fn create_vss_snapshot(args: &Args) -> Result<Option<VssGuard>> {
    if args.vss_snapshot {
        tracing::warn!(
            "--vss-snapshot has no effect outside Windows; continuing without a snapshot"
        );
    }
    Ok(None)
}

struct RunOutcome {
    /// One of the `EXIT_*` constants; `0` means fully acceptable.
    exit_code: u8,
    summary: String,
    report_path: PathBuf,
}

async fn execute(args: &Args, child_pid: Arc<AtomicU32>) -> Result<RunOutcome> {
    let start_all = Instant::now();

    // F39: runs before anything else, including the VSS snapshot — a pre-command's job is
    // typically "stop the database so its files are consistent", which needs to happen before a
    // snapshot is even taken, not just before the copy. Blocking (waits for the child process),
    // so it runs in spawn_blocking like every other blocking operation in this file.
    if let Some(pre_command) = args.pre_command.clone() {
        spawn_blocking_with_span(move || robocopy_ingest::hooks::run_pre_command(&pre_command))
            .await
            .context("the pre-command task panicked")?
            .context("pre-command failed")?;
    }

    // F30: bound to `_vss_guard` (not `_`) so it lives for the rest of this function and its
    // shadow copy is deleted on every exit path, including Ctrl+C cancellation — see `VssGuard`'s
    // doc comment for why a plain `Drop` impl is what makes that work.
    let _vss_guard = create_vss_snapshot(args).await?;
    let effective_source = _vss_guard
        .as_ref()
        .map(|guard| guard.remapped_source.clone())
        .unwrap_or_else(|| args.source().to_path_buf());

    let start_inv = Instant::now();
    let inventory = inventory_source(args, &effective_source).await?;
    let inventory_seconds = start_inv.elapsed().as_secs_f64();

    if inventory.is_empty() {
        tracing::warn!(pattern = %args.pattern, "no file matches the pattern in the source tree");
        println!(
            "warning: no file matching {} found in {}",
            args.pattern,
            args.source().display()
        );
    }

    // F34: --backup-type diverts into a completely different pipeline (a naive, explicit-file-list
    // copy into a new generation subfolder, tracked in a manifest) rather than the plain-sync path
    // below — see `execute_generation_backup`'s doc comment for why the two can't share `transfer()`.
    // `validate()` already rejects --backup-type together with --mirror, so nothing past this
    // point ever needs to consider the two together.
    if let Some(backup_type) = args.backup_type {
        return execute_generation_backup(
            args,
            backup_type,
            &effective_source,
            &inventory,
            inventory_seconds,
            start_all,
        )
        .await;
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
    let (robocopy_outcome, copy_failure) =
        transfer(args, &effective_source, &inventory, child_pid).await?;
    let transfer_seconds = start_transfer.elapsed().as_secs_f64();

    let (baseline_outcome, baseline_seconds) = if args.compare_baseline && copy_failure.is_none() {
        let start_base = Instant::now();
        let outcome = baseline(args, &effective_source, &inventory).await?;
        (Some(outcome), Some(start_base.elapsed().as_secs_f64()))
    } else {
        if args.compare_baseline {
            tracing::warn!("skipping the baseline benchmark because the main transfer failed");
        }
        (None, None)
    };

    let (integrity_check, verification_seconds) = if args.verify_integrity
        && !args.dry_run
        && copy_failure.is_none()
    {
        if inventory.total_files_hint.is_some() {
            tracing::warn!(
                "skipping integrity verification: --no-prescan did not collect per-file paths"
            );
            println!("warning: --verify-integrity has no effect together with --no-prescan");
            (None, None)
        } else {
            let start_ver = Instant::now();
            let check = verify(args, &effective_source, &inventory).await?;
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
    let decrypted_count =
        if let (Some(key_spec), None, false) = (&args.decrypt, &copy_failure, args.dry_run) {
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

    // Must read before write_to below overwrites this exact path -- nothing else in this
    // function touches args.report_path before that point, so reading it here (rather than
    // right before write_to) is equally correct and lets the webhook payload below carry the
    // comparison too. Through spawn_blocking_with_span like every other blocking file read in
    // this file (e.g. IngestCache::load_from in verify() below) -- a bare synchronous read here
    // would block this tokio worker thread.
    let previous_report_path = args.report_path.clone();
    let previous_report = spawn_blocking_with_span(move || {
        robocopy_ingest::report::read_previous_report(&previous_report_path)
    })
    .await
    .context("the previous-report read task panicked")?;
    report.attach_previous_comparison(previous_report);

    if let Some(post_command) = args.post_command.clone() {
        let error = spawn_blocking_with_span(move || {
            robocopy_ingest::hooks::run_post_command(&post_command)
        })
        .await
        .context("the post-command task panicked")?;
        if let Some(error) = error {
            tracing::warn!(error = %error, "post-command failed");
            eprintln!("warning: {error}");
            report.post_command_error = Some(error);
        } else {
            tracing::info!("post-command completed successfully");
        }
    }

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

    // F29b: distinguish "the transfer itself failed" from "the transfer succeeded but
    // verification found a problem" — a scheduler needs to react differently to the two (retry
    // the whole job vs. flag a data-integrity incident).
    let exit_code = if copy_failure.is_some() {
        EXIT_INGESTION_PROBLEM
    } else if !integrity_ok {
        EXIT_INTEGRITY_FAILED
    } else {
        0
    };

    record_run_history(&report, exit_code, args).await;

    Ok(RunOutcome {
        exit_code,
        summary: report.human_summary(),
        report_path: args.report_path.clone(),
    })
}

/// Fase 0 of `VALUTAZIONE_AI.md` (closes the write half of ROADMAP F50): append one line
/// describing this run to `<dest>/.rustcopy_history.jsonl`, so `--advise` has a sample to reason
/// over instead of a directory of unrelated JSON files.
///
/// Called after `exit_code` has been computed, never before: the exit code is the single most
/// informative field in the record, and `main.rs` deliberately computes it in exactly one place
/// (AGENTS.md rule 12 makes it a contract with schedulers) rather than letting a second site
/// re-derive it.
///
/// **A failure here is never fatal.** The history is a statistics file; a backup that actually
/// moved and verified data has succeeded whether or not its run was indexed. Mirrors the
/// `webhook_error`/`post_command_error` non-fatal pattern (AGENTS.md rule 11) — logged, not
/// propagated. Through `spawn_blocking_with_span` like every other blocking file operation in this
/// file (D13).
async fn record_run_history(report: &IngestReport, exit_code: u8, args: &Args) {
    let report_path = args.report_path.clone();
    let record = robocopy_ingest::history::RunRecord::from_report(
        report,
        exit_code,
        Some(&args.report_path),
    )
    .with_job(args.job_name.as_deref())
    .with_backup_type(
        args.backup_type
            .map(|kind| format!("{kind:?}").to_lowercase()),
    );
    let job_name = args.job_name.clone();

    let result = spawn_blocking_with_span(move || {
        robocopy_ingest::history::RunHistory::append(&report_path, job_name.as_deref(), &record)
    })
    .await;

    match result {
        Ok(Ok(())) => tracing::debug!("run recorded in the history index"),
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "could not record this run in the history index")
        }
        Err(error) => {
            tracing::warn!(error = %error, "the history-index task panicked")
        }
    }
}

/// F34: run one backup generation (full, incremental, or differential) instead of the plain sync
/// above.
///
/// This deliberately does **not** reuse `transfer()` (robocopy): robocopy walks the source tree
/// itself, filtered only by `--pattern`/`--exclude-*`/age — it has no way to be told "copy
/// exactly these N specific relative paths and nothing else". Its own file-selection arguments
/// match filenames uniformly at every directory level, not a manifest of exact relative paths.
/// So a full generation can copy the whole (already-computed) `inventory` via the same
/// per-file naive copy engine used for `--compare-baseline`, and an incremental/differential
/// generation can copy just the changed subset the same way — see `engine::naive::copy_selected`.
/// Incremental diffs against the immediately preceding generation (`GenerationManifest::latest`);
/// differential always diffs against the last `Full` generation
/// (`GenerationManifest::latest_full`), so its size doesn't reset with every run in between.
///
/// Known scope limit of this first cut (F34): no baseline comparison, no `--verify-integrity`,
/// no encrypt/decrypt, no VSS remap of the *destination* side. These aren't fundamentally
/// incompatible with generations, just not wired up yet — `--backup-type` is opt-in (`None` by
/// default), so none of the existing single-destination flows lose anything by this existing
/// alongside them rather than folding into `execute()`'s main body.
async fn execute_generation_backup(
    args: &Args,
    backup_type: robocopy_ingest::generations::BackupType,
    effective_source: &Path,
    inventory: &ScanSummary,
    inventory_seconds: f64,
    start_all: Instant,
) -> Result<RunOutcome> {
    use robocopy_ingest::generations::{self, BackupType, GenerationManifest};

    let dest_root = args.dest().to_path_buf();
    let job_name = args.job_name.clone();

    // D20: the reference generation is read per backup type, not by loading the whole manifest up
    // front. `Full` needs no reference at all and now reads nothing (it used to pay for the entire
    // history — 580 MB at the scale measured in `probe_manifest_ram_at_real_world_scale` — and
    // then never look at it); the other two stream out the single generation they diff against,
    // so peak memory no longer grows with how many generations the destination has accumulated.
    // Do not reintroduce a `GenerationManifest::load_or_default` here.
    let reference = match backup_type {
        BackupType::Full => None,
        BackupType::Incremental | BackupType::Differential => {
            let dest_for_load = dest_root.clone();
            let job_name = job_name.clone();
            let found = spawn_blocking_with_span(move || match backup_type {
                BackupType::Differential => GenerationManifest::load_latest_full_generation(
                    &dest_for_load,
                    job_name.as_deref(),
                ),
                _ => {
                    GenerationManifest::load_latest_generation(&dest_for_load, job_name.as_deref())
                }
            })
            .await
            .context("the generation manifest load task panicked")?
            .context("cannot load the generation manifest")?;

            match (found, backup_type) {
                (Some(generation), _) => Some(generation),
                (None, BackupType::Differential) => anyhow::bail!(
                    "--backup-type differential found no prior full generation in {}; run --backup-type full first",
                    dest_root.display()
                ),
                (None, _) => anyhow::bail!(
                    "--backup-type incremental found no prior generation in {}; run --backup-type full first",
                    dest_root.display()
                ),
            }
        }
    };

    // D21: a full backup copies the inventory as-is, so it shares it rather than duplicating it.
    // Only the incremental/differential arms allocate, and correctly so -- they build a genuinely
    // different (filtered, smaller) list, not a duplicate.
    let files_to_copy: Arc<[ScannedFile]> = match &reference {
        None => Arc::clone(&inventory.files),
        Some(reference) => generations::changed_since(&inventory.files, &reference.files)
            .into_iter()
            .cloned()
            .collect(),
    };
    // The reference inventory is only needed for the diff above; drop it before the copy so its
    // memory (145 MB at real-world scale) isn't held for the whole transfer.
    drop(reference);

    let generation_id = generations::new_generation_id(backup_type);
    let effective_dest = dest_root.join(&generation_id);

    println!(
        "Backup type     : {} ({} of {} file(s) to copy)",
        backup_type.as_str(),
        files_to_copy.len(),
        inventory.file_count(),
    );

    if !args.dry_run {
        tokio::fs::create_dir_all(&effective_dest)
            .await
            .map_err(|error| IngestError::io(&effective_dest, error))
            .context("cannot create the generation destination directory")?;
    }

    let copied_bytes: u64 = files_to_copy.iter().map(|f| f.size_bytes).sum();
    let progress = new_progress(args, copied_bytes, "generation");
    // Kept for the report below (`IngestReport::with_timing` wants the file list, not just a
    // count) — the naive copy call needs its own owned copy to move into `spawn_blocking`.
    let copied_files = Arc::clone(&files_to_copy);

    let start_transfer = Instant::now();
    let (source_owned, dest_owned, dry_run, sink) = (
        effective_source.to_path_buf(),
        effective_dest.clone(),
        args.dry_run,
        Arc::clone(&progress),
    );
    // D15 (closes half of hypothesis #7): a copy failure here used to propagate fatally via `?`,
    // which `async_main()` maps to `EXIT_UNRECOVERABLE` (2) — unlike the plain-sync pipeline's
    // `transfer()`, whose failure is caught explicitly and surfaces as `EXIT_INGESTION_PROBLEM`
    // (1), the correct code for "the copy itself failed" rather than a usage/config error. It also
    // meant no report was ever written for a failed generation backup, unlike the plain-sync
    // pipeline which always writes one. This closes the exit-code half of the gap; a JSON report
    // is now written here too. It does *not* attempt to reconstruct per-file success/failure
    // counts (`engine::naive::copy_files` aborts its loop on the first failing file without
    // returning any partial `CopyOutcome` at all) — that would mean changing the naive engine
    // itself, deliberately out of scope for this fix (see the `AskUserQuestion` decision recorded
    // in `ANALYSIS.md` D15).
    let (copy_outcome, copy_error) = match spawn_blocking_with_span(move || {
        robocopy_ingest::engine::naive::copy_selected(
            &source_owned,
            &dest_owned,
            &files_to_copy,
            dry_run,
            sink.as_ref(),
        )
    })
    .await
    .context("the generation copy task panicked")?
    {
        Ok(outcome) => (outcome, None),
        Err(error) => {
            tracing::error!("generation copy failed: {error}");
            eprintln!("generation copy failed: {error}");
            (CopyOutcome::new("naive"), Some(error))
        }
    };
    let transfer_seconds = start_transfer.elapsed().as_secs_f64();

    if !args.dry_run && copy_error.is_none() {
        let new_generation = generations::Generation {
            id: generation_id.clone(),
            backup_type,
            created_at: chrono::Utc::now().to_rfc3339(),
            files_copied: copy_outcome.files_copied as usize,
            files: generations::to_generation_files(&inventory.files),
        };
        let dest_root_for_save = dest_root.clone();
        let job_name_for_save = job_name.clone();
        // D-NEXT: appends the one new generation as an NDJSON line instead of rewriting the whole
        // manifest (`GenerationManifest::save`) -- see that function's doc comment for why this
        // matters at scale. `manifest` (loaded above, used only to find the incremental/
        // differential reference generation) is intentionally not reused here.
        spawn_blocking_with_span(move || {
            GenerationManifest::append_generation(
                &dest_root_for_save,
                job_name_for_save.as_deref(),
                &new_generation,
            )
        })
        .await
        .context("the generation manifest append task panicked")?
        .context("cannot append to the generation manifest")?;

        if let Some(keep_cycles) = args.keep_generations {
            prune_old_generations(args, &dest_root, keep_cycles).await?;
        }
    }

    let timing = robocopy_ingest::report::PhaseTiming {
        inventory_seconds,
        transfer_seconds,
        verification_seconds: None,
        baseline_seconds: None,
        total_seconds: start_all.elapsed().as_secs_f64(),
    };

    // The report documents where *this generation's* files actually landed, not the destination
    // root — hence a clone of `args` with `dest` pointed at the generation subfolder, purely for
    // `IngestReport::with_timing`'s benefit (nothing else uses `gen_args`). `total_files`/
    // `total_bytes` in the report reflect what this generation actually copied (`copied_files`),
    // not the full source inventory — for an incremental generation those are two very different
    // numbers, and the report should describe the delta that was actually written to disk.
    let mut gen_args = args.clone();
    gen_args.dest = Some(effective_dest.clone());
    let scoped_inventory = ScanSummary {
        files: copied_files,
        total_bytes: copied_bytes,
        total_files_hint: None,
    };

    let mut report = IngestReport::with_timing(
        &gen_args,
        &scoped_inventory,
        &copy_outcome,
        None,
        None,
        timing,
    );
    report.copy_error = copy_error.as_ref().map(|error| error.to_string());

    let previous_report_path = args.report_path.clone();
    let previous_report = spawn_blocking_with_span(move || {
        robocopy_ingest::report::read_previous_report(&previous_report_path)
    })
    .await
    .context("the previous-report read task panicked")?;
    report.attach_previous_comparison(previous_report);

    if let Some(post_command) = args.post_command.clone() {
        let error = spawn_blocking_with_span(move || {
            robocopy_ingest::hooks::run_post_command(&post_command)
        })
        .await
        .context("the post-command task panicked")?;
        if let Some(error) = error {
            tracing::warn!(error = %error, "post-command failed");
            eprintln!("warning: {error}");
            report.post_command_error = Some(error);
        } else {
            tracing::info!("post-command completed successfully");
        }
    }

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

    let exit_code = if copy_error.is_some() {
        EXIT_INGESTION_PROBLEM
    } else {
        0
    };
    record_run_history(&report, exit_code, args).await;

    Ok(RunOutcome {
        exit_code,
        summary: report.human_summary(),
        report_path: args.report_path.clone(),
    })
}

/// F35: delete old backup generations beyond the `keep_cycles` most recent ones. A "cycle" is one
/// `full` generation plus every `incremental`/`differential` generation that follows it — see
/// `GenerationManifest::cycles`'s doc comment for why rotation happens at that granularity rather
/// than per raw generation (deleting a `full` still depended on by a kept `incremental`/
/// `differential` would orphan it).
///
/// Reuses `--force-purge`/the interactive-confirmation gate from `check_mirror_safety`: both are
/// "about to delete data at --dest, get explicit go-ahead first" situations, so the same flag and
/// the same non-interactive-defaults-to-abort behaviour apply here.
///
/// Runs inside `tokio::task::spawn_blocking` for the actual filesystem deletions, same discipline
/// as every other blocking filesystem operation in this file.
async fn prune_old_generations(args: &Args, dest_root: &Path, keep_cycles: usize) -> Result<()> {
    use robocopy_ingest::generations::{GenerationIndex, GenerationManifest};

    let job_name = args.job_name.clone();
    // D20: deciding *what* to prune only reads each generation's id and type, never its `files`
    // inventory (`cycle_ranges` is driven purely by backup type) — so this loads the metadata-only
    // index rather than the whole history, which at real-world scale is the difference between
    // ~0 MB and 580 MB. The full manifest is still loaded further down, but only on the path that
    // genuinely rewrites it. Do not "simplify" this back to one `load_or_default` for both.
    let index = {
        let dest_root = dest_root.to_path_buf();
        let job_name = job_name.clone();
        spawn_blocking_with_span(move || GenerationIndex::load(&dest_root, job_name.as_deref()))
            .await
            .context("the generation index load task panicked")?
            .context("cannot load the generation manifest for retention")?
    };

    let mut prune_ids = index.generations_to_prune(keep_cycles);
    if prune_ids.is_empty() {
        return Ok(());
    }
    prune_ids.sort();

    if !args.force_purge {
        let mut confirmed = false;
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            eprintln!(
                "\n--keep-generations {keep_cycles} would delete {} old generation folder(s) from {}: {}.",
                prune_ids.len(),
                dest_root.display(),
                prune_ids.join(", ")
            );
            eprint!("Proceed with the purge? [y/N] ");
            use std::io::Write;
            std::io::stderr().flush().ok();
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer).ok();
            confirmed = answer.trim().eq_ignore_ascii_case("y");
        }
        if !confirmed {
            return Err(IngestError::RetentionPurgeAborted {
                count: prune_ids.len(),
            }
            .into());
        }
    }

    let dest_root_owned = dest_root.to_path_buf();
    let job_name_owned = job_name.clone();
    spawn_blocking_with_span(move || -> Result<(), IngestError> {
        let ids: HashSet<String> = prune_ids.into_iter().collect();
        for id in &ids {
            let folder = dest_root_owned.join(id);
            if folder.exists() {
                std::fs::remove_dir_all(&folder)
                    .map_err(|error| IngestError::io(&folder, error))?;
            }
        }
        let mut manifest =
            GenerationManifest::load_or_default(&dest_root_owned, job_name_owned.as_deref())?;
        manifest.retain_generations(&ids);
        manifest.save(&dest_root_owned, job_name_owned.as_deref())
    })
    .await
    .context("the retention prune task panicked")?
    .context("cannot prune old generations")?;

    Ok(())
}

/// Build the source inventory off the async runtime: walking a 50 GB tree is blocking work.
///
/// F2.4/F2.6 fix: `--no-prescan` used to be accepted but ignored (this function always did the
/// full walk regardless). It now actually switches to the lightweight `scan::inventory` walk,
/// which counts files/bytes without materialising every path in RAM — the whole point of the
/// flag on multi-million file trees.
async fn inventory_source(args: &Args, effective_source: &Path) -> Result<ScanSummary> {
    let source = effective_source.to_path_buf();
    let pattern = args.pattern.clone();
    let no_prescan = args.no_prescan;
    // F26d: follow junctions/symlinked directories exactly when robocopy itself will (i.e.
    // whenever /XJ is not passed), so the prescan and the actual transfer walk the same tree.
    let follow_links = !args.exclude_junctions;
    let exclude_dirs = args.exclude_dirs.clone();
    let exclude_files = args.exclude_files.clone();
    // D17: min_age_days/max_age_days must be threaded through the same way exclude_dirs/
    // exclude_files are (D11), or --verify-integrity spuriously reports age-excluded files as
    // missing_in_dest and --backup-type ignores the flags entirely (it never goes through
    // robocopy, see AGENTS.md rule 9).
    let min_age_days = args.min_age_days;
    let max_age_days = args.max_age_days;

    let inventory = if no_prescan {
        spawn_blocking_with_span(move || {
            scan::inventory(
                &source,
                &pattern,
                follow_links,
                &exclude_dirs,
                &exclude_files,
                min_age_days,
                max_age_days,
            )
        })
        .await
        .context("the source scan task panicked")?
        .context("cannot scan the source directory")?
        .into_scan_summary()
    } else {
        spawn_blocking_with_span(move || {
            scan::scan(
                &source,
                &pattern,
                follow_links,
                &exclude_dirs,
                &exclude_files,
                min_age_days,
                max_age_days,
            )
        })
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
        return Err(IngestError::MirrorPurgeAborted { count: usize::MAX }.into());
    }

    let source_relative: HashSet<PathBuf> = inventory
        .files
        .iter()
        .map(|f| normalize_for_compare(&f.relative_path))
        .collect();

    let dest = args.dest().to_path_buf();
    // F26d: follow junctions in the destination scan exactly when the source scan did, so the
    // mirror-purge diff isn't computed against a differently-shaped tree. Same reasoning for
    // exclude_dirs/exclude_files: robocopy's /XD /XF leave matching destination-side entries
    // alone during /MIR (neither copied nor purged), so this diff must exclude them too, or it
    // would flag as "extraneous" (and prompt to delete) destination content robocopy itself
    // would never touch. D17: min_age_days/max_age_days need the identical treatment — verified
    // empirically against the real binary (`/MIR /MAXAGE:N` does not purge a destination file
    // that fails the age filter, matching how /XD /XF already behave here), see CLAUDE.md.
    let follow_links = !args.exclude_junctions;
    let exclude_dirs = args.exclude_dirs.clone();
    let exclude_files = args.exclude_files.clone();
    let min_age_days = args.min_age_days;
    let max_age_days = args.max_age_days;
    let dest_all = spawn_blocking_with_span(move || {
        scan::scan(
            &dest,
            "*",
            follow_links,
            &exclude_dirs,
            &exclude_files,
            min_age_days,
            max_age_days,
        )
    })
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
async fn encrypt_destination(
    args: &Args,
    inventory: &ScanSummary,
    key_spec: &str,
) -> Result<usize> {
    let key = robocopy_ingest::crypto::resolve_key(key_spec)?;
    let manager = CryptoManager::new(&key)?;
    let dest_root = args.dest().to_path_buf();
    let files = Arc::clone(&inventory.files);

    let count = spawn_blocking_with_span(move || -> Result<usize, IngestError> {
        let mut encrypted = 0usize;
        for file in files.iter() {
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
async fn decrypt_destination(
    args: &Args,
    inventory: &ScanSummary,
    key_spec: &str,
) -> Result<usize> {
    let key = robocopy_ingest::crypto::resolve_key(key_spec)?;
    let manager = CryptoManager::new(&key)?;
    let dest_root = args.dest().to_path_buf();
    let files = Arc::clone(&inventory.files);

    let count = spawn_blocking_with_span(move || -> Result<usize, IngestError> {
        let mut decrypted = 0usize;
        for file in files.iter() {
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
    effective_source: &Path,
    inventory: &ScanSummary,
    child_pid: Arc<AtomicU32>,
) -> Result<(CopyOutcome, Option<IngestError>)> {
    let progress = new_progress(args, inventory.total_bytes, "robocopy");
    let poller = spawn_dest_poller(args, Arc::clone(&progress));

    let mut request = args.copy_request(args.dest().to_path_buf());
    // F30: read from the VSS shadow copy instead of the live volume when --vss-snapshot created
    // one; effective_source equals args.source() unchanged otherwise.
    request.source = effective_source.to_path_buf();
    let policy = args.retry_policy();
    let sink = Arc::clone(&progress);
    let started = Instant::now();

    let result = spawn_blocking_with_span(move || {
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
            let exclude_dirs = args.exclude_dirs.clone();
            let exclude_files = args.exclude_files.clone();
            // D17: min_age_days/max_age_days deliberately NOT passed here — this counts what is
            // actually present at the destination right now for the report, not a source-side
            // selection decision, so a file's age at scan time has no bearing on it.
            let observed = spawn_blocking_with_span(move || {
                scan::inventory(
                    &dest_for_count,
                    "*",
                    follow_links,
                    &exclude_dirs,
                    &exclude_files,
                    None,
                    None,
                )
            })
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
async fn baseline(
    args: &Args,
    effective_source: &Path,
    inventory: &ScanSummary,
) -> Result<CopyOutcome> {
    let temp = baseline_dir(args.dest())?;
    let dest = temp.path().to_path_buf();
    tracing::info!(path = %dest.display(), "starting naive baseline copy");
    println!("\nRunning the naive baseline copy (single threaded, file by file)...");

    let progress = new_progress(args, inventory.total_bytes, "baseline");
    let mut request = args.copy_request(dest);
    request.source = effective_source.to_path_buf(); // F30: see transfer()
    let sink = Arc::clone(&progress);

    let outcome =
        spawn_blocking_with_span(move || NaiveCopyEngine::new().copy(&request, sink.as_ref()))
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

/// F28: forward-slash relative path, matching exactly how `integrity::verify` labels
/// `mismatches`/`missing_in_dest`/`unreadable` entries — the two must agree for the "don't cache
/// a file that just failed" check in [`verify`] to actually match failures against cache keys.
fn fast_verify_cache_key(file: &ScannedFile) -> String {
    file.relative_path.to_string_lossy().replace('\\', "/")
}

async fn verify(
    args: &Args,
    effective_source: &Path,
    inventory: &ScanSummary,
) -> Result<IntegrityCheck> {
    println!("\nVerifying integrity with {:?}...", args.hash_algo);

    let all_files = Arc::clone(&inventory.files);
    let cache_path = cache::default_cache_path(args.dest(), args.job_name.as_deref());
    let fast_verify = args.fast_verify;

    // F28 (closes D2's --fast-verify half): trust a file as unchanged (and skip re-hashing it)
    // only when its size+mtime still match the `.ingest_cache` entry left by the last run that
    // actually verified it clean. This is the same trust model robocopy's own /A and rsync use —
    // not a substitute for a cryptographic re-check, but real on an incremental run where most
    // files are untouched.
    // D21: both arms produce an `Arc<[ScannedFile]>`. The `--fast-verify` arm genuinely builds a
    // new (smaller, filtered) list and pays one copy for it; the default arm shares the inventory
    // outright instead of duplicating it, which is where the 435 MB measured by
    // `probe_scan_inventory_ram_at_real_world_scale` went.
    let (files_to_check, cache): (Arc<[ScannedFile]>, IngestCache) = if fast_verify {
        let candidates = Arc::clone(&all_files);
        let load_path = cache_path.clone();
        spawn_blocking_with_span(move || {
            let cache = IngestCache::load_from(&load_path);
            let changed: Arc<[ScannedFile]> = candidates
                .iter()
                .filter(|f| {
                    !cache.should_skip(
                        &fast_verify_cache_key(f),
                        f.size_bytes,
                        f.modified_timestamp,
                    )
                })
                .cloned()
                .collect();
            (changed, cache)
        })
        .await
        .context("the fast-verify cache lookup task panicked")?
    } else {
        (Arc::clone(&all_files), IngestCache::default())
    };

    let skipped_unchanged = all_files.len() - files_to_check.len();
    if fast_verify {
        println!(
            "Fast-verify: {skipped_unchanged} of {} file(s) skipped as unchanged since the last \
             verified run (cache: {})",
            all_files.len(),
            cache_path.display()
        );
    }

    // The progress bar's denominator must match what's actually going to be hashed — with
    // --fast-verify skipping most of the tree, sizing it off inventory.total_bytes would leave it
    // stuck well short of 100%.
    let bytes_to_verify: u64 = files_to_check.iter().map(|f| f.size_bytes).sum();
    let progress = new_progress(args, bytes_to_verify, "verify");

    let source = effective_source.to_path_buf();
    let dest = args.dest().to_path_buf();
    let algo = args.hash_algo;
    let sink = Arc::clone(&progress);
    let files_for_verify = Arc::clone(&files_to_check);

    let mut check = spawn_blocking_with_span(move || {
        integrity::verify(&source, &dest, &files_for_verify, algo, sink.as_ref())
    })
    .await
    .context("the integrity task panicked")?;
    check.skipped_unchanged = skipped_unchanged;

    if fast_verify {
        // Only remember files that were freshly re-checked *and* passed this run: never a file
        // that just failed (it must keep being re-checked every run until it's actually fixed,
        // not silently start being trusted), and never a file we skipped without looking at it
        // (its existing cache entry is already correct — touching it would be redundant I/O, and
        // computed from the failure set captured *before* --ignore-transient-missing runs below,
        // so a transient-missing file that gets forgiven for reporting purposes still isn't
        // wrongly cached as "confirmed present and matching").
        let failed: HashSet<String> = check
            .mismatches
            .iter()
            .map(|m| m.path.clone())
            .chain(check.missing_in_dest.iter().cloned())
            .chain(check.unreadable.iter().cloned())
            .collect();
        let mut cache = cache;
        for file in files_to_check.iter() {
            let key = fast_verify_cache_key(file);
            if !failed.contains(&key) {
                cache.update(key, file.size_bytes, file.modified_timestamp, None);
            }
        }
        let save_path = cache_path.clone();
        let save_result = spawn_blocking_with_span(move || cache.save_to(&save_path))
            .await
            .context("the fast-verify cache save task panicked")?;
        if let Err(error) = save_result {
            tracing::warn!(error = %error, path = %cache_path.display(), "could not persist the fast-verify cache");
        }
    }

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
            match spawn_blocking_with_span(move || scan::directory_size(&sampled, follow_links))
                .await
            {
                Ok(size) => progress.observe_total_bytes(size.saturating_sub(already_present)),
                Err(_) => break,
            }
        }
    }))
}
