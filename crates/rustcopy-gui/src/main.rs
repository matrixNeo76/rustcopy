//! Read-only desktop console for rustcopy (milestone 7.0.0, F53).
//!
//! # The one rule this file exists to keep
//!
//! Every command here is a **thin wrapper** over [`robocopy_ingest::gui_api`]: it converts
//! arguments, calls one function, and maps the error to a string the frontend can display. No
//! command decides anything — not whether a purge is safe, not what an exit code means, not
//! whether a mismatch is transient. That judgement lives in the library and is tested there
//! (`PIANO_GUI_TAURI.md` §4.1).
//!
//! If a command in this file ever grows a branch on backup semantics, the branch belongs in
//! `rustcopy-core` instead.
//!
//! # What this application may do
//!
//! No command copies, deletes, purges, schedules or installs *itself*. The prohibitions kept in
//! ROADMAP F61 — never expose `--force-purge`, unattended `--mirror`, retention purges or service
//! installation to an automated caller — apply to this surface identically.
//!
//! [`start_job`] does start a backup, by launching the same CLI a scheduled task would as a
//! separate process. It is not an exception to the rule above but an application of it: the
//! argument list comes from `robocopy_ingest::runner::run_arguments`, which builds a fixed shape
//! rather than forwarding anything, and a test there asserts no prohibited flag can appear. A
//! mirroring job needs nothing extra — its confirmation requires a terminal, and a child process
//! launched from a window has none, so it aborts itself with exit 3. This console can report a
//! purge; it can never authorise one.
//!
//! [`stop_job`] writes the file the run watches rather than killing the process, because the CLI's
//! stop path writes the checkpoint `--resume-from` reads and terminating it would skip exactly
//! that.
//!
//! Every command reads, with **one** exception: [`write_proposal`] (F54) writes a proposed
//! configuration to a new file. It cannot overwrite, cannot enable mirroring or retention, and
//! cannot remove a job by omission. Those rules live in `robocopy_ingest::job_editor`, where they
//! are enforced and tested; this file only carries the refusal back as a string. The running
//! configuration is never modified — the operator performs the substitution, which is what keeps
//! a bad write from taking out every job at once.
//!
//! # Why the CLI is unaffected
//!
//! This crate is a separate workspace member. `robocopy_ingest.exe` does not link Tauri, and
//! `ci.yml` carries a gate proving it (`cargo tree --locked -p rustcopy-cli` must not mention
//! tauri/wry/tao). A scheduled, unattended backup therefore runs exactly the same code whether or
//! not this application is installed.

// Windows: no console window behind the GUI in release builds. Kept off in debug so `println!`
// and panics stay visible while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Tauri reads dev-vs-production from the `custom-protocol` feature, not from the cargo profile:
// its own build script computes `dev = !custom-protocol`. A release build without it points the
// WebView at `devUrl` and ships an application that shows ERR_CONNECTION_REFUSED wherever the
// Vite dev server is not running -- which is every machine an installer reaches.
//
// That shipped once (2 Set 2026, F60): `cargo build`, `clippy` and 422 tests all passed on a
// binary that could not render its own interface, because none of them opens the window. This
// turns the mistake into a build failure instead of a defect discovered by a user, which is the
// only guard that does not depend on someone remembering to look.
#[cfg(all(not(debug_assertions), not(feature = "custom-protocol")))]
compile_error!(
    "a release build without the `custom-protocol` feature loads devUrl instead of the embedded frontend; build with default features"
);

use std::path::PathBuf;

use robocopy_ingest::advise::Advice;
use robocopy_ingest::gui_api::{self, HistoryView, JobSettings, JobSummary, ReportView};
use robocopy_ingest::job_editor::{self, JobDraft};

/// Runs a blocking library call off the IPC thread.
///
/// Every command below reads and parses files, and a synchronous `#[tauri::command]` runs on the
/// thread that services IPC — so a report on a slow network share would freeze the window until it
/// returned. This is the same discipline the CLI applies with `spawn_blocking_with_span` (D13):
/// filesystem work never runs on the thread that has to stay responsive.
async fn off_thread<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, robocopy_ingest::errors::IngestError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("the task panicked: {error}"))?
        .map_err(|error| error.to_string())
}

/// Lists the jobs a TOML config declares.
///
/// Resolved exactly as `run_jobs` resolves them, including the positional `jobN` fallback for
/// unnamed jobs — a UI that invented its own labels would disagree with the reports on disk.
#[tauri::command]
async fn list_jobs(config_path: String) -> Result<Vec<JobSummary>, String> {
    off_thread(move || gui_api::list_jobs(&PathBuf::from(config_path))).await
}

/// Reads every job's settings from a TOML config, resolved and grouped.
///
/// Read-only like the rest: it renders the file the CLI already reads. Which value wins for a job
/// and which settings carry a consequence are decided in the library, where they are tested — this
/// command only carries the result across (F55).
#[tauri::command]
async fn read_settings(config_path: String) -> Result<Vec<JobSettings>, String> {
    off_thread(move || gui_api::read_settings(&PathBuf::from(config_path))).await
}

/// Reads one JSON report, first page of its error lists.
#[tauri::command]
async fn read_report(path: String) -> Result<ReportView, String> {
    off_thread(move || gui_api::read_report(&PathBuf::from(path))).await
}

/// Reads one JSON report, taking `limit` error entries from `offset`.
///
/// `limit` is clamped by the library, not here: the boundary rule belongs where it is tested.
#[tauri::command]
async fn read_report_page(path: String, offset: usize, limit: usize) -> Result<ReportView, String> {
    off_thread(move || gui_api::read_report_page(&PathBuf::from(path), offset, limit)).await
}

/// Reads the run history stored beside `report_path`.
///
/// `limit` is clamped by the library, like the error pages: the boundary rule belongs where it is
/// tested, not in three separate command wrappers.
#[tauri::command]
async fn read_history(
    report_path: String,
    job_name: Option<String>,
    limit: usize,
) -> Result<HistoryView, String> {
    off_thread(move || {
        gui_api::read_history(&PathBuf::from(report_path), job_name.as_deref(), limit)
    })
    .await
}

/// Runs the deterministic advisor over that history.
///
/// No language model and no network: the suggestions are statistics over past runs, computed and
/// tested in `advise`. This command only carries them across.
#[tauri::command]
async fn read_advice(report_path: String, job_name: Option<String>) -> Result<Vec<Advice>, String> {
    off_thread(move || gui_api::read_advice(&PathBuf::from(report_path), job_name.as_deref())).await
}

/// Reads every job of a config file as an editable draft (F54).
#[tauri::command]
async fn read_job_drafts(config_path: String) -> Result<Vec<JobDraft>, String> {
    off_thread(move || job_editor::read_drafts(&PathBuf::from(config_path))).await
}

/// Suggests where the proposal should be written, beside the file it derives from.
#[tauri::command]
async fn suggest_proposal_path(config_path: String) -> Result<String, String> {
    Ok(
        job_editor::suggest_proposal_path_now(&PathBuf::from(config_path))
            .display()
            .to_string(),
    )
}

/// Writes a proposed configuration to a **new** file (F54).
///
/// The only command in this application that writes anything. It cannot overwrite, it cannot
/// enable mirroring or retention, and it cannot delete a job by omission — every one of those
/// rules is enforced and tested in `job_editor`, not here. This command converts arguments and
/// carries the refusal back as a string, like every other wrapper in this file.
#[tauri::command]
async fn write_proposal(
    config_path: Option<String>,
    drafts: Vec<JobDraft>,
    out_path: String,
) -> Result<(), String> {
    off_thread(move || {
        job_editor::propose_config_from_path(
            config_path.as_ref().map(PathBuf::from).as_deref(),
            &drafts,
            &PathBuf::from(out_path),
        )
    })
    .await
}

/// What a supervised run looks like from the window.
#[derive(Debug, Clone, serde::Serialize)]
struct RunStatus {
    running: bool,
    config_path: String,
    /// `None` while it runs; the process's exit code once it has finished.
    exit_code: Option<i32>,
    /// What that exit code means, decided by `robocopy_ingest::runner` and never here.
    meaning: Option<String>,
    /// Set once a stop has been requested and the run has not ended yet, so the window can say
    /// "stopping" rather than looking frozen while the checkpoint is written.
    stopping: bool,
    /// The run's latest published sample, when there is one.
    ///
    /// `None` before the first sample lands and again once the run ends, because the CLI removes
    /// the file: absent means "not running", which is a signal that needs no reasoning about
    /// staleness.
    progress: Option<robocopy_ingest::progress_file::ProgressSample>,
    /// The tail of what the run printed, shown when it fails.
    ///
    /// A tail rather than the whole file: a run that failed after copying for an hour can have
    /// produced a lot of output, and the operator needs the end of it — where the error is — not
    /// a transcript that has to cross the IPC boundary whole.
    output_tail: Option<String>,
    /// What that phase is, in words. Decided in the core: which phase a run is in is a fact about
    /// the backup, and naming it is not a rendering choice.
    phase_label: Option<String>,
}

/// The one run this window supervises at a time.
///
/// One, deliberately: two concurrent runs of the same job would race on that destination's
/// fast-verify cache and generation manifest, and offering a second Start would be offering a way
/// to corrupt them.
#[derive(Default)]
struct ActiveRun {
    child: Option<std::process::Child>,
    config_path: String,
    cancel_file: Option<PathBuf>,
    stopping: bool,
    last_exit: Option<i32>,
    /// Kept after the run ends, because the exit code alone does not say what went wrong.
    last_output: Option<String>,
}

type RunState = std::sync::Mutex<ActiveRun>;

/// Starts one configuration file as a child process.
///
/// The console never runs a backup itself: it starts the same CLI a scheduled task would, so a job
/// behaves identically whether a person launched it or Task Scheduler did. Which binary, which
/// arguments and where the stop file goes are decided in `robocopy_ingest::runner`, where the F61
/// prohibitions are enforced by a test rather than by this function remembering them.
#[tauri::command]
async fn start_job(
    config_path: String,
    state: tauri::State<'_, RunState>,
) -> Result<RunStatus, String> {
    // One lock, held from the idle check through the spawn and the assignment. Async Tauri
    // commands run concurrently, so releasing it in between made this a check-then-act: two
    // clicks in quick succession could both pass the check, and the second would overwrite the
    // first child — leaving a backup running that no window could stop, against a destination the
    // second run is also writing to.
    let mut active = state.lock().map_err(|_| "run state poisoned".to_string())?;

    if let Some(child) = active.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => active.last_exit = status.code(),
            Ok(None) => return Err("un backup è già in corso in questa finestra".to_string()),
            Err(error) => return Err(format!("cannot check the running job: {error}")),
        }
    }

    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let cli = robocopy_ingest::runner::cli_beside(&exe).map_err(|error| error.to_string())?;

    let config = PathBuf::from(&config_path);
    // Creates the directory too, so a run cannot start somewhere its stop file could not be
    // written later.
    let cancel =
        robocopy_ingest::runner::cancel_file_for_now(&config).map_err(|error| error.to_string())?;
    // A stop file left over would make the CLI refuse to start; clearing it here means a crashed
    // previous run cannot block the next one behind a message about a file nobody created on
    // purpose.
    let _ = std::fs::remove_file(&cancel);

    let args = robocopy_ingest::runner::run_arguments(&config, &cancel);

    // Captured, not discarded. A window inherits no terminal, so sending these to null left a
    // failed run reaching the operator as a bare exit code — the sentence explaining it existed
    // and was thrown away.
    let output_path = robocopy_ingest::runner::output_file_for(&cancel);
    let capture = std::fs::File::create(&output_path)
        .map_err(|error| format!("cannot capture the run's output: {error}"))?;
    let capture_err = capture
        .try_clone()
        .map_err(|error| format!("cannot capture the run's output: {error}"))?;

    let mut command = std::process::Command::new(&cli);
    command
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(capture))
        .stderr(std::process::Stdio::from(capture_err));

    // Relative paths in a configuration can only sensibly mean "relative to the configuration".
    // A person picking a file in a dialog has no notion of this window's working directory, and
    // inheriting it made `examples/demo-data` resolve against wherever the console happened to be
    // started from — which for an installed shortcut is not the repository.
    if let Some(parent) = config.parent().filter(|p| !p.as_os_str().is_empty()) {
        command.current_dir(parent);
    }

    // The CLI is a console-subsystem binary and this window has no console to lend it, so Windows
    // allocates a fresh one — a black terminal popping up in front of the application on every
    // Start. Found by clicking the button, not by reading the code.
    //
    // Suppressing it is not only cosmetic. The mirror confirmation appears when stdin *and* stdout
    // are terminals; leaving a console attached would put that question somewhere this window
    // cannot answer it, turning "a mirroring job stops itself" into "a mirroring job waits
    // forever behind a window nobody is looking at".
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", cli.display()))?;

    active.child = Some(child);
    active.config_path = config_path.clone();
    active.cancel_file = Some(cancel);
    active.stopping = false;
    active.last_exit = None;
    active.last_output = None;

    Ok(RunStatus {
        running: true,
        config_path,
        exit_code: None,
        meaning: None,
        stopping: false,
        progress: None,
        phase_label: None,
        output_tail: None,
    })
}

/// Asks the run to stop, by creating the file it watches.
///
/// Not by killing it. The CLI's stop path writes a checkpoint that `--resume-from` can read, and
/// terminating the process would skip exactly that — the property that makes an interruption worth
/// having. See `--cancel-file` for why a file rather than a console control event.
#[tauri::command]
async fn stop_job(state: tauri::State<'_, RunState>) -> Result<RunStatus, String> {
    let mut active = state.lock().map_err(|_| "run state poisoned".to_string())?;

    let cancel = active
        .cancel_file
        .clone()
        .ok_or_else(|| "nessun backup in corso da fermare".to_string())?;

    std::fs::write(&cancel, b"stop requested from the console")
        .map_err(|error| format!("cannot write the stop file {}: {error}", cancel.display()))?;
    active.stopping = true;

    Ok(RunStatus {
        running: true,
        config_path: active.config_path.clone(),
        exit_code: None,
        meaning: None,
        stopping: true,
        progress: read_progress(active.cancel_file.as_deref()),
        phase_label: None,
        output_tail: None,
    })
}

/// Reports the supervised run, polled by the window.
///
/// Polled rather than pushed: a backup emits nothing this process could subscribe to, and a status
/// the window asks for on its own timer cannot flood it — which is the shape §2.3 requires and the
/// one D18 showed matters at this scale.
#[tauri::command]
async fn run_status(state: tauri::State<'_, RunState>) -> Result<RunStatus, String> {
    let mut active = state.lock().map_err(|_| "run state poisoned".to_string())?;

    if let Some(child) = active.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                active.last_exit = status.code();
                active.child = None;
                active.stopping = false;
                // The run is over, so the file it watched has no reader left.
                // Read before the files go: this is the moment the operator most needs the
                // sentence the run printed, and cleaning up first would throw it away exactly
                // then.
                active.last_output = read_output_tail(active.cancel_file.as_deref());
                if let Some(path) = active.cancel_file.take() {
                    // The CLI removes its own progress file on a normal exit; this covers the run
                    // that crashed before it could.
                    let _ = std::fs::remove_file(robocopy_ingest::runner::progress_file_for(&path));
                    let _ = std::fs::remove_file(robocopy_ingest::runner::output_file_for(&path));
                    let _ = std::fs::remove_file(path);
                }
            }
            Ok(None) => {
                let progress = read_progress(active.cancel_file.as_deref());
                return Ok(RunStatus {
                    running: true,
                    config_path: active.config_path.clone(),
                    exit_code: None,
                    meaning: None,
                    stopping: active.stopping,
                    phase_label: progress
                        .as_ref()
                        .map(|sample| sample.phase.describe().to_string()),
                    progress,
                    output_tail: None,
                });
            }
            Err(error) => return Err(format!("cannot check the running job: {error}")),
        }
    }

    let exit_code = active.last_exit;
    let output_tail = active.last_output.clone();
    Ok(RunStatus {
        running: false,
        config_path: active.config_path.clone(),
        exit_code,
        // What the number means is backup semantics: read in the core, carried across here.
        meaning: exit_code
            .and_then(|code| u8::try_from(code).ok())
            .map(|code| robocopy_ingest::runner::exit_code_meaning(code).to_string()),
        stopping: false,
        progress: None,
        phase_label: None,
        output_tail,
    })
}

/// The last few lines the run printed, if any.
///
/// Bounded on both counts — lines and bytes — because this crosses the IPC boundary and a run that
/// logged for an hour must not be able to hand the window a transcript.
fn read_output_tail(cancel_file: Option<&std::path::Path>) -> Option<String> {
    const MAX_LINES: usize = 40;
    const MAX_BYTES: usize = 16 * 1024;

    let path = robocopy_ingest::runner::output_file_for(cancel_file?);
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    let tail: Vec<&str> = trimmed.lines().rev().take(MAX_LINES).collect();
    let mut text = tail.into_iter().rev().collect::<Vec<_>>().join(
        "
",
    );
    if text.len() > MAX_BYTES {
        text = text.split_off(text.len() - MAX_BYTES);
    }
    Some(text)
}

/// Reads the sample beside a run's stop file, if the run has published one yet.
fn read_progress(
    cancel_file: Option<&std::path::Path>,
) -> Option<robocopy_ingest::progress_file::ProgressSample> {
    let path = robocopy_ingest::runner::progress_file_for(cancel_file?);
    robocopy_ingest::progress_file::ProgressSample::read_from(&path)
}

fn main() {
    tauri::Builder::default()
        .manage(RunState::default())
        // Native pickers. The plugin reads nothing and writes nothing on its own: it returns the
        // path a person selected, which is strictly less error-prone than the text box it
        // replaces.
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_jobs,
            read_settings,
            read_report,
            read_report_page,
            read_history,
            read_advice,
            read_job_drafts,
            suggest_proposal_path,
            write_proposal,
            start_job,
            stop_job,
            run_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running the rustcopy console");
}
