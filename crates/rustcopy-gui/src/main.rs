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
//! There is no command that copies, deletes, purges, schedules or installs. The prohibitions kept
//! in ROADMAP F61 — never expose `--force-purge`, unattended `--mirror`, retention purges or
//! service installation to an automated caller — apply to this surface identically.
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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_jobs,
            read_settings,
            read_report,
            read_report_page,
            read_history,
            read_advice,
            read_job_drafts,
            suggest_proposal_path,
            write_proposal
        ])
        .run(tauri::generate_context!())
        .expect("error while running the rustcopy console");
}
