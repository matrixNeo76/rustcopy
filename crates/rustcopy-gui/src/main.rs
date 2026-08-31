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
//! # Read-only, deliberately
//!
//! There is no command that copies, deletes, purges, schedules or installs. A v1 with no write
//! path **cannot** damage a backup, which is the strongest guarantee available and the reason
//! §5.2 recommends starting here. The prohibitions preserved in ROADMAP F61 — never expose
//! `--force-purge`, unattended `--mirror`, retention purges or service installation to an
//! automated caller — apply to this surface identically.
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

use robocopy_ingest::gui_api::{self, JobSummary, ReportView};

/// Lists the jobs a TOML config declares.
///
/// Resolved exactly as `run_jobs` resolves them, including the positional `jobN` fallback for
/// unnamed jobs — a UI that invented its own labels would disagree with the reports on disk.
#[tauri::command]
fn list_jobs(config_path: String) -> Result<Vec<JobSummary>, String> {
    gui_api::list_jobs(&PathBuf::from(config_path)).map_err(|error| error.to_string())
}

/// Reads one JSON report, first page of its error lists.
#[tauri::command]
fn read_report(path: String) -> Result<ReportView, String> {
    gui_api::read_report(&PathBuf::from(path)).map_err(|error| error.to_string())
}

/// Reads one JSON report, taking `limit` error entries from `offset`.
///
/// `limit` is clamped by the library, not here: the boundary rule belongs where it is tested.
#[tauri::command]
fn read_report_page(path: String, offset: usize, limit: usize) -> Result<ReportView, String> {
    gui_api::read_report_page(&PathBuf::from(path), offset, limit)
        .map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_jobs,
            read_report,
            read_report_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running the rustcopy console");
}
