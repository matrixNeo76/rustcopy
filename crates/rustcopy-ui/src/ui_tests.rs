//! Tests of the `.slint` layer, run without opening a window (CATALOGO_COMPORTAMENTI_GUI.md L50).
//!
//! The pure modules next to this one are unit-tested, but whether a button *exists*, is *enabled*, is
//! *inside the window*, and *calls the right callback with the right argument* lived only in whoever
//! clicked it by hand: several of this console's real bugs (a task row whose main button was cut off, a
//! choice box that did not follow a new job, a stale answer shown next to a path it no longer described)
//! were found that way. These tests drive the compiled interface through Slint's headless testing
//! backend and find elements by their accessible label, which is also what a screen reader uses, so a
//! control that cannot be found here cannot be found by a person who cannot see either.
//!
//! What they do **not** cover: the Rust handlers in `main.rs` (they need a real window and the command
//! line) and how anything looks. Page numbers are the ones `app.slint` documents: 0 new copy, 1 detail,
//! 2 tasks, 3 properties, 4 editor, 5 history, 6 report, 7 credentials, 8 help, 9 safety, 10 schedule,
//! 11 jobs of a configuration, 12 log.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use i_slint_backend_testing::{ElementHandle, ElementRoot};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::generated::{AppWindow, QueueRow, TaskRow};

const WIDTH: f32 = 1040.0;

/// A window on the headless backend, as wide as the real one starts.
fn window() -> AppWindow {
    // One backend per test thread; cargo gives every test its own.
    i_slint_backend_testing::init_no_event_loop();
    let ui = AppWindow::new().expect("the interface builds");
    ui.window().set_size(slint::LogicalSize::new(WIDTH, 700.0));
    ui
}

fn find(ui: &AppWindow, label: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(ui, label).collect()
}

fn has(ui: &AppWindow, label: &str) -> bool {
    !find(ui, label).is_empty()
}

fn one(ui: &AppWindow, label: &str) -> ElementHandle {
    let mut found = find(ui, label);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one «{label}», found {}",
        found.len()
    );
    found.remove(0)
}

fn press(ui: &AppWindow, label: &str) {
    one(ui, label).invoke_accessible_default_action();
}

fn all_labels(ui: &AppWindow) -> Vec<String> {
    ui.root_element()
        .query_descendants()
        .match_predicate(|_| true)
        .find_all()
        .into_iter()
        .filter_map(|e| e.accessible_label().map(|l| l.to_string()))
        .collect()
}

fn task(name: &str, generations: bool, jobs: i32, saved: bool) -> TaskRow {
    TaskRow {
        path: format!("C:/t/{name}.toml").into(),
        name: name.into(),
        folders: "src  →  dst".into(),
        last_state: -1,
        last_when: "".into(),
        verify: "".into(),
        jobs,
        saved,
        scheduled: false,
        next_run: "".into(),
        generations,
    }
}

fn show_tasks(ui: &AppWindow, rows: Vec<TaskRow>) {
    ui.set_tasks(ModelRc::new(VecModel::from(rows)));
    ui.set_page(2);
}

/// What a callback was last called with.
type Log<T> = Rc<RefCell<Vec<T>>>;

fn capture<T: 'static>() -> (Log<T>, Log<T>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    (log.clone(), log)
}

// ---------------------------------------------------------------- the whole interface builds

#[test]
fn every_page_builds_and_keeps_the_sidebar() {
    let ui = window();
    for page in 0..=12 {
        ui.set_page(page);
        let labels = all_labels(&ui);
        for always in ["Nuova copia", "Attività", "Aiuto"] {
            assert!(
                labels.iter().any(|l| l == always),
                "page {page} lost «{always}»"
            );
        }
    }
}

#[test]
fn the_sidebar_buttons_call_their_callbacks() {
    let ui = window();
    let (calls, sink) = capture::<&'static str>();
    let (a, b) = (sink.clone(), sink.clone());
    ui.on_show_tasks(move || a.borrow_mut().push("tasks"));
    ui.on_show_help(move || b.borrow_mut().push("help"));
    press(&ui, "Attività");
    press(&ui, "Aiuto");
    assert_eq!(*calls.borrow(), vec!["tasks", "help"]);
}

#[test]
fn the_safety_level_is_shown_where_the_person_can_see_it() {
    let ui = window();
    ui.set_safety_level_name("Esperto".into());
    assert!(has(&ui, "Sicurezza: Esperto"), "{:?}", all_labels(&ui));
}

// ---------------------------------------------------------------- the tasks page

/// The main button of a task row was once cut off by the others (seven buttons beside the text in a
/// window of normal width). Every button of a row with every option on must lie inside the window.
#[test]
fn a_task_row_with_every_button_fits_the_window_and_esegui_is_among_them() {
    let ui = window();
    ui.set_can_schedule(true);
    ui.set_can_force_full(true);
    ui.set_running(false);
    show_tasks(&ui, vec![task("foto", true, 3, false)]);
    for label in [
        "Togli",
        "Forza completo",
        "Job",
        "Pianifica",
        "Storico",
        "Modifica",
        "Proprietà",
        "Esegui",
    ] {
        let button = one(&ui, label);
        let (x, w) = (button.absolute_position().x, button.size().width);
        assert!(w > 0.0, "«{label}» has no width");
        assert!(
            x >= 0.0 && x + w <= WIDTH,
            "«{label}» is outside the window: x={x} w={w}"
        );
    }
}

#[test]
fn the_optional_task_buttons_appear_only_when_they_apply() {
    let ui = window();

    // A plain saved task: no purge-style options, nothing to force, a single job.
    ui.set_can_schedule(false);
    ui.set_can_force_full(true);
    show_tasks(&ui, vec![task("a", false, 1, true)]);
    for absent in ["Forza completo", "Job", "Pianifica", "Togli"] {
        assert!(!has(&ui, absent), "«{absent}");
    }

    // Force-full needs both a generation backup in the task and the level for it.
    show_tasks(&ui, vec![task("a", true, 1, true)]);
    assert!(has(&ui, "Forza completo"));
    ui.set_can_force_full(false);
    assert!(!has(&ui, "Forza completo"), "the level is what allows it");

    // Several jobs add the per-job page; scheduling follows the level; a task that is only remembered
    // (not saved by the console) can be taken off the list.
    ui.set_can_schedule(true);
    show_tasks(&ui, vec![task("a", false, 3, false)]);
    assert!(has(&ui, "Job") && has(&ui, "Pianifica") && has(&ui, "Togli"));
}

#[test]
fn the_task_buttons_call_the_right_callbacks_with_the_task_path() {
    let ui = window();
    ui.set_can_schedule(true);
    ui.set_can_force_full(true);
    show_tasks(&ui, vec![task("foto", true, 2, false)]);
    let (calls, sink) = capture::<String>();
    let sinks: Vec<_> = (0..4).map(|_| sink.clone()).collect();
    let mut it = sinks.into_iter();
    let s = it.next().unwrap();
    ui.on_run_task(move |p| s.borrow_mut().push(format!("run {p}")));
    let s = it.next().unwrap();
    ui.on_force_full_task(move |p| s.borrow_mut().push(format!("force {p}")));
    let s = it.next().unwrap();
    ui.on_show_jobs(move |p| s.borrow_mut().push(format!("jobs {p}")));
    let s = it.next().unwrap();
    ui.on_remove_config(move |p| s.borrow_mut().push(format!("remove {p}")));
    press(&ui, "Esegui");
    press(&ui, "Forza completo");
    press(&ui, "Job");
    press(&ui, "Togli");
    assert_eq!(
        *calls.borrow(),
        vec![
            "run C:/t/foto.toml",
            "force C:/t/foto.toml",
            "jobs C:/t/foto.toml",
            "remove C:/t/foto.toml"
        ]
    );
}

#[test]
fn nothing_can_be_started_while_a_copy_runs() {
    let ui = window();
    show_tasks(&ui, vec![task("foto", false, 1, true)]);
    assert_eq!(one(&ui, "Esegui").accessible_enabled(), Some(true));
    ui.set_running(true);
    assert_eq!(one(&ui, "Esegui").accessible_enabled(), Some(false));
}

// ---------------------------------------------------------------- the running copy

#[test]
fn the_queue_of_a_multi_job_run_names_each_job_and_where_it_stands() {
    let ui = window();
    let rows = vec![
        QueueRow {
            name: "alfa".into(),
            state: 2,
            label: "concluso".into(),
        },
        QueueRow {
            name: "beta".into(),
            state: 1,
            label: "in corso".into(),
        },
        QueueRow {
            name: "gamma".into(),
            state: 0,
            label: "in attesa".into(),
        },
    ];
    ui.set_running(true);
    ui.set_page(0);
    ui.set_queue_rows(ModelRc::new(VecModel::from(rows.clone())));
    let labels = all_labels(&ui);
    for wanted in ["alfa", "beta", "gamma", "concluso", "in corso", "in attesa"] {
        assert!(
            labels.iter().any(|l| l == wanted),
            "missing «{wanted}»: {labels:?}"
        );
    }
    // A single job has no queue to draw.
    ui.set_queue_rows(ModelRc::new(VecModel::from(vec![rows[0].clone()])));
    assert!(!has(&ui, "alfa"));
}

#[test]
fn pause_and_stop_exist_only_while_something_runs_and_stop_calls_back() {
    let ui = window();
    ui.set_page(0);
    assert!(!has(&ui, "Pausa") && !has(&ui, "Ferma"));
    ui.set_running(true);
    assert!(has(&ui, "Pausa") && has(&ui, "Ferma"));
    let stopped = Rc::new(Cell::new(0));
    let s = stopped.clone();
    ui.on_stop_copy(move || s.set(s.get() + 1));
    press(&ui, "Ferma");
    assert_eq!(stopped.get(), 1);
}

// ---------------------------------------------------------------- the report and its problems

#[test]
fn problems_are_paged_and_the_buttons_say_where_there_is_more() {
    let ui = window();
    ui.set_page(6);
    // A report with no problems offers no paging and no export.
    ui.set_report_pageable(false);
    for absent in ["Precedenti", "Successivi", "Esporta CSV dei problemi..."] {
        assert!(!has(&ui, absent), "«{absent}»");
    }
    // The first of several pages: nothing before, more after.
    ui.set_report_pageable(true);
    ui.set_report_has_prev(false);
    ui.set_report_has_next(true);
    assert_eq!(one(&ui, "Precedenti").accessible_enabled(), Some(false));
    assert_eq!(one(&ui, "Successivi").accessible_enabled(), Some(true));
    let (calls, sink) = capture::<bool>();
    ui.on_report_page(move |forward| sink.borrow_mut().push(forward));
    ui.on_export_problems({
        let exported = Rc::new(Cell::new(false));
        move || exported.set(true)
    });
    press(&ui, "Successivi");
    ui.set_report_has_prev(true);
    press(&ui, "Precedenti");
    assert_eq!(*calls.borrow(), vec![true, false]);
}

// ---------------------------------------------------------------- resuming an interruption

fn detail_with(ui: &AppWindow, checkpoints: &[&str]) {
    ui.set_selected_id("20261012-095247-000".into());
    ui.set_d_state(3);
    ui.set_d_has_reports(true);
    ui.set_d_resume(checkpoints.first().copied().unwrap_or("").into());
    ui.set_d_checkpoints(ModelRc::new(VecModel::from(
        checkpoints
            .iter()
            .map(|c| (*c).into())
            .collect::<Vec<slint::SharedString>>(),
    )));
    ui.set_page(1);
}

#[test]
fn the_choice_of_interruption_appears_only_when_there_is_more_than_one() {
    let ui = window();
    detail_with(&ui, &["12/10 09:53 - fermata dalla console"]);
    assert!(has(&ui, "Riprendi"));
    assert!(!has(&ui, "Da quale interruzione riprendere"));
    detail_with(
        &ui,
        &[
            "12/10 09:53 - fermata",
            "11/10 10:15 - interrotta con Ctrl+C",
        ],
    );
    assert!(has(&ui, "Da quale interruzione riprendere"));
}

#[test]
fn resuming_passes_the_interruption_that_was_chosen() {
    let ui = window();
    detail_with(&ui, &["new", "old"]);
    ui.set_d_checkpoint_index(1);
    let (calls, sink) = capture::<(String, i32)>();
    ui.on_resume_session(move |id, index| sink.borrow_mut().push((id.to_string(), index)));
    press(&ui, "Riprendi");
    assert_eq!(
        *calls.borrow(),
        vec![("20261012-095247-000".to_string(), 1)]
    );
}

#[test]
fn nothing_to_resume_means_no_resume_button() {
    let ui = window();
    detail_with(&ui, &[]);
    assert!(!has(&ui, "Riprendi"));
}

// ---------------------------------------------------------------- the editor

#[test]
fn a_new_job_can_be_added_only_once_it_has_a_name() {
    let ui = window();
    ui.set_page(4);
    assert_eq!(one(&ui, "+ Nuovo job").accessible_enabled(), Some(false));
    one(&ui, "Nome di un nuovo job").set_accessible_value("archivio");
    assert_eq!(ui.get_new_job_name(), "archivio");
    assert_eq!(one(&ui, "+ Nuovo job").accessible_enabled(), Some(true));
    let (calls, sink) = capture::<String>();
    ui.on_edit_add_job(move |name| sink.borrow_mut().push(name.to_string()));
    press(&ui, "+ Nuovo job");
    assert_eq!(*calls.borrow(), vec!["archivio"]);
}

#[test]
fn each_folder_box_has_its_own_check_that_calls_back_with_which_one() {
    let ui = window();
    ui.set_page(4);
    let checks = find(&ui, "Verifica");
    assert_eq!(
        checks.len(),
        2,
        "one beside the source and one beside the destination"
    );
    let (calls, sink) = capture::<i32>();
    ui.on_edit_check(move |which| sink.borrow_mut().push(which));
    // Top to bottom: source first, then destination.
    let mut ordered = checks;
    ordered.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
    for check in &ordered {
        check.invoke_accessible_default_action();
    }
    assert_eq!(*calls.borrow(), vec![0, 1]);
}

/// An answer about a path must never be shown next to a different one: the text in the box moved on.
#[test]
fn a_check_answer_is_shown_only_while_the_box_still_holds_the_checked_path() {
    let ui = window();
    ui.set_page(4);
    let answer = "Contiene 3 file, 0 sottocartelle, 12 B.";
    ui.set_source_check(answer.into());
    ui.set_source_checked_for("C:/a".into());

    let mut form = ui.get_form();
    form.source = "C:/b".into();
    ui.set_form(form);
    assert!(
        !all_labels(&ui).iter().any(|l| l == answer),
        "an answer for another path"
    );

    let mut form = ui.get_form();
    form.source = "C:/a".into();
    ui.set_form(form);
    assert!(
        all_labels(&ui).iter().any(|l| l == answer),
        "the box holds the checked path again"
    );
}

// ---------------------------------------------------------------- the log

#[test]
fn the_log_page_says_so_when_there_is_no_log_yet_and_shows_it_when_there_is() {
    let ui = window();
    ui.set_page(12);
    assert!(has(&ui, "Torna al lavoro") && has(&ui, "Aggiorna"));
    let none = "Il registro non c'è ancora: si crea alla prima copia di questa configurazione.";
    let text_area = |ui: &AppWindow| {
        find(ui, "Registro")
            .into_iter()
            .filter_map(|e| e.accessible_value())
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
    };
    assert!(
        text_area(&ui).iter().any(|v| v == none),
        "{:?}",
        text_area(&ui)
    );
    ui.set_log_text("riga uno\nriga due".into());
    assert!(
        text_area(&ui).iter().any(|v| v.contains("riga due")),
        "{:?}",
        text_area(&ui)
    );
    let refreshed = Rc::new(Cell::new(false));
    let r = refreshed.clone();
    ui.on_refresh_log(move || r.set(true));
    press(&ui, "Aggiorna");
    assert!(refreshed.get());
}

// ---------------------------------------------------------------- errors

#[test]
fn an_error_is_shown_on_the_page_the_person_is_looking_at() {
    let ui = window();
    ui.set_error("Non è andata a buon fine.".into());
    for page in [0, 1, 2, 3, 5, 6, 11, 12] {
        ui.set_page(page);
        assert!(
            all_labels(&ui)
                .iter()
                .any(|l| l.contains("Non è andata a buon fine.")),
            "page {page} hides the error"
        );
    }
}

/// The editor reports through its own banner (it also says where a proposal was written), not the shared
/// one: a message it sets must show on its page.
#[test]
fn the_editor_shows_its_own_message() {
    let ui = window();
    ui.set_page(4);
    ui.set_edit_ok(false);
    ui.set_edit_message("Il job «x» ha bisogno di una cartella di origine.".into());
    assert!(
        all_labels(&ui)
            .iter()
            .any(|l| l.contains("ha bisogno di una cartella di origine")),
        "{:?}",
        all_labels(&ui)
    );
}
