//! Starting and following one run (CATALOGO_COMPORTAMENTI_GUI.md G03-G07, E03-E05).
//!
//! The interface never runs a backup itself: it starts the same CLI a scheduled task would, through
//! the fixed argument form in `robocopy_ingest::runner`, with the child's output captured to a file and
//! its working directory set to the configuration's folder.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use robocopy_ingest::progress_file::ProgressSample;
use robocopy_ingest::runner;

use crate::state::Running;

/// One run started from this window.
pub struct ActiveRun {
    child: Child,
    cancel_file: PathBuf,
    config: PathBuf,
}

impl Running for ActiveRun {
    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

/// How a run ended, with the report it left (if any).
pub struct Finished {
    pub exit_code: i32,
    pub report: Option<PathBuf>,
}

impl ActiveRun {
    /// Starts one configuration file as a child process.
    pub fn spawn(config: &Path) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // G03: beside this executable, never on PATH.
        let cli = runner::cli_beside(&exe).map_err(|e| e.to_string())?;
        // The child runs in the configuration's folder, so a relative `--config` would resolve
        // against itself: make it absolute first (G05).
        let config = std::path::absolute(config).map_err(|e| e.to_string())?;
        let cancel = runner::cancel_file_for_now(&config).map_err(|e| e.to_string())?;
        // A stop file left by a crashed run would make the CLI refuse to start.
        let _ = std::fs::remove_file(&cancel);

        let args = runner::run_arguments(&config, &cancel);
        let capture = std::fs::File::create(runner::output_file_for(&cancel))
            .map_err(|e| format!("cannot capture the run's output: {e}"))?;
        let capture_err = capture.try_clone().map_err(|e| e.to_string())?;

        let mut command = Command::new(&cli);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(capture))
            .stderr(Stdio::from(capture_err));
        if let Some(parent) = config.parent().filter(|p| !p.as_os_str().is_empty()) {
            command.current_dir(parent);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // G06: a console-subsystem child launched from a windowed process would open a console.
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = command
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", cli.display()))?;
        Ok(Self {
            child,
            cancel_file: cancel,
            config,
        })
    }

    /// G04: stopping is writing the stop file, never killing the process: the CLI's own cancel branch
    /// writes the checkpoint a kill would skip.
    pub fn request_stop(&self) {
        let _ = std::fs::write(&self.cancel_file, b"");
    }

    /// The latest progress sample the child has published, if any.
    pub fn progress(&self) -> Option<ProgressSample> {
        ProgressSample::read_from(&runner::progress_file_for(&self.cancel_file))
    }

    /// `Some` once the child has exited.
    pub fn finished(&mut self) -> Option<Finished> {
        let status = self.child.try_wait().ok().flatten()?;
        let report = self
            .config
            .parent()
            .map(|dir| dir.join("robocopy_ingest_report.json"))
            .filter(|path| path.exists());
        Some(Finished {
            exit_code: status.code().unwrap_or(-1),
            report,
        })
    }
}
