//! SPIKE (PIANO_GUI_SLINT.md Fase 1) -- throwaway prototype of the Copia and Report screens.
//!
//! It exists to be measured (memory, start-up, readability, accessibility, Explorer drop), not to be
//! kept. Every decision about a backup stays in `robocopy_ingest::{runner, gui_api}`: this file only
//! collects a choice, asks the core to plan and write the throwaway configuration, starts the same
//! CLI a scheduled task would, and shows what the core reports.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use robocopy_ingest::{gui_api, progress_file::ProgressSample, runner};
use slint::winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor};
use slint::{ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

slint::include_modules!();

/// One run started from this window.
struct ActiveRun {
    child: Child,
    cancel_file: PathBuf,
    config: PathBuf,
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit]).replace('.', ",")
    }
}

fn human_duration(seconds: f64) -> String {
    let total = seconds.round() as u64;
    if total < 60 {
        format!("{total} s")
    } else {
        format!("{} min {:02} s", total / 60, total % 60)
    }
}

fn set_sources(ui: &AppWindow, sources: &[String]) {
    let model: Vec<SharedString> = sources
        .iter()
        .map(|s| SharedString::from(s.as_str()))
        .collect();
    ui.set_sources(ModelRc::new(VecModel::from(model)));
}

fn show_report(ui: &AppWindow, path: &Path) {
    match gui_api::read_report(path) {
        Ok(view) => {
            let ok = view.exit_code_is_success.unwrap_or(false);
            let outcome = if view.dry_run {
                "Simulazione: nessun file è stato copiato davvero.".to_string()
            } else if ok {
                "Copia riuscita.".to_string()
            } else {
                format!(
                    "Da controllare: {}",
                    view.exit_code_meaning
                        .clone()
                        .unwrap_or_else(|| "esito non disponibile".into())
                )
            };
            ui.set_r_outcome(outcome.into());
            ui.set_r_ok(ok && !view.dry_run);
            ui.set_r_files(view.files_copied.to_string().into());
            ui.set_r_size(human_bytes(view.bytes_copied).into());
            ui.set_r_time(human_duration(view.elapsed_seconds).into());
            ui.set_r_speed(format!("{:.0} MB/s", view.throughput_mbps).into());
            ui.set_r_where(format!("{}  ->  {}", view.source, view.dest).into());
            ui.set_has_report(true);
        }
        Err(error) => {
            ui.set_has_report(false);
            ui.set_error(format!("Report non leggibile: {error}").into());
        }
    }
}

/// Starts one configuration as a child process, the same way the Tauri console's `start_job` does.
fn spawn_run(config: &Path) -> Result<ActiveRun, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let cli = runner::cli_beside(&exe).map_err(|e| e.to_string())?;
    let config = std::path::absolute(config).map_err(|e| e.to_string())?;
    let cancel = runner::cancel_file_for_now(&config).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&cancel);
    let args = runner::run_arguments(&config, &cancel);
    let capture =
        std::fs::File::create(runner::output_file_for(&cancel)).map_err(|e| e.to_string())?;
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
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let child = command
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", cli.display()))?;
    Ok(ActiveRun {
        child,
        cancel_file: cancel,
        config,
    })
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let sources: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let active: Rc<RefCell<Option<ActiveRun>>> = Rc::new(RefCell::new(None));

    // Add folders: the native dialog blocks, so it runs on a worker thread and hands the result back.
    {
        let weak: Weak<AppWindow> = ui.as_weak();
        let sources = sources.clone();
        ui.on_add_folders(move || {
            let weak = weak.clone();
            let sources = sources.clone();
            std::thread::spawn(move || {
                let picked = rfd::FileDialog::new().pick_folders().unwrap_or_default();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        let mut list = sources.lock().expect("sources lock");
                        for path in picked {
                            let text = path.to_string_lossy().into_owned();
                            if !list.iter().any(|known| known.eq_ignore_ascii_case(&text)) {
                                list.push(text);
                            }
                        }
                        set_sources(&ui, &list);
                        ui.set_error("".into());
                    }
                });
            });
        });
    }
    {
        let weak = ui.as_weak();
        let sources = sources.clone();
        ui.on_remove_source(move |index| {
            if let Some(ui) = weak.upgrade() {
                let mut list = sources.lock().expect("sources lock");
                if (index as usize) < list.len() {
                    list.remove(index as usize);
                }
                set_sources(&ui, &list);
                ui.set_error("".into());
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_browse_dest(move || {
            let weak = weak.clone();
            std::thread::spawn(move || {
                let picked = rfd::FileDialog::new().pick_folder();
                let _ = slint::invoke_from_event_loop(move || {
                    if let (Some(ui), Some(path)) = (weak.upgrade(), picked) {
                        ui.set_dest(path.to_string_lossy().into_owned().into());
                        ui.set_error("".into());
                    }
                });
            });
        });
    }

    // M13: files dropped from Explorer arrive as a winit `DroppedFile` event, which Slint itself does
    // not expose to .slint code. Folders are added to the list; anything else is ignored.
    {
        let weak = ui.as_weak();
        let sources = sources.clone();
        ui.window().on_winit_window_event(move |_window, event| {
            if let WindowEvent::DroppedFile(path) = event {
                if path.is_dir() {
                    if let Some(ui) = weak.upgrade() {
                        let mut list = sources.lock().expect("sources lock");
                        let text = path.to_string_lossy().into_owned();
                        if !list.iter().any(|known| known.eq_ignore_ascii_case(&text)) {
                            list.push(text);
                        }
                        set_sources(&ui, &list);
                        ui.set_error("".into());
                        ui.set_page(0);
                    }
                }
            }
            EventResult::Propagate
        });
    }

    // Start: planning and every refusal are the core's (`runner::plan_copy`).
    {
        let weak = ui.as_weak();
        let sources = sources.clone();
        let active = active.clone();
        ui.on_start_copy(move || {
            let Some(ui) = weak.upgrade() else { return };
            let chosen: Vec<PathBuf> = sources
                .lock()
                .expect("sources lock")
                .iter()
                .map(PathBuf::from)
                .collect();
            let dest = ui.get_dest().to_string();
            let started = (|| -> Result<ActiveRun, String> {
                let items = runner::plan_copy(&chosen, Path::new(dest.trim()))
                    .map_err(|e| e.to_string())?;
                let config = runner::shell_drop_config_path().map_err(|e| e.to_string())?;
                runner::write_shell_drop_config(&items, &config).map_err(|e| e.to_string())?;
                spawn_run(&config)
            })();
            match started {
                Ok(run) => {
                    *active.borrow_mut() = Some(run);
                    ui.set_error("".into());
                    ui.set_has_report(false);
                    ui.set_fraction(-1.0);
                    ui.set_status_line("Avvio...".into());
                    ui.set_current_file("".into());
                    ui.set_running(true);
                }
                Err(message) => ui.set_error(message.into()),
            }
        });
    }
    {
        let active = active.clone();
        ui.on_stop_copy(move || {
            // Stopping means writing the stop file, never killing the process: the checkpoint is
            // written by the CLI's own cancel branch.
            if let Some(run) = active.borrow().as_ref() {
                let _ = std::fs::write(&run.cancel_file, b"");
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_open_report(move || {
            let weak = weak.clone();
            std::thread::spawn(move || {
                let picked = rfd::FileDialog::new()
                    .add_filter("Report JSON", &["json"])
                    .pick_file();
                let _ = slint::invoke_from_event_loop(move || {
                    if let (Some(ui), Some(path)) = (weak.upgrade(), picked) {
                        show_report(&ui, &path);
                    }
                });
            });
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_load_rows(move |count| {
            if let Some(ui) = weak.upgrade() {
                let rows: Vec<SharedString> = (0..count.max(0))
                    .map(|i| {
                        SharedString::from(format!(
                            "D:\\Dati\\Foto\\2026\\IMG_{i:06}.jpg    {} KB",
                            1200 + (i * 37) % 9000
                        ))
                    })
                    .collect();
                ui.set_rows_info(format!("{} righe", rows.len()).into());
                ui.set_rows(ModelRc::new(VecModel::from(rows)));
            }
        });
    }

    // Progress polling at 250 ms, on the UI thread: only a small file read and `try_wait`.
    let timer = Timer::default();
    {
        let weak = ui.as_weak();
        let active = active.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut guard = active.borrow_mut();
            let Some(run) = guard.as_mut() else { return };
            let progress = ProgressSample::read_from(&runner::progress_file_for(&run.cancel_file));
            if let Some(sample) = progress {
                ui.set_fraction(sample.fraction().map_or(-1.0, |f| f.min(0.99) as f32));
                ui.set_status_line(
                    format!(
                        "{} / {} file  -  {:.0} MB/s",
                        sample.files_done,
                        sample
                            .files_total
                            .map_or("?".to_string(), |t| t.to_string()),
                        sample.throughput_mbps
                    )
                    .into(),
                );
                ui.set_current_file(sample.current_file.unwrap_or_default().into());
            }
            if let Ok(Some(status)) = run.child.try_wait() {
                let report = run
                    .config
                    .parent()
                    .map(|p| p.join("robocopy_ingest_report.json"));
                let code = status.code().unwrap_or(-1);
                *guard = None;
                drop(guard);
                ui.set_running(false);
                if let Some(path) = report.filter(|p| p.exists()) {
                    show_report(&ui, &path);
                    ui.set_page(1);
                } else {
                    ui.set_error(
                        format!("La copia è terminata (codice {code}) senza un report leggibile.")
                            .into(),
                    );
                }
            }
        });
    }

    // Tray icon (M-tray): left click or "Apri" brings the window back; "Esci" quits.
    let tray = AppTray::new()?;
    {
        let weak = ui.as_weak();
        tray.on_show_window(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = ui.show();
            }
        });
        tray.on_quit(|| {
            let _ = slint::quit_event_loop();
        });
    }
    tray.show()?;
    if std::env::var_os("RUSTCOPY_UI_DARK").is_some() {
        ui.set_force_dark(true);
    }

    ui.show()?;
    slint::run_event_loop_until_quit()
}
