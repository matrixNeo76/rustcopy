//! Command line interface definition.

use std::path::{Path, PathBuf};

use clap::Parser;

use crate::engine::{CopyRequest, RetryPolicy};
use crate::errors::IngestError;
use crate::logging;

pub const MIN_THREADS: u8 = 1;
pub const MAX_THREADS: u16 = 128;

/// F27: verbosity level for `--log-level`, mapped to a per-crate `tracing` filter. D18: default
/// is `Info`, not `Debug` — `Debug` emits one line per file copied (`robocopy transferred file`),
/// which produced a 356 MB log on a real 1.34M-file run (`_ops_reports/full-profile-test.log`,
/// see `logging::DEFAULT_FILTER`'s doc comment). Still available via `--log-level debug`.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

/// Number of copy threads to use when --threads is not specified.
fn default_threads() -> u16 {
    // num_cpus::get() returns logical CPUs, which is a sensible starting point.
    // Clamped to [1, 128] to honour robocopy's /MT constraints.
    (num_cpus::get() as u16).clamp(MIN_THREADS as u16, MAX_THREADS)
}

/// Ingest large CSV datasets with Robocopy and benchmark it against a naive copy.
#[derive(Parser, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    name = "robocopy_ingest",
    version,
    about = "Robocopy-based CSV ingestion with throughput reporting and baseline benchmarking",
    long_about = "Wraps Windows robocopy.exe to ingest large (50GB-scale) CSV datasets.\n\
                  Provides throughput-based progress, an outer retry loop on top of robocopy's \
                  own /R and /W retries, optional SHA-256 integrity verification and an optional \
                  baseline comparison against a naive recursive copy (the equivalent of \
                  `Get-ChildItem | Copy-Item`). Results are written as a JSON report.\n\n\
                  Note: real transfers require Windows because robocopy.exe is a Windows tool. \
                  The baseline engine and all report/verification logic are cross-platform."
)]
pub struct Args {
    /// Path to a TOML configuration file containing default argument settings.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Source directory containing the CSV files to ingest.
    /// Not required when --restore-from, --resume-from or --config is given (derived from the
    /// backup report / interruption checkpoint / config file respectively).
    ///
    /// F24 fix: this used to be a plain `PathBuf` with `default_value = ""` alongside
    /// `required_unless_present`, on the assumption that an empty-string default would let clap
    /// skip the arg when --restore-from was present. It didn't: clap treats an empty-string
    /// default as no default at all, so the arg stayed unconditionally required and
    /// `--restore-from` was unreachable from the CLI no matter what was passed (reproduced with a
    /// minimal clap repro outside this crate, and confirmed native PowerShell, not just this
    /// crate's own test harness, hit the same error). `Option<PathBuf>` sidesteps the whole
    /// default-value question: clap simply leaves it `None` when omitted, exactly like it already
    /// does for `--config`/`--restore-from` above.
    ///
    /// F33 fix: `--config` was missing from this list entirely, so even the pre-existing
    /// single-job config-file mode could never be invoked with `--config` alone — clap demanded
    /// `--source`/`--dest` as dummy CLI args regardless, defeating the point of a config file.
    /// `--config`'s multi-job mode (`[[jobs]]`) never touches `Args::source()`/`dest()` at all
    /// (each job builds its own `Args`), so this only had to be caught once job mode needed it.
    #[arg(
        long,
        value_name = "PATH",
        required_unless_present_any = [
            "restore_from",
            "resume_from",
            "config",
            "uninstall_schedule",
            "install_service",
            "uninstall_service",
            "advise",
            "set_credential",
            "delete_credential",
            "list_schedules"
        ]
    )]
    pub source: Option<PathBuf>,

    /// Destination directory for the ingested files.
    /// Not required when --restore-from, --resume-from or --config is given.
    #[arg(
        long,
        value_name = "PATH",
        required_unless_present_any = [
            "restore_from",
            "resume_from",
            "config",
            "uninstall_schedule",
            "install_service",
            "uninstall_service",
            "advise",
            "set_credential",
            "delete_credential",
            "list_schedules"
        ]
    )]
    pub dest: Option<PathBuf>,

    /// File pattern to match (robocopy file filter / glob on the file name). Defaults to "*" (all files).
    #[arg(long, default_value = "*", value_name = "GLOB")]
    pub pattern: String,

    /// Number of copy threads, mapped to robocopy's /MT:N (1-128). Defaults to logical CPU count.
    #[arg(long, default_value_t = default_threads(), value_name = "N")]
    pub threads: u16,

    /// Retries per failed file, mapped to robocopy's /R:N and used as the outer retry budget.
    #[arg(long, default_value_t = 3, value_name = "N")]
    pub retries: u32,

    /// Seconds to wait between retries, mapped to robocopy's /W:N and the outer backoff base.
    #[arg(long, default_value_t = 5, value_name = "N")]
    pub retry_wait_seconds: u64,

    /// After the transfer, compare checksums of source and destination files.
    #[arg(long, default_value_t = false)]
    pub verify_integrity: bool,

    /// Selective integrity verification: skip re-hashing files whose size and modification time
    /// still match the last verified-clean run, recorded in `<dest>/.ingest_cache`. Has no effect
    /// without --verify-integrity. Trust model note: this trusts the SOURCE file's identity
    /// (size+mtime), not a re-check of the destination's actual bytes — if a destination file were
    /// corrupted independently (e.g. silent disk bit rot) while its source counterpart stays
    /// byte-for-byte unchanged, --fast-verify won't catch it on a run where that file gets skipped.
    /// A file that fails verification is never cached as trusted, so it keeps being re-checked on
    /// every subsequent run until it actually passes.
    #[arg(long, default_value_t = false)]
    pub fast_verify: bool,

    /// After --verify-integrity, treat missing/unreadable files matching well-known transient
    /// patterns (.log, .tmp, anything under .git/objects/) as expected rather than a verification
    /// failure — these can legitimately disappear between the copy and the verification pass (log
    /// rotation, temp-file cleanup, git garbage collection). Has no effect without
    /// --verify-integrity.
    #[arg(long, default_value_t = false)]
    pub ignore_transient_missing: bool,

    /// Hash algorithm for integrity checks: sha256 (default), blake3 (3-5x faster), or xxh3
    /// (~5-10x faster than blake3). xxh3 is NOT cryptographic — fine for detecting accidental
    /// corruption, not for a backup where an attacker could have tampered with the data.
    #[arg(long, default_value = "sha256", value_name = "ALGO")]
    pub hash_algo: crate::integrity::HashAlgorithm,

    /// Also run a naive recursive copy into a temporary destination and time it for comparison.
    #[arg(long, default_value_t = false)]
    pub compare_baseline: bool,

    /// Path of the final JSON report. Supports the placeholder {timestamp}, replaced with this
    /// run's start time as yyyyMMdd_HHmmss (e.g. report-{timestamp}.json), so a scheduled job
    /// keeps its report history instead of overwriting the same file every run. Without the
    /// placeholder the path is fixed and overwritten every run, exactly as before.
    #[arg(
        long,
        default_value = "./robocopy_ingest_report.json",
        value_name = "PATH"
    )]
    pub report_path: PathBuf,

    /// Path of the asynchronous log file.
    #[arg(long, default_value = "./robocopy_ingest.log", value_name = "PATH")]
    pub log_path: PathBuf,

    // ── F27: log verbosity, quiet mode and rotation ─────────────────────────
    /// Verbosity written to --log-path (per-crate; dependencies always log at WARN). Ignored if
    /// the RUST_LOG environment variable is set. debug writes one line per file, which on
    /// multi-million-file trees means gigabytes per run (D18) — use it when actually diagnosing a
    /// specific run, not as a standing default.
    #[arg(long, value_enum, default_value_t = LogLevel::Info, conflicts_with = "quiet")]
    pub log_level: LogLevel,

    /// Shorthand for --log-level warn: suppresses the per-file DEBUG lines responsible for most of
    /// the log volume on large trees, keeping only warnings/errors.
    #[arg(long, default_value_t = false, conflicts_with = "log_level")]
    pub quiet: bool,

    /// Rotate the log aside (--log-path.1, .2, ...) once it reaches this many bytes — both at
    /// startup (the previous run's log) and mid-run (D18: this run's own log, if it crosses the
    /// threshold while still writing), instead of letting a single long run or repeated runs
    /// against the same --log-path grow it without bound. 0 disables rotation.
    #[arg(long, default_value_t = logging::DEFAULT_MAX_LOG_BYTES, value_name = "BYTES")]
    pub log_max_bytes: u64,

    /// Number of rotated log backups to keep (oldest dropped first). Has no effect if
    /// --log-max-bytes is 0.
    #[arg(long, default_value_t = logging::DEFAULT_MAX_LOG_BACKUPS, value_name = "N")]
    pub log_max_backups: u32,

    /// Show what would happen without copying anything (robocopy /L).
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    // ── F34: backup generations ─────────────────────────────────────────────
    /// Run as a versioned backup generation instead of a plain sync into --dest: writes into a
    /// new `<dest>/<timestamp>_<type>/` subfolder and records it in
    /// `<dest>/.rustcopy_generations.json` for future runs to diff against. `full` copies
    /// everything; `incremental` copies only files new or changed since the immediately
    /// preceding generation (of either type) and requires at least one prior generation to
    /// exist; `differential` copies only files new or changed since the last `full` generation
    /// (not the last generation of any type) and requires at least one prior full generation to
    /// exist. Omitted (the default) keeps the pre-F34 behaviour: a plain sync directly into
    /// --dest, no generation folder, no manifest. Not compatible with --mirror (mirror deletes
    /// destination-only files, which makes no sense once --dest holds a manifest and multiple
    /// generation subfolders rather than a 1:1 mirror of the source).
    #[arg(long, value_enum, value_name = "TYPE")]
    pub backup_type: Option<crate::generations::BackupType>,

    /// F35: retention/rotation for backup generations. Keeps the N most recent *cycles* — a
    /// cycle is one `full` generation plus every `incremental`/`differential` generation that
    /// follows it, up to the next `full` — and deletes the entire folder (and manifest entry) of
    /// every generation in an older cycle. Rotating by cycle rather than by raw generation count
    /// avoids ever deleting a `full` that a still-kept `incremental`/`differential` depends on
    /// for restoration. Requires `--backup-type` (there is nothing to rotate without a generation
    /// history) and, like `--mirror`'s purge, requires `--force-purge` or an interactive
    /// confirmation before actually deleting anything.
    #[arg(long, value_name = "N")]
    pub keep_generations: Option<usize>,

    // ── F4.3: Mirror mode ───────────────────────────────────────────────────
    /// Mirror source to destination: delete files in the destination that are not in the source.
    /// Maps to robocopy /MIR.  CAUTION: files present only in dest will be DELETED.
    #[arg(long, default_value_t = false)]
    pub mirror: bool,

    /// Force purge without safety confirmation: skips the interactive prompt for --mirror's
    /// destination-file purge and, separately, for --keep-generations' old-generation purge.
    #[arg(long, default_value_t = false)]
    pub force_purge: bool,

    /// F63: write the full, untruncated list of files --mirror would delete to PATH as JSON, then
    /// exit without copying or deleting anything. Requires --mirror (there is nothing to preview
    /// otherwise). Unlike the interactive confirmation this never asks and never purges — a
    /// preview is a read, not an authorization, and runs regardless of --force-purge.
    #[arg(long, value_name = "PATH", requires = "mirror")]
    pub purge_preview_path: Option<PathBuf>,

    // ── F4.1: Exclusion filters ─────────────────────────────────────────────
    /// Exclude files matching the given pattern(s) (repeatable, maps to /XF).
    /// Example: --exclude-files "*.tmp" --exclude-files "thumbs.db"
    #[arg(long, value_name = "GLOB", action = clap::ArgAction::Append)]
    pub exclude_files: Vec<String>,

    /// Exclude directories matching the given pattern(s) (repeatable, maps to /XD).
    /// Example: --exclude-dirs ".git" --exclude-dirs "node_modules"
    #[arg(long, value_name = "GLOB", action = clap::ArgAction::Append)]
    pub exclude_dirs: Vec<String>,

    // ── F4.2: Date-based filters ────────────────────────────────────────────
    /// Skip files modified less than N days ago — keep only files at least N days old (maps to
    /// robocopy /MINAGE:N). D17: this help text had the direction backwards until it was
    /// verified against the real robocopy.exe binary; see CLAUDE.md.
    #[arg(long, value_name = "DAYS")]
    pub min_age_days: Option<u32>,

    /// Skip files modified more than N days ago — keep only files within the last N days (maps
    /// to robocopy /MAXAGE:N). D17: see the note on --min-age-days above.
    #[arg(long, value_name = "DAYS")]
    pub max_age_days: Option<u32>,

    // ── F4.5: Bandwidth throttling ──────────────────────────────────────────
    /// Limit transfer bandwidth to approximately this many MB/s.
    /// Converted to robocopy's /IPG (inter-packet gap in ms).
    /// Example: --bandwidth-limit-mbps 100 to cap at ~100 MB/s.
    #[arg(long, value_name = "MB_PER_SEC")]
    pub bandwidth_limit_mbps: Option<u32>,

    // ── F2.6: Pre-scan control ──────────────────────────────────────────────
    /// Skip the upfront source tree walk that counts files and bytes before copying starts.
    /// Without a prescan the progress bar has no total and integrity check is unavailable.
    /// Useful for very large trees (millions of files) where the walk itself takes minutes.
    #[arg(long, default_value_t = false)]
    pub no_prescan: bool,

    // ── F6.1: Windows Long Path support ─────────────────────────────────────
    /// Prepend Windows long path prefix `\\?\` for deep path structures (> 260 chars).
    #[arg(long, default_value_t = false)]
    pub long_paths: bool,

    // ── F6.2: Metadata & ACL preservation ────────────────────────────────────
    /// Preserve directory timestamps (maps to robocopy /DCOPY:DAT).
    #[arg(long, default_value_t = false)]
    pub preserve_timestamps: bool,

    /// Preserve NTFS ACL security permissions (maps to robocopy /COPYALL).
    #[arg(long, default_value_t = false)]
    pub preserve_acl: bool,

    // ── F30: VSS Snapshot ─────────────────────────────────────────────────────
    /// Create a Volume Shadow Copy of the source volume before scanning/copying, so files locked
    /// by other processes can still be read, instead of permanently failing and exhausting the
    /// retry budget. Requires running as Administrator; fails clearly (no silent fallback to
    /// reading the live volume) if the snapshot cannot be created. Windows only — ignored (with a
    /// warning) elsewhere. The shadow copy is crash-consistent only: there is no VSS writer
    /// coordination with an application such as a live database.
    #[arg(long, default_value_t = false)]
    pub vss_snapshot: bool,

    // ── F26d: junction/symlink consistency ───────────────────────────────────
    /// Exclude junction points and symlinked directories from the copy (maps to robocopy /XJ).
    /// Without this, robocopy follows them (its own default), which can duplicate data or
    /// recurse into a self-referencing junction; the source inventory used for the progress
    /// total and --verify-integrity follows the same rule, so what gets counted and what gets
    /// copied always agree.
    #[arg(long, default_value_t = false)]
    pub exclude_junctions: bool,

    // ── F7.1: Webhook completion notifications ──────────────────────────────
    /// Send an HTTP POST JSON execution summary to this Webhook URL upon completion.
    #[arg(long, value_name = "URL")]
    pub webhook_url: Option<String>,

    // ── F39: pre/post job commands ──────────────────────────────────────────
    /// Shell command to run before the job starts (e.g. stop a database service so its files are
    /// consistent when backed up). Runs via `cmd /C` on Windows, `sh -c` elsewhere. If it exits
    /// non-zero (or can't be spawned), the job aborts without copying anything — proceeding after
    /// a failed pre-command (e.g. a database that didn't actually stop) would silently back up
    /// inconsistent data.
    #[arg(long, value_name = "CMD")]
    pub pre_command: Option<String>,

    /// Shell command to run after the job finishes (e.g. restart the service stopped by
    /// --pre-command). Runs via `cmd /C` on Windows, `sh -c` elsewhere. Unlike --pre-command, a
    /// failure here does NOT fail the job — the backup already succeeded by this point — it is
    /// only logged and recorded in the report's `post_command_error` field.
    #[arg(long, value_name = "CMD")]
    pub post_command: Option<String>,

    // ── F36: schtasks.exe-backed scheduling ──────────────────────────────────
    /// Install this exact invocation (minus the scheduling flags themselves) as a recurring
    /// Windows Task Scheduler entry via `schtasks.exe`, then exit without running a backup now.
    /// SPEC is one of: `daily@HH:MM`, `hourly@N`, or `weekly@DAY,...@HH:MM` (DAY is a 3-letter
    /// weekday code, e.g. `MON`). No internal scheduler process is involved — Windows itself wakes
    /// this binary up on the given trigger, same as if the operator had run `schtasks.exe`
    /// directly. Re-running with the same --schedule-name updates the existing task in place.
    #[arg(
        long,
        value_name = "SPEC",
        conflicts_with_all = ["uninstall_schedule", "restore_from", "resume_from"]
    )]
    pub install_schedule: Option<String>,

    /// Name of the Task Scheduler entry to create (with --install-schedule) or delete (with
    /// --uninstall-schedule). Defaults to `rustcopy` when omitted with --install-schedule.
    #[arg(long, value_name = "NAME")]
    pub schedule_name: Option<String>,

    /// Remove a previously installed schedule by name via `schtasks.exe /Delete`, then exit
    /// without running a backup. Unlike --install-schedule, does not require --source/--dest.
    #[arg(
        long,
        value_name = "NAME",
        conflicts_with_all = ["install_schedule", "restore_from", "resume_from"]
    )]
    pub uninstall_schedule: Option<String>,

    // F8.1: Disaster Recovery & Restore Mode ──────────────────────────────
    /// Path to a previous backup report JSON file to initiate reverse restore mode.
    #[arg(long, value_name = "REPORT_PATH", conflicts_with = "resume_from")]
    pub restore_from: Option<PathBuf>,

    // ── F31: Interrupted-run checkpoint & resume ─────────────────────────────
    /// Path to a checkpoint file written when a previous run was interrupted (Ctrl+C), to
    /// continue it. Unlike --restore-from, source and dest are NOT reversed — this continues the
    /// same source -> dest direction, relying on robocopy's own default behaviour of skipping
    /// destination files that already match the source, so whatever fully landed before the
    /// interruption isn't re-copied. Not a substitute for true mid-file resume (this crate never
    /// passes robocopy's /Z, deliberately, for its throughput cost on small files).
    #[arg(long, value_name = "CHECKPOINT_PATH", conflicts_with = "restore_from")]
    pub resume_from: Option<PathBuf>,

    // ── F10.1: HTML Standalone Dashboard Report ──────────────────────────────
    /// Path to write an interactive HTML summary report.
    #[arg(long, value_name = "PATH")]
    pub html_report_path: Option<PathBuf>,

    // ── F11.1: State Cache & Deduplication ───────────────────────────────────
    /// [NOT IMPLEMENTED] Reserved for incremental state caching (.ingest_cache) to skip
    /// unchanged files; accepted for forward compatibility but currently has no effect.
    #[arg(long, default_value_t = false)]
    pub enable_dedup: bool,

    // ── F15.1: Zero-Trust Streaming Encryption ──────────────────────────────
    /// Encrypt every copied file in the destination with AES-256-GCM after the transfer
    /// completes. VALUE is the key material: `env:NAME` reads it from environment variable
    /// NAME, `file:PATH` reads it from the first line of PATH, and any other value is treated
    /// as a literal passphrase (avoid this on shared/multi-user hosts: it is visible in the
    /// process list). The key is stretched to 256 bits with SHA-256.
    #[arg(long, value_name = "KEY")]
    pub encrypt_aes256: Option<String>,

    // ── F25b: Zero-Trust Streaming Decryption ─────────────────────────────────
    /// Decrypt every copied file in the destination with AES-256-GCM after the transfer
    /// completes — the counterpart to --encrypt-aes256, using the same VALUE key format
    /// (`env:NAME`, `file:PATH`, or a literal passphrase). Typically combined with
    /// --restore-from to decrypt a backup while restoring it, but works after any successful
    /// transfer. Cannot be combined with --encrypt-aes256 in the same run.
    #[arg(long, value_name = "KEY")]
    pub decrypt: Option<String>,

    // ── F18.1: Direct Cloud Sync ─────────────────────────────────────────────
    /// [NOT IMPLEMENTED] Reserved for direct S3/Azure Blob sync; accepted for forward
    /// compatibility but currently has no effect (no cloud transfer is performed).
    #[arg(long, value_name = "URI")]
    pub cloud_sync_target: Option<String>,

    // ── F19.1/F37: Windows Service Registration ──────────────────────────────
    /// Registers this binary as a Windows service (via the Service Control Manager) and exits
    /// without running a backup now. The service starts `OnDemand` (not automatic) and, once
    /// running, is idle — it only responds to Stop/Interrogate control requests; F41 is expected
    /// to give it real work to do. Requires Administrator. Does NOT require --source/--dest: this
    /// registers the binary itself, not any particular backup invocation.
    #[arg(long, default_value_t = false, conflicts_with = "uninstall_service")]
    pub install_service: bool,

    /// Removes a previously installed Windows service and exits. Requires Administrator. Does NOT
    /// require --source/--dest.
    #[arg(long, default_value_t = false, conflicts_with = "install_service")]
    pub uninstall_service: bool,

    /// Analyse this destination's run history and print deterministic suggestions (schedule
    /// interval, retention cost, thread count, anomalies, recurring integrity failures).
    ///
    /// Reads `.rustcopy_history.jsonl` from the `--report-path` directory, written automatically
    /// at the end of every run. Needs neither `--source` nor `--dest`: it inspects past runs and
    /// copies nothing. Pass the same `--report-path` your runs use.
    /// Involves no language model and no network — see `src/advise.rs`.
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = ["restore_from", "resume_from", "install_service", "uninstall_service"]
    )]
    pub advise: bool,

    /// List every Windows Task Scheduler entry that invokes this binary, then exit.
    ///
    /// Closes a gap left by --install-schedule/--uninstall-schedule: neither one could previously
    /// show what is already scheduled, short of running `schtasks /Query` directly and reading
    /// its output by hand. Needs neither --source nor --dest, like --advise: it inspects the
    /// scheduler and copies nothing. Read-only — never installs, updates or removes a schedule.
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = ["restore_from", "resume_from", "install_service", "uninstall_service"]
    )]
    pub list_schedules: bool,

    /// Store a secret in the Windows Credential Manager under this name, then exit.
    ///
    /// The secret itself is read from **stdin**, never from the command line: an argument would be
    /// visible in the process list, which is the exact exposure `--encrypt-aes256 <literal>` warns
    /// about. Reference it afterwards as `keyring:NAME` wherever a key is accepted.
    ///
    ///   echo my-secret | robocopy_ingest --set-credential nas-key
    ///
    /// Requires neither `--source` nor `--dest`.
    #[arg(
        long,
        value_name = "NAME",
        // Every meta-operation returns early from `run()`, so combining two would silently run
        // whichever branch comes first and ignore the other. Rejecting the combination at parse
        // time says so instead of quietly picking one.
        conflicts_with_all = [
            "delete_credential",
            "advise",
            "list_schedules",
            "install_schedule",
            "uninstall_schedule",
            "install_service",
            "uninstall_service",
            "restore_from",
            "resume_from"
        ]
    )]
    pub set_credential: Option<String>,

    /// Remove a secret previously stored with `--set-credential`, then exit.
    #[arg(
        long,
        value_name = "NAME",
        conflicts_with_all = [
            "advise",
            "list_schedules",
            "install_schedule",
            "uninstall_schedule",
            "install_service",
            "uninstall_service",
            "restore_from",
            "resume_from"
        ]
    )]
    pub delete_credential: Option<String>,

    // ── F33 internal: multi-job cache/manifest namespacing (D12) ─────────────
    /// Publish this run's progress to a file a supervisor can read.
    ///
    /// The terminal progress bar is drawn with ANSI escapes for a person; a program watching from
    /// another process needs the numbers instead. One JSON line, rewritten in place at most once a
    /// second, carrying the phase as well as the counts — a window showing only "bytes copied"
    /// would sit at 100% for the whole verification and look hung.
    ///
    /// Costs nothing when absent, which is every scheduled run. A failure to publish never fails
    /// the backup: progress is a convenience, the copy is the product.
    #[arg(long, value_name = "PATH")]
    pub progress_file: Option<PathBuf>,

    /// Stop the run when this file appears, exactly as Ctrl+C would.
    ///
    /// A supervisor that is not a terminal — the desktop console, a service wrapper, a CI job —
    /// has no clean way to deliver Ctrl+C to this process on Windows. `GenerateConsoleCtrlEvent`
    /// needs the caller attached to a console and the child in its own process group, and a GUI
    /// built with `windows_subsystem = "windows"` has no console at all. Killing the process
    /// instead loses the checkpoint, which is the property that makes an interruption resumable.
    ///
    /// So the supervisor creates this file and the run treats it as the interrupt it is, taking
    /// the **same** branch Ctrl+C takes rather than a second implementation that could drift.
    #[arg(long, value_name = "PATH")]
    pub cancel_file: Option<PathBuf>,

    /// Internal-only, never a real CLI flag (`#[arg(skip)]`, no `--job-name`): set by
    /// `main.rs::run_jobs` to the current job's name so the fast-verify cache and the
    /// backup-generations manifest, both of which live purely under `dest` with no user-facing
    /// path override, don't collide when two jobs in the same `[[jobs]]` batch share a `dest`.
    /// `None` in the single-job path, which keeps today's unnamespaced filenames unchanged.
    #[arg(skip)]
    pub job_name: Option<String>,

    /// Internal-only, never a real CLI flag: set by `main.rs::run_jobs` to this job's 1-based
    /// position and the batch's total count, so a published progress sample (`--progress-file`)
    /// can say which job of a batch is currently running instead of leaving a supervisor watching
    /// one continuous progress bar with no idea it is now on job 3 of 5. `None` in the single-job
    /// path, same as `job_name` above.
    #[arg(skip)]
    pub batch_index: Option<u32>,
    #[arg(skip)]
    pub batch_total: Option<u32>,
}

impl Args {
    /// Real path to the source directory.
    ///
    /// Panics if called before `validate()` has confirmed `--source` was supplied — clap's
    /// `required_unless_present = "restore_from"` guarantees it is `Some` on every code path that
    /// isn't short-circuited by restore mode (`validate()` returns early for that case, and
    /// `restore::build_restore_args` always supplies both paths explicitly), so this is an
    /// invariant violation, not a user-facing error, if it ever fires.
    #[allow(clippy::expect_used)]
    pub fn source(&self) -> &Path {
        self.source
            .as_deref()
            .expect("--source is required unless --restore-from is given, enforced by clap")
    }

    /// Real path to the destination directory. See [`Self::source`] for the invariant.
    #[allow(clippy::expect_used)]
    pub fn dest(&self) -> &Path {
        self.dest
            .as_deref()
            .expect("--dest is required unless --restore-from is given, enforced by clap")
    }

    /// Merge non-None fields from `config`'s top-level defaults into `self` where CLI flags were
    /// not explicitly passed. Single-job entry point, unchanged in behaviour since before F33 —
    /// the actual per-field merge lives in [`Self::apply_job_config`] so multi-job mode (F33) can
    /// reuse the exact same rules for each resolved `[[jobs]]` entry.
    pub fn merge_config(&mut self, config: crate::config::IngestConfig) {
        self.apply_job_config(&config.defaults);
    }

    /// Merge non-None fields from `job` into `self` where CLI flags were not explicitly passed.
    /// Shared by the single-job config path ([`Self::merge_config`]) and F33's multi-job path,
    /// which calls this once per resolved job (top-level defaults already folded in via
    /// [`crate::config::JobConfig::merged_over`]).
    pub fn apply_job_config(&mut self, job: &crate::config::JobConfig) {
        if self.source.is_none() {
            self.source = job.source.clone();
        }
        if self.dest.is_none() {
            self.dest = job.dest.clone();
        }
        if let Some(pat) = &job.pattern {
            // Only apply the config file's pattern when the CLI still holds clap's own default
            // ("*"); otherwise an explicit `--pattern` on the command line would be silently
            // overwritten. This was previously checked against "*.csv", which never matched the
            // real default and made the config file's pattern dead on arrival.
            //
            // Caveat: this can't distinguish "user typed --pattern '*'" from "user didn't pass
            // --pattern at all" (both look identical here); doing that properly needs
            // `ArgMatches::value_source`, which isn't threaded through yet for any of the
            // merge_config fields (all of them share this limitation, not just pattern).
            if self.pattern == "*" {
                self.pattern = pat.clone();
            }
        }
        if let Some(th) = job.threads {
            self.threads = th;
        }
        if let Some(ret) = job.retries {
            self.retries = ret;
        }
        if let Some(w) = job.retry_wait_seconds {
            self.retry_wait_seconds = w;
        }
        if let Some(v) = job.verify_integrity {
            self.verify_integrity = v;
        }
        if let Some(v) = job.fast_verify {
            self.fast_verify = v;
        }
        if let Some(v) = job.ignore_transient_missing {
            self.ignore_transient_missing = v;
        }
        if let Some(v) = job.exclude_junctions {
            self.exclude_junctions = v;
        }
        if let Some(html) = &job.html_report_path {
            self.html_report_path = Some(html.clone());
        }
        if let Some(algo) = job.hash_algo {
            self.hash_algo = algo;
        }
        if let Some(base) = job.compare_baseline {
            self.compare_baseline = base;
        }
        if let Some(rep) = &job.report_path {
            self.report_path = rep.clone();
        }
        if let Some(log) = &job.log_path {
            self.log_path = log.clone();
        }
        if let Some(dry) = job.dry_run {
            self.dry_run = dry;
        }
        if let Some(bt) = job.backup_type {
            self.backup_type = Some(bt);
        }
        if let Some(keep) = job.keep_generations {
            self.keep_generations = Some(keep);
        }
        if let Some(mir) = job.mirror {
            self.mirror = mir;
        }
        if let Some(ex_files) = &job.exclude_files {
            self.exclude_files.extend(ex_files.iter().cloned());
        }
        if let Some(ex_dirs) = &job.exclude_dirs {
            self.exclude_dirs.extend(ex_dirs.iter().cloned());
        }
        if let Some(min_age) = job.min_age_days {
            self.min_age_days = Some(min_age);
        }
        if let Some(max_age) = job.max_age_days {
            self.max_age_days = Some(max_age);
        }
        if let Some(limit) = job.bandwidth_limit_mbps {
            self.bandwidth_limit_mbps = Some(limit);
        }
        if let Some(no_pre) = job.no_prescan {
            self.no_prescan = no_pre;
        }
        if let Some(lp) = job.long_paths {
            self.long_paths = lp;
        }
        if let Some(pt) = job.preserve_timestamps {
            self.preserve_timestamps = pt;
        }
        if let Some(pa) = job.preserve_acl {
            self.preserve_acl = pa;
        }
        if let Some(wh) = &job.webhook_url {
            self.webhook_url = Some(wh.clone());
        }
        if let Some(pre) = &job.pre_command {
            self.pre_command = Some(pre.clone());
        }
        if let Some(post) = &job.post_command {
            self.post_command = Some(post.clone());
        }
    }
    /// Rejects a `--cancel-file` that already exists, once, before any job starts.
    ///
    /// Deliberately **not** part of [`Self::validate`], which `run_jobs` calls per job: a stop
    /// file created while a batch is running is a legitimate signal, and validating it per job
    /// turned that signal into a usage error — the remaining jobs were skipped one by one with a
    /// configuration complaint, the batch exited 2 instead of 1, and no checkpoint was written.
    /// Checked once at startup it does what it was for: catching a file left behind by an earlier
    /// run, which would otherwise stop this one the instant it first looked and read like a crash.
    pub fn validate_cancel_file_absent(&self) -> Result<(), IngestError> {
        match self.cancel_file.as_ref() {
            Some(path) if path.exists() => Err(IngestError::CancelFileAlreadyExists(path.clone())),
            _ => Ok(()),
        }
    }

    pub fn validate(&self) -> Result<(), IngestError> {
        // Checked before the restore-mode short-circuit below: --decrypt's primary use case is
        // exactly --restore-from, so this conflict must still be caught in that mode.
        if self.encrypt_aes256.is_some() && self.decrypt.is_some() {
            return Err(IngestError::EncryptAndDecryptConflict);
        }
        // F34: --mirror deletes destination-only files to match the source 1:1, which conflicts
        // with --backup-type's destination layout (a manifest plus multiple generation
        // subfolders, not a single mirrored tree).
        if self.backup_type.is_some() && self.mirror {
            return Err(IngestError::BackupTypeAndMirrorConflict);
        }
        // F35: nothing to rotate without a generation history in the first place.
        if self.keep_generations.is_some() && self.backup_type.is_none() {
            return Err(IngestError::KeepGenerationsWithoutBackupType);
        }
        if self.restore_from.is_some()
            || self.resume_from.is_some()
            || self.uninstall_schedule.is_some()
            || self.install_service
            || self.uninstall_service
            // --advise reads a history file and prints; none of the transfer-shaped checks below
            // (thread range, source exists, dest writable) describe anything it does.
            || self.advise
            // --list-schedules queries the task scheduler and prints; same reasoning as --advise.
            || self.list_schedules
            // Credential management touches no path: none of the transfer checks below apply.
            || self.set_credential.is_some()
            || self.delete_credential.is_some()
        {
            return Ok(());
        }
        if !(MIN_THREADS as u16..=MAX_THREADS).contains(&self.threads) {
            return Err(IngestError::InvalidThreads(self.threads));
        }
        // See the F33 note on `IngestError::SourceOrDestMissingFromConfig`: with --config in the
        // mix, clap no longer guarantees source/dest are set by the time we get here.
        if self.source.is_none() || self.dest.is_none() {
            return Err(IngestError::SourceOrDestMissingFromConfig);
        }
        let source = self.source();
        let dest = self.dest();
        if !source.exists() {
            return Err(IngestError::SourceMissing(source.to_path_buf()));
        }
        if !source.is_dir() {
            return Err(IngestError::SourceNotADirectory(source.to_path_buf()));
        }
        if dest.exists() && !dest.is_dir() {
            return Err(IngestError::DestNotADirectory(dest.to_path_buf()));
        }
        if self.pattern.trim().is_empty() {
            return Err(IngestError::EmptyPattern);
        }
        // F3.4: prevent copying a directory into itself.
        let source_canonical = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        let dest_canonical = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
        if source_canonical == dest_canonical {
            return Err(IngestError::SourceEqualsDestination(source_canonical));
        }
        // Check that dest is not inside source (would cause infinite recursion with /E).
        if dest_canonical.starts_with(&source_canonical) {
            return Err(IngestError::DestInsideSource {
                src: source_canonical,
                dest: dest_canonical,
            });
        }
        Ok(())
    }

    /// Outer retry policy: `--retries` extra attempts with `--retry-wait-seconds` as backoff base.
    pub fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy::new(self.retries, self.retry_wait_seconds)
    }

    /// F27: logger verbosity/rotation settings derived from `--log-level`/`--quiet`/
    /// `--log-max-bytes`/`--log-max-backups`. `--quiet` and `--log-level` are mutually exclusive
    /// (enforced by clap), so exactly one of them decides the effective level.
    pub fn log_config(&self) -> logging::LogConfig {
        let level = if self.quiet {
            "warn"
        } else {
            self.log_level.as_str()
        };
        logging::LogConfig {
            filter: format!("robocopy_ingest={level},warn"),
            max_bytes: self.log_max_bytes,
            max_backups: self.log_max_backups,
        }
    }

    /// Convert MB/s bandwidth limit to robocopy's /IPG (inter-packet gap in milliseconds).
    ///
    /// Robocopy's /IPG represents the gap in **milliseconds** inserted after every 64 KB packet.
    /// Formula, with `mbps` in megabytes (10^6 bytes) per second:
    ///   gap_ms = packet_bits / bandwidth_bps * 1000
    ///          = (65_536 bytes * 8 bits/byte) / (mbps * 1_000_000 bytes/s * 8 bits/byte) * 1000
    ///          = 65_536 / (mbps * 1_000_000) * 1000
    ///          = 65.536 / mbps
    ///
    /// (The previous implementation used 524_288 / mbps — the numerator in bits instead of
    /// converting through the correct byte/bit factors — which made the computed gap about
    /// 8000x too large: at 100 MB/s it produced a 5242 ms gap between 64 KB packets, throttling
    /// the real transfer down to roughly 12 KB/s instead of ~100 MB/s.)
    ///
    /// At high requested bandwidths the exact gap rounds below 1 ms, which robocopy cannot
    /// express; those requests are clamped to the smallest representable gap (1 ms) rather than
    /// silently becoming "no throttle".
    pub fn inter_packet_gap_ms(&self) -> Option<u32> {
        self.bandwidth_limit_mbps.and_then(|mbps| {
            if mbps == 0 {
                return None;
            }
            let gap_ms = 65.536_f64 / mbps as f64;
            Some(gap_ms.round().clamp(1.0, u32::MAX as f64) as u32)
        })
    }

    /// Request handed to the copy engines.
    pub fn copy_request(&self, dest: PathBuf) -> CopyRequest {
        CopyRequest {
            source: self.source().to_path_buf(),
            dest,
            pattern: self.pattern.clone(),
            threads: self.threads,
            file_retries: self.retries,
            retry_wait_seconds: self.retry_wait_seconds,
            dry_run: self.dry_run,
            mirror: self.mirror,
            exclude_files: self.exclude_files.clone(),
            exclude_dirs: self.exclude_dirs.clone(),
            min_age_days: self.min_age_days,
            max_age_days: self.max_age_days,
            inter_packet_gap_ms: self.inter_packet_gap_ms(),
            prescan: !self.no_prescan,
            long_paths: self.long_paths,
            preserve_timestamps: self.preserve_timestamps,
            preserve_acl: self.preserve_acl,
            exclude_junctions: self.exclude_junctions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn base_args() -> Vec<&'static str> {
        vec!["robocopy_ingest", "--source", ".", "--dest", "./out"]
    }

    #[test]
    fn clap_definition_is_valid() {
        Args::command().debug_assert();
    }

    #[test]
    fn defaults_match_specification() {
        let args = Args::try_parse_from(base_args()).expect("parse");
        assert_eq!(args.pattern, "*");
        // Thread count defaults to logical CPUs, clamped to [1, 128].
        assert!((MIN_THREADS as u16..=MAX_THREADS).contains(&args.threads));
        assert_eq!(args.retries, 3);
        assert_eq!(args.retry_wait_seconds, 5);
        assert!(!args.verify_integrity);
        assert!(!args.compare_baseline);
        assert!(!args.dry_run);
        assert!(!args.mirror);
        assert!(args.exclude_files.is_empty());
        assert!(args.exclude_dirs.is_empty());
        assert!(args.min_age_days.is_none());
        assert!(args.max_age_days.is_none());
        assert!(args.bandwidth_limit_mbps.is_none());
        assert!(!args.no_prescan);
        assert!(!args.exclude_junctions);
        assert!(!args.fast_verify);
        assert!(!args.ignore_transient_missing);
        assert_eq!(args.log_level, LogLevel::Info); // D18: default changed from Debug
        assert!(!args.quiet);
        assert_eq!(args.log_max_bytes, crate::logging::DEFAULT_MAX_LOG_BYTES);
        assert_eq!(
            args.log_max_backups,
            crate::logging::DEFAULT_MAX_LOG_BACKUPS
        );
        assert!(!args.vss_snapshot);
        assert_eq!(
            args.report_path,
            PathBuf::from("./robocopy_ingest_report.json")
        );
        assert_eq!(args.log_path, PathBuf::from("./robocopy_ingest.log"));
    }

    #[test]
    fn source_and_dest_are_required() {
        assert!(Args::try_parse_from(["robocopy_ingest"]).is_err());
        assert!(Args::try_parse_from(["robocopy_ingest", "--source", "."]).is_err());
    }

    /// F24 regression test: `--restore-from` alone, with neither `--source` nor `--dest`, must
    /// parse successfully. This is the exact invocation from the README's disaster-recovery
    /// example, and the one a minimal clap repro (outside this crate) proved would fail with
    /// "a value is required for '--source <PATH>' but none was supplied" — caused by
    /// `default_value = ""` making clap treat the arg as unconditionally required, silently
    /// ignoring `required_unless_present`. This test parses only (no filesystem access, no
    /// tempdir needed): it exists to catch a clap-level regression, not to exercise the restore
    /// flow itself (see `tests/cli_smoke.rs` for a black-box test that actually runs the compiled
    /// binary end-to-end against a tempdir-only fixture).
    #[test]
    fn restore_from_alone_is_sufficient() {
        let args = Args::try_parse_from(["robocopy_ingest", "--restore-from", "report.json"])
            .expect("--restore-from alone must parse without --source/--dest");
        assert!(args.source.is_none());
        assert!(args.dest.is_none());
        assert_eq!(args.restore_from, Some(PathBuf::from("report.json")));
        // validate() must also accept this shape (it short-circuits for restore mode without
        // ever touching the None source/dest).
        assert!(args.validate().is_ok());
    }

    /// F33: `apply_job_config` (shared by single-job `merge_config` and multi-job mode) must
    /// behave exactly like the pre-F33 `merge_config` it was extracted from: `source`/`dest`
    /// only fill in when unset, `pattern` only applies while the CLI still holds clap's own
    /// default ("*"), and every other field unconditionally takes the config/job value when
    /// present (a documented pre-existing limitation — see the comment on the pattern branch —
    /// not something F33 introduced).
    #[test]
    fn apply_job_config_matches_merge_configs_established_field_rules() {
        let mut args = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            ".",
            "--dest",
            "./out",
            "--threads",
            "4",
        ])
        .expect("parse");

        let job = crate::config::JobConfig {
            threads: Some(64),            // unconditionally wins, like merge_config always did
            verify_integrity: Some(true), // must win: never set on the CLI
            pattern: Some("*.csv".to_string()), // must win: pattern still holds clap's default
            ..crate::config::JobConfig::default()
        };
        args.apply_job_config(&job);

        assert_eq!(args.threads, 64);
        assert!(args.verify_integrity);
        assert_eq!(args.pattern, "*.csv");
    }

    /// Deliberate asymmetry between the two exclude-merge call sites (documented in
    /// `ROADMAP.md` and `PIANO_MIGLIORAMENTI.md`, not a bug): `apply_job_config` (this call
    /// site, shared by single-job `merge_config` and multi-job mode) ACCUMULATES CLI-provided
    /// excludes with the config/job's own list, because CLI and top-level TOML defaults are two
    /// independent sources for the same run. Contrast with
    /// `JobConfig::merged_over`'s REPLACE semantics (`config::tests::job_excludes_replace_not_extend_the_shared_defaults`),
    /// which governs a `[[jobs]]` entry inheriting from the file's own top-level defaults — a
    /// different relationship (inheritance-with-override, not two sources for one run). Do not
    /// "fix" this call site to replace instead of extend: that would silently drop an
    /// `--exclude-files` the user typed on the command line whenever a config file is also
    /// given.
    #[test]
    fn apply_job_config_accumulates_cli_excludes_with_config_excludes() {
        let mut args = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            ".",
            "--dest",
            "./out",
            "--exclude-files",
            "*.tmp",
            "--exclude-dirs",
            "node_modules",
        ])
        .expect("parse");

        let job = crate::config::JobConfig {
            exclude_files: Some(vec!["thumbs.db".to_string()]),
            exclude_dirs: Some(vec![".git".to_string()]),
            ..crate::config::JobConfig::default()
        };
        args.apply_job_config(&job);

        assert_eq!(
            args.exclude_files,
            vec!["*.tmp".to_string(), "thumbs.db".to_string()],
            "CLI excludes must survive alongside the config's own, not be replaced by them"
        );
        assert_eq!(
            args.exclude_dirs,
            vec!["node_modules".to_string(), ".git".to_string()]
        );
    }

    /// F31: `--resume-from` alone must parse without `--source`/`--dest`, mirroring F24's fix for
    /// `--restore-from` (both share the same `required_unless_present_any` mechanism).
    #[test]
    fn resume_from_alone_is_sufficient() {
        let args = Args::try_parse_from(["robocopy_ingest", "--resume-from", "checkpoint.json"])
            .expect("--resume-from alone must parse without --source/--dest");
        assert!(args.source.is_none());
        assert!(args.dest.is_none());
        assert_eq!(args.resume_from, Some(PathBuf::from("checkpoint.json")));
        assert!(args.validate().is_ok());
    }

    #[test]
    fn restore_from_and_resume_from_together_are_rejected() {
        assert!(Args::try_parse_from([
            "robocopy_ingest",
            "--restore-from",
            "report.json",
            "--resume-from",
            "checkpoint.json",
        ])
        .is_err());
    }

    /// F34: `--backup-type` and `--mirror` are mutually exclusive (the generation-folder+manifest
    /// destination layout has nothing in common with `/MIR`'s "purge everything not in source").
    #[test]
    fn backup_type_and_mirror_together_are_rejected() {
        use crate::generations::BackupType;
        let mut args =
            Args::try_parse_from(["robocopy_ingest", "--source", ".", "--dest", "./out"])
                .expect("parse");
        args.backup_type = Some(BackupType::Full);
        args.mirror = true;
        assert!(matches!(
            args.validate(),
            Err(IngestError::BackupTypeAndMirrorConflict)
        ));
    }

    /// F35: `--keep-generations` without `--backup-type` has nothing to rotate.
    #[test]
    fn keep_generations_without_backup_type_is_rejected() {
        let mut args =
            Args::try_parse_from(["robocopy_ingest", "--source", ".", "--dest", "./out"])
                .expect("parse");
        args.keep_generations = Some(3);
        assert!(matches!(
            args.validate(),
            Err(IngestError::KeepGenerationsWithoutBackupType)
        ));
    }

    #[test]
    #[should_panic(expected = "--source is required unless --restore-from is given")]
    fn source_accessor_panics_if_invariant_is_violated() {
        // Directly exercises the accessor's documented invariant (never reachable via the real
        // CLI, since clap enforces required_unless_present before validate() ever runs).
        let args = Args::try_parse_from(["robocopy_ingest", "--restore-from", "report.json"])
            .expect("parse");
        let _ = args.source();
    }

    #[test]
    fn flags_are_parsed() {
        let mut argv = base_args();
        argv.extend([
            "--threads",
            "32",
            "--verify-integrity",
            "--compare-baseline",
            "--dry-run",
        ]);
        let args = Args::try_parse_from(argv).expect("parse");
        assert_eq!(args.threads, 32);
        assert!(args.verify_integrity);
        assert!(args.compare_baseline);
        assert!(args.dry_run);
    }

    #[test]
    fn thread_count_bounds_are_enforced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().to_str().expect("utf8 path");

        // src == src → SourceEqualsDestination, not InvalidThreads.
        // Use a real different dest path for the bound checks.
        let dest = tempfile::tempdir().expect("dest");
        let dst = dest.path().to_str().expect("utf8 dest");

        for (threads, ok) in [("0", false), ("1", true), ("128", true), ("129", false)] {
            let args = Args::try_parse_from([
                "robocopy_ingest",
                "--source",
                src,
                "--dest",
                dst,
                "--threads",
                threads,
            ])
            .expect("parse");
            let result = args.validate();
            assert_eq!(
                result.is_ok(),
                ok,
                "threads={threads} expected ok={ok}, got: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn missing_source_is_rejected() {
        let args = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            "/definitely/not/here",
            "--dest",
            "/tmp",
        ])
        .expect("parse");
        assert!(matches!(
            args.validate(),
            Err(IngestError::SourceMissing(_))
        ));
    }

    #[test]
    fn empty_pattern_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().to_str().expect("utf8 path");
        let args = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            src,
            "--dest",
            src,
            "--pattern",
            "   ",
        ])
        .expect("parse");
        assert!(matches!(args.validate(), Err(IngestError::EmptyPattern)));
    }

    #[test]
    fn retry_policy_mirrors_cli_flags() {
        let mut argv = base_args();
        argv.extend(["--retries", "5", "--retry-wait-seconds", "7"]);
        let args = Args::try_parse_from(argv).expect("parse");

        let policy = args.retry_policy();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.total_attempts(), 6);
        assert_eq!(policy.backoff(0), std::time::Duration::from_secs(7));
        assert_eq!(policy.backoff(1), std::time::Duration::from_secs(14));
    }

    #[test]
    fn copy_request_targets_the_given_destination() {
        let mut argv = base_args();
        argv.extend(["--threads", "16", "--dry-run"]);
        let args = Args::try_parse_from(argv).expect("parse");

        let request = args.copy_request(PathBuf::from("/tmp/baseline"));
        assert_eq!(request.dest, PathBuf::from("/tmp/baseline"));
        assert_eq!(request.source, args.source());
        assert_eq!(request.threads, 16);
        assert_eq!(request.pattern, "*");
        assert!(request.dry_run);
        assert!(!request.mirror);
        assert!(request.exclude_files.is_empty());
        assert!(request.inter_packet_gap_ms.is_none());
        assert!(request.prescan);
    }

    #[test]
    fn new_feature_flags_are_parsed() {
        let mut argv = base_args();
        argv.extend([
            "--mirror",
            "--exclude-files",
            "*.tmp",
            "--exclude-dirs",
            ".git",
            "--min-age-days",
            "7",
            "--max-age-days",
            "30",
            "--bandwidth-limit-mbps",
            "10",
            "--no-prescan",
            "--exclude-junctions",
        ]);
        let args = Args::try_parse_from(argv).expect("parse");

        assert!(args.mirror);
        assert_eq!(args.exclude_files, vec!["*.tmp".to_string()]);
        assert_eq!(args.exclude_dirs, vec![".git".to_string()]);
        assert_eq!(args.min_age_days, Some(7));
        assert_eq!(args.max_age_days, Some(30));
        assert_eq!(args.bandwidth_limit_mbps, Some(10));
        assert!(args.no_prescan);
        assert!(args.exclude_junctions);

        let request = args.copy_request(PathBuf::from("/dst"));
        assert!(request.mirror);
        assert_eq!(request.min_age_days, Some(7));
        // 65.536 / 10 = 6.5536 -> rounds to 7 ms
        assert_eq!(request.inter_packet_gap_ms, Some(7));
        assert!(!request.prescan);
        assert!(request.exclude_junctions);
    }

    #[test]
    fn bandwidth_ipg_conversion_is_correct() {
        let mut argv = base_args();
        argv.extend(["--bandwidth-limit-mbps", "5"]);
        let args = Args::try_parse_from(argv).expect("parse");
        // 65.536 / 5 = 13.1072 -> rounds to 13 ms
        assert_eq!(args.inter_packet_gap_ms(), Some(13));

        // At high requested bandwidths the exact gap is below robocopy's 1ms granularity and
        // is clamped to the smallest representable value rather than silently disabling the
        // throttle.
        let mut argv = base_args();
        argv.extend(["--bandwidth-limit-mbps", "1000"]);
        let args = Args::try_parse_from(argv).expect("parse");
        assert_eq!(args.inter_packet_gap_ms(), Some(1));

        let mut argv = base_args();
        argv.extend(["--bandwidth-limit-mbps", "0"]);
        let args = Args::try_parse_from(argv).expect("parse");
        assert_eq!(args.inter_packet_gap_ms(), None);
    }

    /// F3.4: source == destination must be rejected.
    #[test]
    fn source_equals_dest_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_str().expect("utf8");
        let args = Args::try_parse_from(["robocopy_ingest", "--source", path, "--dest", path])
            .expect("parse");
        assert!(matches!(
            args.validate(),
            Err(IngestError::SourceEqualsDestination(_))
        ));
    }

    /// F27: default --log-level maps to the pre-existing DEFAULT_FILTER string unchanged.
    #[test]
    fn log_config_defaults_match_the_previous_hardcoded_filter() {
        let args = Args::try_parse_from(base_args()).expect("parse");
        assert_eq!(args.log_config().filter, crate::logging::DEFAULT_FILTER);
    }

    #[test]
    fn log_config_honours_an_explicit_log_level() {
        let mut argv = base_args();
        argv.extend(["--log-level", "warn"]);
        let args = Args::try_parse_from(argv).expect("parse");
        assert_eq!(args.log_config().filter, "robocopy_ingest=warn,warn");
    }

    /// F27: --quiet is shorthand for --log-level warn.
    #[test]
    fn quiet_produces_a_warn_only_filter() {
        let mut argv = base_args();
        argv.push("--quiet");
        let args = Args::try_parse_from(argv).expect("parse");
        assert!(args.quiet);
        assert_eq!(args.log_config().filter, "robocopy_ingest=warn,warn");
    }

    /// F27: --quiet and --log-level are mutually exclusive — combining them is a usage error,
    /// not a silent "one wins" ambiguity.
    #[test]
    fn quiet_and_log_level_together_are_rejected() {
        let mut argv = base_args();
        argv.extend(["--quiet", "--log-level", "warn"]);
        assert!(Args::try_parse_from(argv).is_err());
    }

    #[test]
    fn log_rotation_flags_are_parsed() {
        let mut argv = base_args();
        argv.extend(["--log-max-bytes", "1000", "--log-max-backups", "5"]);
        let args = Args::try_parse_from(argv).expect("parse");
        assert_eq!(args.log_max_bytes, 1000);
        assert_eq!(args.log_max_backups, 5);
        let config = args.log_config();
        assert_eq!(config.max_bytes, 1000);
        assert_eq!(config.max_backups, 5);
    }

    /// F3.4: destination inside source must be rejected.
    #[test]
    fn dest_inside_source_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().to_str().expect("utf8 src");
        let dest = dir.path().join("subdir");
        std::fs::create_dir_all(&dest).expect("create dest");
        let dst = dest.to_str().expect("utf8 dst");
        let args = Args::try_parse_from(["robocopy_ingest", "--source", src, "--dest", dst])
            .expect("parse");
        assert!(matches!(
            args.validate(),
            Err(IngestError::DestInsideSource { .. })
        ));
    }
}
