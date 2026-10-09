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
}

impl Running for ActiveRun {
    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

/// How a run ended. The reports it left are found and read by the core's sessions log.
pub struct Finished {
    pub exit_code: i32,
}

impl ActiveRun {
    /// Starts one configuration file as a child process.
    pub fn spawn(config: &Path) -> Result<Self, String> {
        // The child runs in the configuration's folder, so a relative `--config` would resolve
        // against itself: make it absolute first (G05).
        let config = std::path::absolute(config).map_err(|e| e.to_string())?;
        Self::launch(&config, runner::run_arguments)
    }

    /// Starts one configuration with its generation backups forced to full (`--force-full`): the
    /// fixed form in `runner::force_full_arguments`.
    pub fn spawn_force_full(config: &Path) -> Result<Self, String> {
        let config = std::path::absolute(config).map_err(|e| e.to_string())?;
        Self::launch(&config, runner::force_full_arguments)
    }

    /// Resumes an interrupted run from its checkpoint (`--resume-from`): the fixed form in
    /// `runner::resume_arguments`, run from the checkpoint's own folder like the original was.
    pub fn spawn_resume(checkpoint: &Path) -> Result<Self, String> {
        let checkpoint = std::path::absolute(checkpoint).map_err(|e| e.to_string())?;
        Self::launch(&checkpoint, runner::resume_arguments)
    }

    /// `anchor` is the file the run is about (a configuration or a checkpoint): its folder is the
    /// child's working directory.
    fn launch(anchor: &Path, arguments: fn(&Path, &Path) -> Vec<String>) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // G03: beside this executable, never on PATH.
        let cli = runner::cli_beside(&exe).map_err(|e| e.to_string())?;
        let cancel = runner::cancel_file_for_now(anchor).map_err(|e| e.to_string())?;
        // A stop file left by a crashed run would make the CLI refuse to start.
        let _ = std::fs::remove_file(&cancel);

        let args = arguments(anchor, &cancel);
        let capture = std::fs::File::create(runner::output_file_for(&cancel))
            .map_err(|e| format!("cannot capture the run's output: {e}"))?;
        let capture_err = capture.try_clone().map_err(|e| e.to_string())?;

        let mut command = Command::new(&cli);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(capture))
            .stderr(Stdio::from(capture_err));
        if let Some(parent) = anchor.parent().filter(|p| !p.as_os_str().is_empty()) {
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
        })
    }

    /// The command line's process id, the root of the tree a pause suspends.
    pub fn pid(&self) -> u32 {
        self.child.id()
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
        Some(Finished {
            exit_code: status.code().unwrap_or(-1),
        })
    }
}

/// Runs the CLI once with a ready-made argument list and waits for it, returning its exit code and the
/// last of what it printed. For short commands that are not copies (installing or removing a schedule):
/// the arguments come from the core, which has already decided whether they may exist at all.
pub fn run_cli_once(args: &[String]) -> Result<(i32, String), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let cli = runner::cli_beside(&exe).map_err(|e| e.to_string())?;
    let mut command = Command::new(&cli);
    command.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command
        .output()
        .map_err(|e| format!("cannot start {}: {e}", cli.display()))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let last: Vec<&str> = text.lines().rev().take(6).collect();
    let tail = last.into_iter().rev().collect::<Vec<_>>().join("\n");
    Ok((output.status.code().unwrap_or(-1), tail))
}
