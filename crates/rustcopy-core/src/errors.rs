//! Error types for the ingestion CLI.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("--threads must be between 1 and 128, got {0}")]
    InvalidThreads(u16),

    /// F33: `--config` is exempt from clap's `required_unless_present_any` on `--source`/`--dest`
    /// (a config file is expected to supply them), which means `Args::source()`/`dest()`'s
    /// documented "clap enforces this before validate() runs" invariant no longer holds
    /// unconditionally for that path — a config file (or `[[jobs]]` defaults) that itself omits
    /// `source`/`dest` must be caught here with a clear error instead of panicking.
    #[error(
        "--source and --dest must be set on the command line or in the config file (via --config)"
    )]
    SourceOrDestMissingFromConfig,

    /// F34: `--mirror` purges destination-only files to match the source tree 1:1; that's
    /// incompatible with `--backup-type`'s destination layout (a manifest plus one subfolder per
    /// generation, not a single mirrored tree).
    #[error("--backup-type and --mirror cannot both be given: --backup-type's destination holds a manifest and multiple generation subfolders, not a single mirrored tree")]
    BackupTypeAndMirrorConflict,

    /// F35: `--keep-generations` only means something once `--backup-type` is producing
    /// generations to rotate in the first place.
    #[error("--keep-generations requires --backup-type: there is nothing to rotate without a generation history")]
    KeepGenerationsWithoutBackupType,

    /// F54. The editor may narrow risk, never widen it: a job that purges the destination cannot
    /// be born in a user interface. See `job_editor`'s module header for why the field is still
    /// writable in the other direction.
    #[error("the editor cannot turn mirroring on for job {0}: a job that deletes at the destination must be written in the configuration file by hand")]
    EditorCannotEnableMirror(String),

    /// F54. Retention deletes whole generation cycles.
    #[error("the editor cannot introduce keep_generations for job {0}: retention deletes old generations and must be written in the configuration file by hand")]
    EditorCannotIntroduceRetention(String),

    /// F54. Lowering the count is the half that is easy to miss, because the number gets smaller
    /// while the deletion gets larger.
    #[error("the editor cannot lower keep_generations for job {name} from {from} to {to}: keeping fewer cycles deletes more")]
    EditorCannotLowerRetention {
        name: String,
        from: usize,
        to: usize,
    },

    /// F54. `--mirror` uses the prescan to work out what it would delete; without it the safety
    /// diff cannot run at all.
    #[error("the editor cannot disable the prescan on the mirroring job {0}: mirroring relies on it to know what it would delete")]
    EditorCannotDisablePrescanOnMirror(String),

    /// F54. A file with no `[[jobs]]` keeps its single job in the top-level fields; turning it
    /// into a multi-job file changes what every one of those fields means.
    #[error("the editor cannot split the single-job configuration holding {0} into several jobs: add the [[jobs]] section by hand first")]
    EditorCannotSplitSingleJobConfig(String),

    /// `--cancel-file` names a file that must not exist yet: one left behind by an earlier run
    /// would stop this one the moment it looked, which reads like a crash rather than a stop.
    #[error("the --cancel-file {0} already exists: it would stop this run immediately. Remove it, or name a path that does not exist yet")]
    CancelFileAlreadyExists(PathBuf),

    /// F54. The editor always writes a new file and leaves the substitution to the operator.
    #[error("refusing to overwrite {0}: the editor writes a proposal and leaves it to you to put it in place")]
    EditorWouldOverwrite(PathBuf),

    #[error("source directory does not exist: {0}")]
    SourceMissing(PathBuf),

    #[error("source path is not a directory: {0}")]
    SourceNotADirectory(PathBuf),

    #[error("destination path exists but is not a directory: {0}")]
    DestNotADirectory(PathBuf),

    /// F3.4: source and destination resolve to the same path.
    #[error("source and destination are the same path: {0}")]
    SourceEqualsDestination(PathBuf),

    /// F3.4: destination is inside the source tree (would cause infinite recursion).
    #[error(
        "destination {dest} is inside source {src}: copying a directory into itself is not allowed"
    )]
    DestInsideSource { src: PathBuf, dest: PathBuf },

    #[error("--pattern must not be empty")]
    EmptyPattern,

    #[error("invalid file pattern {pattern:?}: {source}")]
    InvalidPattern {
        pattern: String,
        #[source]
        source: globset::Error,
    },

    /// Raised by the real process runner when robocopy.exe cannot exist on this platform.
    #[error(
        "robocopy.exe is only available on Windows; run this tool on Windows for real transfers \
         (on other platforms only the naive baseline engine and the test suite can run)"
    )]
    RobocopyUnavailable,

    #[error("failed to launch {program}: {source}")]
    SpawnFailed {
        program: String,
        #[source]
        source: std::io::Error,
    },

    #[error("robocopy exited with code {code} ({description}) after {attempts} attempt(s)")]
    CopyFailed {
        code: i32,
        description: String,
        attempts: u32,
    },

    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// F21: mirror mode was about to purge destination files and no confirmation was given.
    #[error(
        "--mirror would purge {count} file(s)/dir(s) from the destination that are not present \
         in the source; re-run with --force-purge to proceed, confirm interactively, or drop --mirror"
    )]
    MirrorPurgeAborted { count: usize },

    /// F35: retention rotation was about to delete old generation folders and no confirmation
    /// was given. Reuses the same `--force-purge`/interactive-confirmation gate as
    /// `MirrorPurgeAborted` — both are "about to delete data at --dest, get explicit go-ahead"
    /// situations.
    #[error(
        "--keep-generations would delete {count} old generation(s) from the destination; \
         re-run with --force-purge to proceed, confirm interactively, or drop --keep-generations"
    )]
    RetentionPurgeAborted { count: usize },

    #[error("encryption error: {0}")]
    Crypto(String),

    /// F25b: --encrypt-aes256 and --decrypt are mutually exclusive in a single run.
    #[error("--encrypt-aes256 and --decrypt cannot both be given in the same run")]
    EncryptAndDecryptConflict,

    /// F30: --vss-snapshot could not create/parse/delete a Volume Shadow Copy via vssadmin.exe.
    #[error("VSS snapshot error: {0}")]
    Vss(String),

    /// F39: `--pre-command` exited non-zero (or couldn't even be spawned, which surfaces as
    /// `SpawnFailed` instead). Fails the whole run: a pre-command is typically "stop the
    /// database before backing it up", and proceeding after that failed would silently back up a
    /// live, possibly inconsistent, database.
    #[error("pre-command {command:?} exited with status {code:?}")]
    PreCommandFailed { command: String, code: Option<i32> },

    /// F36: `--install-schedule`'s value didn't match any of the accepted spec shapes
    /// (`daily@HH:MM` / `hourly@N` / `weekly@DAY,...@HH:MM`).
    #[error("invalid --install-schedule spec {0:?}: expected daily@HH:MM, hourly@N, or weekly@DAY,...@HH:MM")]
    InvalidScheduleSpec(String),

    /// F36: `schtasks.exe` ran but reported failure (bad permissions, invalid task name, etc.), or
    /// scheduling was attempted on a non-Windows platform where Task Scheduler doesn't exist.
    #[error("schedule error: {0}")]
    Schedule(String),

    /// F37: `--install-service`/`--uninstall-service` failed talking to the Windows Service
    /// Control Manager (commonly: not running as Administrator), or was attempted on a
    /// non-Windows platform where SCM doesn't exist.
    #[error("service error: {0}")]
    Service(String),
}

impl IngestError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        IngestError::Io {
            path: path.into(),
            source,
        }
    }

    /// Whether re-running the copy could plausibly succeed.
    ///
    /// I/O errors are worth another attempt (a busy file, a flaky share); a missing binary, an
    /// invalid pattern or the wrong operating system are not, and retrying them would only add
    /// minutes of backoff before the same failure.
    ///
    /// F1.3 fix: CopyFailed now delegates to RobocopyStatus::should_retry() instead of
    /// unconditionally returning false. This makes the outer retry loop actually work for
    /// transient robocopy failures (exit code 8/9/11: files couldn't be copied due to locks).
    pub fn is_transient(&self) -> bool {
        match self {
            IngestError::Io { .. } => true,
            // SpawnFailed is transient unless the binary is missing or access is denied.
            IngestError::SpawnFailed { source, .. } => {
                let kind = source.kind();
                kind != std::io::ErrorKind::NotFound && kind != std::io::ErrorKind::PermissionDenied
            }
            // F1.3: delegate to the robocopy bitmask to determine retry-ability.
            // Code 8 (retry limit exceeded per file) is transient; code 16 (fatal config
            // error) is not. This matches what CopyOutcome::should_retry() already does.
            IngestError::CopyFailed { code, .. } => {
                crate::exit_code::RobocopyStatus::new(*code).should_retry()
            }
            IngestError::RobocopyUnavailable
            | IngestError::InvalidThreads(_)
            | IngestError::SourceOrDestMissingFromConfig
            | IngestError::BackupTypeAndMirrorConflict
            | IngestError::KeepGenerationsWithoutBackupType
            | IngestError::SourceMissing(_)
            | IngestError::SourceNotADirectory(_)
            | IngestError::DestNotADirectory(_)
            | IngestError::SourceEqualsDestination(_)
            | IngestError::DestInsideSource { .. }
            | IngestError::EmptyPattern
            | IngestError::InvalidPattern { .. }
            | IngestError::MirrorPurgeAborted { .. }
            | IngestError::RetentionPurgeAborted { .. }
            | IngestError::Crypto(_)
            | IngestError::EncryptAndDecryptConflict
            | IngestError::Vss(_)
            | IngestError::PreCommandFailed { .. }
            | IngestError::InvalidScheduleSpec(_)
            | IngestError::Schedule(_)
            | IngestError::Service(_)
            // F54: every editor refusal is a decision about what the interface may write, not a
            // condition that could clear on a second attempt.
            | IngestError::EditorCannotEnableMirror(_)
            | IngestError::EditorCannotIntroduceRetention(_)
            | IngestError::EditorCannotLowerRetention { .. }
            | IngestError::EditorCannotDisablePrescanOnMirror(_)
            | IngestError::EditorCannotSplitSingleJobConfig(_)
            | IngestError::EditorWouldOverwrite(_)
            | IngestError::CancelFileAlreadyExists(_) => false,
        }
    }
}
