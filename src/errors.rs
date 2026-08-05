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
    #[error("--source and --dest must be set on the command line or in the config file (via --config)")]
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
    #[error("destination {dest} is inside source {src}: copying a directory into itself is not allowed")]
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
                kind != std::io::ErrorKind::NotFound
                    && kind != std::io::ErrorKind::PermissionDenied
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
            | IngestError::Vss(_) => false,
        }
    }
}
