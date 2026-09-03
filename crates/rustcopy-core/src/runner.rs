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

/// Where the stop file for one run goes.
///
/// Beside the configuration and named after this run, so two runs cannot stop each other and a
/// file left behind by a previous one cannot stop this one — which the CLI refuses at startup
/// anyway, but a colliding name would turn that refusal into a puzzle rather than a message.
pub fn cancel_file_for(config: &Path, stamp: &str) -> PathBuf {
    let stem = config
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "rustcopy".to_string());
    config.with_file_name(format!(".{stem}.stop-{stamp}"))
}

/// [`cancel_file_for`] with this moment's stamp.
///
/// Here rather than in the caller so a supervisor needs no clock of its own: the desktop console
/// would otherwise take a `chrono` dependency to format one string, and every dependency the GUI
/// crate does not need is one it cannot drag into the workspace's lockfile.
pub fn cancel_file_for_now(config: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f").to_string();
    cancel_file_for(config, &stamp)
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
        assert_eq!(args.len(), 4, "and nothing else may be added silently");
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

    /// Two runs must not be able to stop each other, and the name must not collide with a file the
    /// CLI would then refuse to start against.
    #[test]
    fn each_run_gets_its_own_stop_file_beside_its_configuration() {
        let config = Path::new("C:/backup/jobs.toml");

        let first = cancel_file_for(config, "20260903-0900");
        let second = cancel_file_for(config, "20260903-0901");

        assert_ne!(first, second);
        assert_eq!(first.parent(), config.parent());
        assert!(first
            .file_name()
            .expect("named")
            .to_string_lossy()
            .starts_with(".jobs.stop-"));
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
}
