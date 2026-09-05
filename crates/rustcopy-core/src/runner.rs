//! Deciding how to launch a backup, for a supervisor that is not a terminal.
//!
//! The desktop console runs a job by starting the CLI as a child process. *Which* binary, *which*
//! arguments and *where* the stop file goes are decisions about what a supervisor may ask rustcopy
//! to do, so they live here and are tested here rather than being composed inside a Tauri command
//! — the same rule that keeps `gui_api` thin, applied to the one path that starts something.
//!
//! # The prohibition, made mechanical
//!
//! ROADMAP F61 forbids exposing `--force-purge`, unattended `--mirror`, retention purges and
//! service or schedule installation to an automated caller. [`run_arguments`] builds the argument
//! list from a fixed shape rather than forwarding anything a caller hands it, and a test asserts
//! that none of those flags can appear. A rule that is only written down is a rule someone
//! eventually forgets; this one fails a test.
//!
//! A mirroring job needs nothing extra here. `check_mirror_safety` asks for confirmation only when
//! stdin and stdout are terminals, and a child process launched from a window has neither, so a
//! job that would purge stops on its own with [`EXIT_MIRROR_ABORTED`]. The console reports that
//! outcome; it can never be the thing that authorises a purge.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::errors::IngestError;

/// The run succeeded.
pub const EXIT_SUCCESS: u8 = 0;
/// The transfer itself failed: robocopy exhausted its retries on some item.
pub const EXIT_INGESTION_PROBLEM: u8 = 1;
/// A usage error, or an environment problem nothing can recover from.
pub const EXIT_UNRECOVERABLE: u8 = 2;
/// `--mirror` was aborted because it would have purged destination files without confirmation.
pub const EXIT_MIRROR_ABORTED: u8 = 3;
/// The transfer succeeded but `--verify-integrity` found a mismatch, a missing or an unreadable
/// file. Distinct from [`EXIT_INGESTION_PROBLEM`] on purpose: "the data never landed" and "the
/// data landed and does not match" are different failures to whoever reads them (F29b).
pub const EXIT_INTEGRITY_FAILED: u8 = 4;
/// A `--keep-generations` retention purge was aborted. Kept apart from [`EXIT_MIRROR_ABORTED`] so
/// a scheduler can tell which purge it was (F35).
pub const EXIT_RETENTION_ABORTED: u8 = 5;
/// The preflight free-space check (F65) found less free space at the destination than the run
/// needs. Distinct from [`EXIT_UNRECOVERABLE`] so a scheduler can tell "the disk is full" apart
/// from "a flag was wrong" without parsing stderr.
pub const EXIT_INSUFFICIENT_DISK_SPACE: u8 = 6;

/// What an exit code means, in one place.
///
/// These meanings existed twice already — as bare constants in the CLI and as a hand-written map
/// in the console's history pane — and a third copy would only guarantee they drift. Exit codes
/// are a contract with schedulers (`AGENTS.md` rule 12), which makes reading one a judgement about
/// backup semantics and not a rendering detail: it belongs behind the same boundary as everything
/// else the frontend is not allowed to decide.
pub fn exit_code_meaning(code: u8) -> &'static str {
    match code {
        EXIT_SUCCESS => "riuscito",
        EXIT_INGESTION_PROBLEM => "trasferimento fallito",
        EXIT_UNRECOVERABLE => "errore d'uso o di configurazione",
        EXIT_MIRROR_ABORTED => "cancellazione di --mirror annullata",
        EXIT_INTEGRITY_FAILED => "copiato, ma la verifica ha trovato differenze",
        EXIT_RETENTION_ABORTED => "cancellazione della retention annullata",
        EXIT_INSUFFICIENT_DISK_SPACE => "spazio libero insufficiente in destinazione",
        _ => "sconosciuto",
    }
}

/// The CLI executable's file name on this platform.
pub const CLI_BINARY: &str = if cfg!(windows) {
    "robocopy_ingest.exe"
} else {
    "robocopy_ingest"
};

/// Finds the CLI beside the given executable.
///
/// Beside, and nowhere else: the installer puts both binaries in one directory, and searching
/// `PATH` instead would let a console launch some *other* rustcopy — an older one left on the
/// machine, or one a different user put earlier in their `PATH`. A supervisor should run the
/// engine it shipped with.
pub fn cli_beside(supervisor_exe: &Path) -> Result<PathBuf, IngestError> {
    let candidate = supervisor_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(CLI_BINARY);

    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(IngestError::CliBinaryNotFound(candidate))
    }
}

/// Distinguishes stop files produced within the same millisecond by the same process.
static CANCEL_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The directory stop files live in.
///
/// **Not** beside the configuration, which was the first choice and the wrong one: a perfectly
/// valid configuration can sit in a directory the operator can read and not write — under
/// `Program Files`, on a share mounted read-only. A run would start and then be impossible to
/// stop, because `stop_job` could not create the file, and the one guarantee this whole mechanism
/// exists to provide — a stop that leaves a checkpoint — would be the part that failed.
pub fn cancel_file_dir() -> PathBuf {
    std::env::temp_dir().join("rustcopy")
}

/// Names the stop file for one run inside `dir`.
///
/// The configuration's stem is kept in the name so a directory holding several stop files can be
/// read by a person, not only by the process that made them.
pub fn cancel_file_in(dir: &Path, config: &Path, stamp: &str) -> PathBuf {
    let stem = config
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "rustcopy".to_string());
    dir.join(format!("{stem}.stop-{stamp}"))
}

/// A stop file for a run starting now, in a directory the caller is known to be able to write.
///
/// The stamp carries the process id and a per-process counter as well as the clock. A millisecond
/// alone is not unique: two windows starting the same configuration inside one would pick the same
/// file, and a Stop in either would then interrupt both runs.
///
/// Creates the directory, so a caller cannot start a run it will not be able to stop.
pub fn cancel_file_for_now(config: &Path) -> Result<PathBuf, IngestError> {
    let dir = cancel_file_dir();
    std::fs::create_dir_all(&dir).map_err(|error| IngestError::io(&dir, error))?;

    let stamp = format!(
        "{}-{}-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S%.3f"),
        std::process::id(),
        CANCEL_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    Ok(cancel_file_in(&dir, config, &stamp))
}

/// Where a run publishes its progress, beside its stop file and named from the same run.
///
/// The same directory for the same reason: it is writable by construction, which the stop file
/// learned the hard way. Derived from the stop file rather than named separately so one run has
/// one identity and a supervisor cannot end up watching one run and stopping another.
pub fn progress_file_for(cancel_file: &Path) -> PathBuf {
    let mut name = cancel_file.file_name().unwrap_or_default().to_os_string();
    name.push(".progress");
    cancel_file.with_file_name(name)
}

/// Where a run's console output is captured, beside its stop and progress files.
///
/// A window has no terminal to inherit, so without this the CLI's stdout and stderr go nowhere and
/// a failed run reaches the operator as an exit code with no sentence attached. The messages exist
/// — "source directory does not exist: examples/demo-data" is exactly what a person needs — and
/// discarding them is what made the console unhelpful at the only moment it mattered.
pub fn output_file_for(cancel_file: &Path) -> PathBuf {
    let mut name = cancel_file.file_name().unwrap_or_default().to_os_string();
    name.push(".output");
    cancel_file.with_file_name(name)
}

/// The complete argument list for running one configuration file.
///
/// Built from a fixed shape rather than from anything a caller passes: the only two values that
/// vary are the paths. That is what makes the prohibition above enforceable instead of merely
/// documented — there is no parameter through which another flag could arrive.
pub fn run_arguments(config: &Path, cancel_file: &Path) -> Vec<String> {
    vec![
        "--config".to_string(),
        config.display().to_string(),
        "--cancel-file".to_string(),
        cancel_file.display().to_string(),
        "--progress-file".to_string(),
        progress_file_for(cancel_file).display().to_string(),
    ]
}

/// The complete argument list for resuming from a checkpoint (F31, closes the resume half of
/// Onda 3 in `PIANO_GUI.md`). Same fixed shape and same reasoning as [`run_arguments`] — the only
/// difference is `--resume-from` in place of `--config`, because `main.rs` treats the two as
/// mutually exclusive top-level modes (`--resume-from` never goes through `run_jobs`, so a
/// resumed run is always single-job, whatever `[[jobs]]` the original interrupted config had).
pub fn resume_arguments(checkpoint: &Path, cancel_file: &Path) -> Vec<String> {
    vec![
        "--resume-from".to_string(),
        checkpoint.display().to_string(),
        "--cancel-file".to_string(),
        cancel_file.display().to_string(),
        "--progress-file".to_string(),
        progress_file_for(cancel_file).display().to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The F61 prohibitions, as a test rather than a paragraph. A rule that is only written down
    /// is a rule someone eventually forgets.
    #[test]
    fn the_argument_list_cannot_carry_a_destructive_flag() {
        let args = run_arguments(Path::new("jobs.toml"), Path::new(".jobs.stop-1"));
        let joined = args.join(" ");

        for forbidden in [
            "--force-purge",
            "--mirror",
            "--install-service",
            "--uninstall-service",
            "--install-schedule",
            "--uninstall-schedule",
            "--keep-generations",
        ] {
            assert!(
                !joined.contains(forbidden),
                "{forbidden} must never reach a run started by a supervisor: {joined}"
            );
        }
        assert_eq!(args.len(), 6, "and nothing else may be added silently");
    }

    /// Same prohibition, same reasoning, for the resume path — `--resume-from` is a second entry
    /// point into the CLI and needs its own test rather than trusting that `run_arguments`'s test
    /// somehow covers it too.
    #[test]
    fn the_resume_argument_list_cannot_carry_a_destructive_flag() {
        let args = resume_arguments(Path::new("run.checkpoint.json"), Path::new(".jobs.stop-1"));
        let joined = args.join(" ");

        for forbidden in [
            "--force-purge",
            "--mirror",
            "--install-service",
            "--uninstall-service",
            "--install-schedule",
            "--uninstall-schedule",
            "--keep-generations",
            "--config",
        ] {
            assert!(
                !joined.contains(forbidden),
                "{forbidden} must never reach a run started by a supervisor: {joined}"
            );
        }
        assert!(joined.starts_with("--resume-from run.checkpoint.json"));
        assert_eq!(args.len(), 6, "and nothing else may be added silently");
    }

    /// One run, one identity: watching one run's progress while holding another's stop file would
    /// let a supervisor report on one job and stop a different one.
    #[test]
    fn the_progress_file_belongs_to_the_same_run_as_the_stop_file() {
        let cancel = Path::new("C:/temp/rustcopy/jobs.stop-abc");
        let progress = progress_file_for(cancel);

        assert_eq!(progress.parent(), cancel.parent());
        assert!(progress
            .file_name()
            .expect("named")
            .to_string_lossy()
            .starts_with("jobs.stop-abc"));
        assert_ne!(progress, cancel.to_path_buf());
    }

    /// A missing CLI must say which path it looked at. "Not found" without the path sends an
    /// operator hunting through an installation directory they cannot see from a window.
    #[test]
    fn a_missing_cli_names_the_path_it_looked_for() {
        let dir = tempfile::tempdir().expect("tempdir");
        let supervisor = dir.path().join("rustcopy-gui.exe");

        let error = cli_beside(&supervisor).expect_err("nothing is installed there");
        let message = error.to_string();
        assert!(message.contains(CLI_BINARY), "got {message}");
    }

    #[test]
    fn the_cli_is_found_beside_the_supervisor() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(CLI_BINARY), b"").expect("write");

        let found = cli_beside(&dir.path().join("rustcopy-gui.exe")).expect("found");
        assert_eq!(found, dir.path().join(CLI_BINARY));
    }

    /// Two runs must not be able to stop each other.
    #[test]
    fn each_run_gets_its_own_stop_file() {
        let config = Path::new("C:/backup/jobs.toml");
        let dir = Path::new("C:/temp/rustcopy");

        let first = cancel_file_in(dir, config, "a");
        let second = cancel_file_in(dir, config, "b");

        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(dir));
        assert!(first
            .file_name()
            .expect("named")
            .to_string_lossy()
            .starts_with("jobs.stop-"));
    }

    /// A configuration can live somewhere readable and not writable. Putting the stop file beside
    /// it — the first design — meant a run could start and then be impossible to stop, which is
    /// the one thing this mechanism exists to make possible.
    #[test]
    fn the_stop_file_does_not_live_beside_the_configuration() {
        let config = Path::new("C:/Program Files/rustcopy/jobs.toml");
        let path = cancel_file_for_now(config).expect("the directory is created");

        assert_ne!(path.parent(), config.parent());
        assert_eq!(path.parent(), Some(cancel_file_dir().as_path()));
        assert!(
            cancel_file_dir().is_dir(),
            "and it exists before a run starts"
        );
    }

    /// A millisecond is not unique. Two windows starting the same configuration inside one would
    /// otherwise pick the same file, and a Stop in either would interrupt both runs.
    #[test]
    fn two_runs_started_in_the_same_instant_do_not_share_a_stop_file() {
        let config = Path::new("jobs.toml");

        let first = cancel_file_for_now(config).expect("created");
        let second = cancel_file_for_now(config).expect("created");

        assert_ne!(first, second);
    }

    /// Exit 4 is not a failed copy, and a supervisor that renders it as one would send someone
    /// looking for data that is in fact present.
    #[test]
    fn a_verification_failure_reads_differently_from_a_failed_transfer() {
        assert_ne!(
            exit_code_meaning(EXIT_INTEGRITY_FAILED),
            exit_code_meaning(EXIT_INGESTION_PROBLEM)
        );
        assert!(exit_code_meaning(EXIT_INTEGRITY_FAILED).contains("copiato"));
        assert_eq!(exit_code_meaning(99), "sconosciuto");
    }

    /// F65: exit 6 must read as "not enough disk", not as the generic usage-error message a
    /// scheduler would otherwise have to assume for any unrecognised non-zero code.
    #[test]
    fn insufficient_disk_space_has_its_own_meaning() {
        assert_ne!(
            exit_code_meaning(EXIT_INSUFFICIENT_DISK_SPACE),
            exit_code_meaning(EXIT_UNRECOVERABLE)
        );
    }
}
