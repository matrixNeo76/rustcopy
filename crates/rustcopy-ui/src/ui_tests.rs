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
        kind: "Copia semplice".into(),
        when: "Non pianificata".into(),
        suspended: false,
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
    for page in 0..=14 {
        ui.set_page(page);
        let labels = all_labels(&ui);
        for always in ["Copia al volo", "Attività", "Aiuto"] {
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

    // A plain saved task: nothing to force, a single job, kept by the console.
    show_tasks(&ui, vec![task("a", false, 1, true)]);
    for absent in ["Forza completo", "Job", "Togli"] {
        assert!(!has(&ui, absent), "«{absent}");
    }
    assert!(has(&ui, "Pianifica"), "scheduling is always on the row");

    // Force-full needs a generation backup in the task; several jobs add the per-job page; a task only
    // remembered (not saved by the console) can be taken off the list.
    show_tasks(&ui, vec![task("a", true, 3, false)]);
    assert!(has(&ui, "Forza completo") && has(&ui, "Job") && has(&ui, "Togli"));
}

/// A control the safety level does not unlock is still on the row, greyed, with the reason beside it
/// and a way to the page that changes the level; unlocked, the reason and the way go away.
#[test]
fn a_locked_task_button_is_visible_greyed_and_says_why() {
    let ui = window();
    show_tasks(&ui, vec![task("a", true, 1, true)]);
    assert!(
        !has(&ui, "Cambia livello..."),
        "nothing locked, nothing to explain"
    );
    assert!(one(&ui, "Pianifica").accessible_enabled().unwrap_or(false));

    ui.set_lock_schedule("Richiede il livello Standard".into());
    ui.set_lock_force_full("Richiede il livello Standard".into());
    let schedule = one(&ui, "Pianifica");
    assert!(!schedule.accessible_enabled().unwrap_or(true), "greyed");
    assert!(!one(&ui, "Forza completo")
        .accessible_enabled()
        .unwrap_or(true));
    let text = all_labels(&ui).join("\n");
    assert!(
        text.contains("Pianifica: Richiede il livello Standard")
            || find(&ui, "Pianifica").iter().any(|e| e
                .accessible_description()
                .is_some_and(|d| d.contains("Standard"))),
        "the reason is on screen: {text}"
    );
    assert_eq!(
        schedule
            .accessible_description()
            .map(|d| d.to_string())
            .as_deref(),
        Some("Richiede il livello Standard")
    );

    let (calls, sink) = capture::<&'static str>();
    ui.on_show_safety(move || sink.borrow_mut().push("level"));
    press(&ui, "Cambia livello...");
    assert_eq!(*calls.borrow(), vec!["level"]);

    // Everything locked and every button present still fits the window.
    for label in [
        "Forza completo",
        "Pianifica",
        "Storico",
        "Modifica",
        "Proprietà",
        "Esegui",
        "Cambia livello...",
    ] {
        let button = one(&ui, label);
        let (x, w) = (button.absolute_position().x, button.size().width);
        assert!(x >= 0.0 && x + w <= WIDTH, "«{label}»: x={x} w={w}");
    }
}

/// The table has its column titles above the rows, and each row says its kind, when it runs and when
/// next; a suspended task says so instead of a time.
#[test]
fn the_tasks_table_has_columns_and_each_row_fills_them() {
    let ui = window();
    show_tasks(&ui, vec![]);
    assert!(!has(&ui, "Ultima volta"), "no rows, no column titles");

    let mut daily = task("notte", true, 1, true);
    daily.kind = "Incrementale".into();
    daily.when = "Ogni giorno alle 22:00".into();
    daily.scheduled = true;
    daily.next_run = "14/10/2026 22:00:00".into();
    let mut paused = task("vecchia", false, 1, true);
    paused.when = "Sospesa".into();
    paused.scheduled = true;
    paused.suspended = true;
    show_tasks(&ui, vec![daily, paused]);
    for title in ["Nome", "Tipo", "Quando", "Ultima volta", "Prossima"] {
        assert!(has(&ui, title), "column «{title}» is missing");
    }
    let text = all_labels(&ui).join("\n");
    for expected in [
        "Incrementale",
        "Ogni giorno alle 22:00",
        "14/10/2026 22:00:00",
        "Sospesa",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
}

/// The first thing the person sees is the tasks, not a copy to fill in.
#[test]
fn the_window_opens_on_the_tasks() {
    let ui = window();
    assert_eq!(ui.get_page(), 2);
}

/// The move choice is on the new-copy page whatever the level; below the one that unlocks it, it is
/// greyed and says which level that is.
#[test]
fn the_move_choice_is_visible_and_greyed_below_its_level() {
    let ui = window();
    ui.set_page(0);
    ui.set_can_move(false);
    ui.set_lock_move("Richiede il livello Standard".into());
    let choice = one(&ui, "Sposta: cancella gli originali dopo la verifica");
    assert!(!choice.accessible_enabled().unwrap_or(true));
    assert!(all_labels(&ui)
        .join("\n")
        .contains("Richiede il livello Standard"));
    ui.set_can_move(true);
    ui.set_lock_move("".into());
    assert!(one(&ui, "Sposta: cancella gli originali dopo la verifica")
        .accessible_enabled()
        .unwrap_or(false));
}

#[test]
fn the_task_buttons_call_the_right_callbacks_with_the_task_path() {
    let ui = window();
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
        find(ui, "Registro tecnico")
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

// ---------------------------------------------------------------- elevated schedule

const ELEVATED: &str = "Con privilegi elevati (per copiare file in uso)";

/// The choice to run elevated is always on the schedule page; below the Expert level it is greyed and
/// says which level unlocks it, so it cannot be ticked by mistake and does not look missing.
#[test]
fn the_elevated_schedule_box_is_greyed_below_the_level_that_unlocks_it() {
    let ui = window();
    ui.set_page(10);
    ui.set_can_elevated_schedule(false);
    ui.set_lock_elevated("Richiede il livello Esperto".into());
    let boxed = one(&ui, ELEVATED);
    assert!(
        !boxed.accessible_enabled().unwrap_or(true),
        "greyed below Expert"
    );
    assert!(all_labels(&ui)
        .join("\n")
        .contains("Richiede il livello Esperto"));
    ui.set_can_elevated_schedule(true);
    ui.set_lock_elevated("".into());
    assert!(
        one(&ui, ELEVATED).accessible_enabled().unwrap_or(false),
        "usable at Expert"
    );
}

/// Ticking the box sets the property the Rust side reads, and the box says what it will do (one prompt
/// from Windows, then no password) before the person confirms, through its accessible description.
#[test]
fn ticking_the_elevated_box_sets_the_flag_and_explains_itself() {
    let ui = window();
    ui.set_page(10);
    ui.set_can_elevated_schedule(true);
    assert!(!ui.get_sch_elevated(), "starts unticked");
    let description = one(&ui, ELEVATED)
        .accessible_description()
        .map(|d| d.to_string())
        .unwrap_or_default();
    assert!(
        description.contains("Windows chiede conferma una volta"),
        "{description}"
    );
    press(&ui, ELEVATED);
    assert!(ui.get_sch_elevated());
    press(&ui, ELEVATED);
    assert!(!ui.get_sch_elevated(), "ticking again clears it");
}

// ---------------------------------------------------------------- clarity of the pages

/// With no task yet the page says what a task is and how to make one, and offers the first step as a
/// button: an empty list with a faint sentence in a large blank page was the thing a person could not
/// make sense of.
#[test]
fn the_tasks_page_explains_itself_when_it_is_empty() {
    let ui = window();
    ui.set_page(2);
    ui.set_tasks(ModelRc::new(VecModel::from(Vec::<TaskRow>::new())));
    let labels = all_labels(&ui);
    let text = labels.join("\n");
    assert!(
        text.contains("Non hai ancora nessuna attività"),
        "{labels:?}"
    );
    assert!(
        text.contains("Salva come attività..."),
        "the steps name the button"
    );
    assert!(
        text.contains("Un'attività è una copia"),
        "it says what a task is"
    );
    // Three «Copia al volo»: the sidebar's, the page's own and the card's first step.
    assert_eq!(find(&ui, "Copia al volo").len(), 3);
    let (calls, sink) = capture::<&'static str>();
    ui.on_new_copy(move || sink.borrow_mut().push("new"));
    find(&ui, "Copia al volo")
        .last()
        .expect("the card's button")
        .invoke_accessible_default_action();
    assert_eq!(*calls.borrow(), vec!["new"]);
}

/// Once there is a task the explanation steps aside and the list is shown.
#[test]
fn the_empty_explanation_goes_away_once_there_is_a_task() {
    let ui = window();
    ui.set_page(2);
    show_tasks(&ui, vec![task("notte", false, 1, true)]);
    assert!(!all_labels(&ui)
        .join("\n")
        .contains("Non hai ancora nessuna attività"));
    assert!(has(&ui, "Esegui"));
}

/// The tasks page groups what it offers: a file you already have, and keys and passwords, each with
/// its own sentence; the old page had three buttons in a row with two unrelated sentences above them.
#[test]
fn the_tasks_page_separates_files_from_keys_and_passwords() {
    let ui = window();
    ui.set_page(2);
    let labels = all_labels(&ui);
    for expected in [
        "Aggiungi un file di configurazione...",
        "Apri un report...",
        "Chiavi e password...",
    ] {
        assert!(
            labels.iter().any(|l| l == expected),
            "{expected}: {labels:?}"
        );
    }
}

/// The log page says what the log is for before showing it.
#[test]
fn the_log_page_says_what_a_log_is() {
    let ui = window();
    ui.set_page(12);
    assert!(all_labels(&ui)
        .join("\n")
        .contains("È il diario che rustcopy scrive mentre lavora"));
}

/// The safety page describes the level picked in full, says what it cannot do, and does not promise a
/// mirror from the console (which no level offers).
#[test]
fn the_safety_page_describes_the_level_in_full() {
    let ui = window();
    ui.set_page(9);
    ui.set_safety_unlocks(
        robocopy_ingest::safety::SafetyLevel::Standard
            .describe()
            .into(),
    );
    let text = all_labels(&ui).join("\n");
    assert!(text.contains("«Sposta»"), "{text}");
    assert!(text.contains("Non puoi ancora"), "{text}");
    ui.set_safety_unlocks(
        robocopy_ingest::safety::SafetyLevel::Expert
            .describe()
            .into(),
    );
    let text = all_labels(&ui).join("\n");
    assert!(text.contains("privilegi elevati"), "{text}");
    assert!(!text.contains("mirror e pulizie presidiati"));
}

/// The technical details are tall enough to read, and the page scrolls as a whole instead of squeezing
/// them into what is left under the buttons.
#[test]
fn the_technical_details_are_not_squeezed() {
    let ui = window();
    ui.set_page(1);
    ui.set_d_has_reports(true);
    ui.set_d_state(0);
    ui.set_d_show_technical(true);
    let rows: Vec<crate::generated::DetailRow> = (0..40)
        .map(|i| crate::generated::DetailRow {
            kind: 1,
            label: format!("voce {i}").into(),
            value: "x".into(),
        })
        .collect();
    ui.set_d_rows(ModelRc::new(VecModel::from(rows)));
    assert!(
        !has(&ui, "Mostra i dettagli tecnici"),
        "when open, the button reads «Nascondi»"
    );
    let hide = one(&ui, "Nascondi i dettagli tecnici");
    // Its button stays inside the window even with the table open: the table sits below it and the page
    // scrolls, instead of the button being pushed away or the table squeezed.
    assert!(hide.absolute_position().y < 700.0);
}

/// Not a check: writes what the pages look like to `RUSTCOPY_SNAPSHOT_DIR` as raw RGBA, for looking at.
/// Run alone (`-- --ignored snapshot_pages`): it installs its own software-rendering platform, which
/// cannot coexist with the testing backend the other tests use in the same process.
#[test]
#[ignore]
fn snapshot_pages() {
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, WindowAdapter};

    let Some(dir) = std::env::var_os("RUSTCOPY_SNAPSHOT_DIR") else {
        return;
    };
    thread_local! {
        static WINDOW: Rc<MinimalSoftwareWindow> = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    }
    struct Headless;
    impl Platform for Headless {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(WINDOW.with(Clone::clone))
        }
    }
    slint::platform::set_platform(Box::new(Headless)).expect("platform");
    let window = WINDOW.with(Clone::clone);
    window.set_size(slint::PhysicalSize::new(1040, 780));
    let ui = AppWindow::new().expect("the interface builds");
    let dump = |ui: &AppWindow, name: &str| {
        let _ = ui;
        slint::platform::update_timers_and_animations();
        let mut pixels = vec![slint::Rgb8Pixel::new(0, 0, 0); 1040 * 780];
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 1040);
        });
        let mut bytes = b"P6
1040 780
255
"
        .to_vec();
        for p in &pixels {
            bytes.extend_from_slice(&[p.r, p.g, p.b]);
        }
        std::fs::write(
            std::path::Path::new(&dir).join(format!("{name}.ppm")),
            bytes,
        )
        .expect("write");
    };
    ui.set_page(2);
    ui.set_tasks(ModelRc::new(VecModel::from(Vec::<TaskRow>::new())));
    dump(&ui, "tasks-empty");

    let row = |name: &str,
               folders: &str,
               kind: &str,
               when: &str,
               next: &str,
               state: i32,
               last: &str,
               generations: bool| TaskRow {
        path: format!("C:/t/{name}.toml").into(),
        name: name.into(),
        folders: folders.into(),
        last_state: state,
        last_when: last.into(),
        verify: "".into(),
        jobs: 1,
        saved: true,
        scheduled: when != "Non pianificata",
        next_run: next.into(),
        kind: kind.into(),
        when: when.into(),
        suspended: when == "Sospesa",
        generations,
    };
    ui.set_tasks(ModelRc::new(VecModel::from(vec![
        row(
            "Documenti",
            "Documenti  →  \\\\NAS\\backup",
            "Incrementale",
            "Ogni giorno alle 22:00",
            "14/10/2026 22:00:00",
            0,
            "ieri 22:00",
            true,
        ),
        row(
            "Foto",
            "Foto  →  F:\\Foto",
            "Copia semplice",
            "Non pianificata",
            "",
            2,
            "3 giorni fa",
            false,
        ),
        row(
            "Archivio vecchio",
            "Archivio  →  \\\\NAS\\old",
            "Completa",
            "Sospesa",
            "",
            -1,
            "",
            true,
        ),
    ])));
    dump(&ui, "tasks-table");
    ui.set_lock_schedule("Richiede il livello Standard".into());
    ui.set_lock_force_full("Richiede il livello Standard".into());
    dump(&ui, "tasks-table-locked");

    ui.set_page(13);
    ui.set_wiz_name("documenti".into());
    ui.set_wiz_sources(ModelRc::new(VecModel::from(vec![
        slint::SharedString::from("C:\\Users\\io\\Documenti"),
        slint::SharedString::from("C:\\Users\\io\\Foto"),
    ])));
    dump(&ui, "wiz-1");
    ui.set_wiz_step(1);
    ui.set_wiz_dest("\\\\NAS\\backup\\Documenti".into());
    ui.set_wiz_kind(2);
    dump(&ui, "wiz-2");
    ui.set_wiz_step(2);
    ui.set_can_schedule(false);
    ui.set_lock_schedule("Richiede il livello Standard".into());
    dump(&ui, "wiz-3-locked");
    ui.set_can_schedule(true);
    ui.set_lock_schedule("".into());
    ui.set_wiz_schedule_on(true);
    dump(&ui, "wiz-3");
    ui.set_wiz_step(3);
    dump(&ui, "wiz-4");
    ui.set_wiz_step(4);
    ui.set_wiz_summary(
        "Nome: documenti\nCosa copiare: 2 elementi\nDove: NAS\\backup\nTipo: Incrementale\nQuando: Ogni giorno alle 02:00\nVerifica dopo la copia: no\nLa prima esecuzione sarà una copia completa, come punto di partenza.\nMai: mirror, cancellazioni, comandi prima o dopo la copia."
            .into(),
    );
    dump(&ui, "wiz-5");

    ui.set_page(14);
    ui.set_mail_enabled(true);
    ui.set_mail_host("smtp.example.com".into());
    ui.set_mail_security(0);
    ui.set_mail_user("rustcopy@example.com".into());
    ui.set_mail_password_name("smtp-rustcopy".into());
    ui.set_mail_from("rustcopy@example.com".into());
    ui.set_mail_to("admin@example.com".into());
    ui.set_mail_file("C:\\Users\\io\\AppData\\Local\\rustcopy\\console\\notify-server.toml".into());
    ui.set_mail_server_text("Il server di notifica non risponde su 127.0.0.1:3000: perché le attività mandino la posta deve essere avviato (notify-server.exe --config <questo file>) e l'attività deve inviargli l'esito con --webhook-url http://127.0.0.1:3000/notify.".into());
    ui.set_mail_message("Messaggio di prova inviato a admin@example.com.".into());
    ui.set_mail_ok(true);
    dump(&ui, "mail");

    ui.set_page(4);
    ui.set_edit_tab(5);
    ui.set_can_edit_commands(true);
    let mut form = ui.get_form();
    form.name = "foto".into();
    form.pre_command = "net stop mydb".into();
    form.post_command = "net start mydb".into();
    ui.set_form(form);
    dump(&ui, "editor-commands");
    ui.set_edit_confirm_text("Stai per scrivere un comando che verrà eseguito con i tuoi diritti a ogni esecuzione di questa attività, anche in automatico e senza nessuno davanti. Controlla il testo:\n  • «foto» - Prima della copia: net stop mydb\n  • «foto» - Dopo la copia: net start mydb\nViene scritto in una proposta accanto al file: il file in uso non cambia finché non lo sostituisci tu.".into());
    ui.set_edit_confirming(true);
    dump(&ui, "editor-confirm");
    ui.set_edit_confirming(false);
    ui.set_can_edit_commands(false);
    ui.set_lock_commands("Richiede il livello Esperto".into());
    dump(&ui, "editor-locked");

    let rows: Vec<crate::generated::HelpRow> = crate::help::entries()
        .into_iter()
        .map(|e| crate::generated::HelpRow {
            heading: e.heading,
            term: e.term.into(),
            text: e.text.into(),
        })
        .collect();
    ui.set_help_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_page(8);
    dump(&ui, "help");

    ui.set_page(9);
    ui.set_safety_unlocks(
        robocopy_ingest::safety::SafetyLevel::Expert
            .describe()
            .into(),
    );
    ui.set_safety_level_name("Prudente".into());
    ui.set_safety_pick(2);
    dump(&ui, "safety-expert");
    ui.set_safety_unlocks(
        robocopy_ingest::safety::SafetyLevel::Standard
            .describe()
            .into(),
    );
    ui.set_safety_pick(1);
    dump(&ui, "safety-standard");

    ui.set_page(12);
    ui.set_log_path(
        "C:/Users/x/AppData/Local/rustcopy/console/sessions/1/robocopy_ingest.log".into(),
    );
    ui.set_log_text(
        "2026-10-12T20:09:31 INFO ingestion starting source=C:/src dest=NAS/dest
2026-10-12T20:09:33 INFO source inventory complete files=31976"
            .into(),
    );
    dump(&ui, "log");

    ui.set_page(1);
    ui.set_d_where("claude-code → claude-code".into());
    ui.set_d_when("12/10 22:09".into());
    ui.set_d_has_numbers(true);
    ui.set_d_has_reports(true);
    ui.set_d_files("31976".into());
    ui.set_d_size("754,8 MB".into());
    ui.set_d_time("5 min 24 s".into());
    ui.set_d_speed("2,5 MB/s".into());
    ui.set_d_state(0);
    ui.set_d_show_technical(true);
    let detail: Vec<crate::generated::DetailRow> = (0..30)
        .map(|i| crate::generated::DetailRow {
            kind: if i % 10 == 0 { 0 } else { 1 },
            label: format!("voce {i}").into(),
            value: "valore".into(),
        })
        .collect();
    ui.set_d_rows(ModelRc::new(VecModel::from(detail)));
    dump(&ui, "detail-technical");
}

// ---------------------------------------------------------------- the new-task wizard

/// The tasks page offers the wizard, and the button starts it.
#[test]
fn the_tasks_page_offers_a_new_task() {
    let ui = window();
    ui.set_page(2);
    let (calls, sink) = capture::<&'static str>();
    ui.on_wiz_open(move || sink.borrow_mut().push("open"));
    press(&ui, "Nuova attività");
    assert_eq!(*calls.borrow(), vec!["open"]);
}

/// Each step shows its own title, the step counter and the buttons that belong to it: «Indietro» only after
/// the first step, «Avanti» until the last, then «Crea attività».
#[test]
fn the_wizard_shows_each_step_and_the_right_buttons() {
    let ui = window();
    ui.set_page(13);
    let expected = [
        (0, "Cosa copiare", "Passo 1 di 5"),
        (1, "Dove metterla", "Passo 2 di 5"),
        (2, "Quando", "Passo 3 di 5"),
        (3, "Come", "Passo 4 di 5"),
        (4, "Riepilogo", "Passo 5 di 5"),
    ];
    for (step, title, counter) in expected {
        ui.set_wiz_step(step);
        let labels = all_labels(&ui);
        assert!(
            labels.iter().any(|l| l.contains(counter)),
            "{counter}: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l == title),
            "step {step} has no heading «{title}»: {labels:?}"
        );
        assert!(has(&ui, "Indietro"), "the button exists on every step");
        assert_eq!(
            one(&ui, "Indietro").accessible_enabled().unwrap_or(false),
            step > 0,
            "step {step}"
        );
        assert_eq!(has(&ui, "Avanti"), step < 4, "step {step}");
        assert_eq!(has(&ui, "Crea attività"), step == 4, "step {step}");
    }
}

#[test]
fn the_wizard_buttons_call_their_callbacks() {
    let ui = window();
    ui.set_page(13);
    let (calls, sink) = capture::<&'static str>();
    let (a, b, c, d) = (sink.clone(), sink.clone(), sink.clone(), sink.clone());
    ui.on_wiz_next(move || a.borrow_mut().push("next"));
    ui.on_wiz_back(move || b.borrow_mut().push("back"));
    ui.on_wiz_create(move || c.borrow_mut().push("create"));
    ui.on_wiz_cancel(move || d.borrow_mut().push("cancel"));
    ui.set_wiz_step(1);
    press(&ui, "Avanti");
    press(&ui, "Indietro");
    ui.set_wiz_step(4);
    press(&ui, "Crea attività");
    press(&ui, "Annulla");
    assert_eq!(*calls.borrow(), vec!["next", "back", "create", "cancel"]);
}

/// A busy wizard cannot be moved: neither «Avanti» nor «Indietro» works while the task is being made.
#[test]
fn nothing_moves_while_the_task_is_being_created() {
    let ui = window();
    ui.set_page(13);
    ui.set_wiz_step(4);
    ui.set_wiz_busy(true);
    assert!(!one(&ui, "Crea attività")
        .accessible_enabled()
        .unwrap_or(true));
    assert!(!one(&ui, "Indietro").accessible_enabled().unwrap_or(true));
}

/// The first step lists what was chosen, lets one be taken off by position, and says when there is none.
#[test]
fn the_sources_can_be_added_and_taken_off() {
    let ui = window();
    ui.set_page(13);
    ui.set_wiz_step(0);
    assert!(all_labels(&ui)
        .join("\n")
        .contains("Nessuna cartella o file scelto."));
    ui.set_wiz_sources(ModelRc::new(VecModel::from(vec![
        slint::SharedString::from("C:/Documenti"),
        slint::SharedString::from("C:/Foto"),
    ])));
    let text = all_labels(&ui).join("\n");
    assert!(
        text.contains("C:/Documenti") && text.contains("C:/Foto"),
        "{text}"
    );
    let (calls, sink) = capture::<i32>();
    ui.on_wiz_remove_source(move |i| sink.borrow_mut().push(i));
    let removes = find(&ui, "Togli");
    assert_eq!(removes.len(), 2);
    removes[1].invoke_accessible_default_action();
    assert_eq!(
        *calls.borrow(),
        vec![1],
        "the second row's button names the second source"
    );
}

/// The kinds of copy are cards: pressing one chooses it, and the chosen one is marked.
#[test]
fn a_kind_of_copy_is_chosen_by_pressing_its_card() {
    let ui = window();
    ui.set_page(13);
    ui.set_wiz_step(1);
    assert_eq!(ui.get_wiz_kind(), 0);
    press(&ui, "Incrementale");
    assert_eq!(ui.get_wiz_kind(), 2);
    press(&ui, "Differenziale");
    assert_eq!(ui.get_wiz_kind(), 3);
    press(&ui, "Copia semplice");
    assert_eq!(ui.get_wiz_kind(), 0);
}

/// Scheduling is a choice that the safety level can lock: greyed with the reason, and it cannot be selected;
/// «Solo a mano» always can.
#[test]
fn the_scheduled_choice_is_greyed_below_its_level_and_cannot_be_picked() {
    let ui = window();
    ui.set_page(13);
    ui.set_wiz_step(2);
    ui.set_can_schedule(false);
    ui.set_lock_schedule("Richiede il livello Standard".into());
    let scheduled = one(&ui, "Pianificata");
    assert!(!scheduled.accessible_enabled().unwrap_or(true));
    assert_eq!(
        scheduled
            .accessible_description()
            .map(|d| d.to_string())
            .as_deref(),
        Some("Richiede il livello Standard")
    );
    scheduled.invoke_accessible_default_action();
    assert!(!ui.get_wiz_schedule_on(), "a locked choice does nothing");
    assert!(has(&ui, "Cambia livello..."));

    ui.set_can_schedule(true);
    ui.set_lock_schedule("".into());
    press(&ui, "Pianificata");
    assert!(ui.get_wiz_schedule_on());
    assert!(has(&ui, "Ogni giorno") || all_labels(&ui).iter().any(|l| l.contains("Quando")));
    press(&ui, "Solo a mano");
    assert!(!ui.get_wiz_schedule_on());
}

/// The wizard never offers what deletes: there is no retention field, whatever the kind of copy.
#[test]
fn the_wizard_never_offers_retention() {
    let ui = window();
    ui.set_page(13);
    ui.set_wiz_step(3);
    for kind in 0..=3 {
        ui.set_wiz_kind(kind);
        assert!(
            !all_labels(&ui)
                .iter()
                .any(|l| l.contains("Cicli da conservare")),
            "kind {kind}"
        );
    }
}

/// What is wrong is said on the step it belongs to, as a banner.
#[test]
fn a_problem_is_shown_on_the_wizard() {
    let ui = window();
    ui.set_page(13);
    ui.set_wiz_error("Dai un nome all'attività.".into());
    assert!(all_labels(&ui)
        .join("\n")
        .contains("Dai un nome all'attività."));
}

/// The last step shows the summary the core will turn into a file.
#[test]
fn the_last_step_shows_the_summary() {
    let ui = window();
    ui.set_page(13);
    ui.set_wiz_step(4);
    ui.set_wiz_summary("Nome: documenti\nTipo: Incrementale".into());
    let text = all_labels(&ui).join("\n");
    assert!(text.contains("Nome: documenti"), "{text}");
}

// ---------------------------------------------------------------- suspend and run all

/// A row offers to suspend an active task and to reactivate a suspended one, and says so with the path of
/// the task and the state asked for.
#[test]
fn a_task_row_suspends_or_reactivates_with_its_path() {
    let ui = window();
    let mut paused = task("vecchia", false, 1, true);
    paused.suspended = true;
    paused.when = "Sospesa".into();
    show_tasks(&ui, vec![task("notte", false, 1, true), paused]);
    let (calls, sink) = capture::<String>();
    ui.on_toggle_suspend_task(move |path, suspend| {
        sink.borrow_mut().push(format!("{path} {suspend}"));
    });
    assert_eq!(
        find(&ui, "Sospendi").len(),
        1,
        "only the active one can be suspended"
    );
    assert_eq!(
        find(&ui, "Riattiva").len(),
        1,
        "only the suspended one can be reactivated"
    );
    press(&ui, "Sospendi");
    press(&ui, "Riattiva");
    assert_eq!(
        *calls.borrow(),
        vec!["C:/t/notte.toml true", "C:/t/vecchia.toml false"]
    );
}

/// Suspending never needs a level, so its button is never greyed by one, and it sits inside the window
/// next to everything else on the row.
#[test]
fn the_suspend_button_is_never_locked_and_fits_the_window() {
    let ui = window();
    show_tasks(&ui, vec![task("foto", true, 3, false)]);
    ui.set_lock_schedule("Richiede il livello Standard".into());
    ui.set_lock_force_full("Richiede il livello Standard".into());
    let button = one(&ui, "Sospendi");
    assert!(
        button.accessible_enabled().unwrap_or(false),
        "a level does not lock it"
    );
    let (x, w) = (button.absolute_position().x, button.size().width);
    assert!(x >= 0.0 && x + w <= WIDTH, "x={x} w={w}");
}

/// «Esegui tutte» is there when there is something to run and nothing is running; it calls back; its result
/// is shown as a sentence.
#[test]
fn run_all_is_offered_only_when_it_can_start_and_reports_how_it_went() {
    let ui = window();
    ui.set_page(2);
    show_tasks(&ui, vec![]);
    assert!(
        !one(&ui, "Esegui tutte")
            .accessible_enabled()
            .unwrap_or(true),
        "no task, nothing to run"
    );
    show_tasks(
        &ui,
        vec![task("a", false, 1, true), task("b", false, 1, true)],
    );
    ui.set_running(false);
    assert!(one(&ui, "Esegui tutte")
        .accessible_enabled()
        .unwrap_or(false));
    let (calls, sink) = capture::<&'static str>();
    ui.on_run_all(move || sink.borrow_mut().push("all"));
    press(&ui, "Esegui tutte");
    assert_eq!(*calls.borrow(), vec!["all"]);

    ui.set_running(true);
    assert!(
        !one(&ui, "Esegui tutte")
            .accessible_enabled()
            .unwrap_or(true),
        "not while a copy runs"
    );
    ui.set_running(false);
    ui.set_run_all_message("Esegui tutte: 2 attività, 2 riuscite.".into());
    assert!(all_labels(&ui)
        .join("\n")
        .contains("Esegui tutte: 2 attività, 2 riuscite."));
}

// ---------------------------------------------------------------- the mail page

/// The tasks page leads to the mail settings, from the compact row as well as from the empty state.
#[test]
fn the_tasks_page_leads_to_the_mail_settings() {
    let (calls, sink) = capture::<&'static str>();
    let ui = window();
    ui.on_show_mail(move || sink.borrow_mut().push("mail"));
    show_tasks(&ui, vec![]);
    press(&ui, "Notifiche per posta...");
    show_tasks(&ui, vec![task("a", false, 1, true)]);
    press(&ui, "Notifiche per posta...");
    assert_eq!(*calls.borrow(), vec!["mail", "mail"]);
}

/// Every field has a label a screen reader can read, and the buttons call their callbacks.
#[test]
fn the_mail_page_has_its_fields_and_buttons() {
    let ui = window();
    ui.set_page(14);
    for label in [
        "Server di posta",
        "Porta (vuoto = quella usuale)",
        "Protezione della connessione",
        "Nome utente (vuoto se il server non lo chiede)",
        "Nome della password salvata",
        "Password (solo per salvarla o cambiarla)",
        "Mittente",
        "Destinatari (separati da virgola)",
        "Salva",
        "Invia una prova",
        "Torna alle attività",
    ] {
        assert!(
            has(&ui, label),
            "«{label}» is missing: {:?}",
            all_labels(&ui)
        );
    }
    let (calls, sink) = capture::<&'static str>();
    let (a, b) = (sink.clone(), sink.clone());
    ui.on_save_mail(move || a.borrow_mut().push("save"));
    ui.on_test_mail(move || b.borrow_mut().push("test"));
    press(&ui, "Salva");
    press(&ui, "Invia una prova");
    assert_eq!(*calls.borrow(), vec!["save", "test"]);
}

/// Nothing can be saved or tested twice at once while a test is going.
#[test]
fn the_mail_buttons_wait_while_a_test_is_going() {
    let ui = window();
    ui.set_page(14);
    ui.set_mail_busy(true);
    assert!(!one(&ui, "Salva").accessible_enabled().unwrap_or(true));
    assert!(!one(&ui, "Invia una prova")
        .accessible_enabled()
        .unwrap_or(true));
}

/// The password is typed into a masked box that is only for saving it; a password somebody wrote by hand in
/// the file in another form is shown as it is and the box for saving one is not offered.
#[test]
fn a_hand_written_password_spec_hides_the_box_for_a_new_one() {
    let ui = window();
    ui.set_page(14);
    assert!(has(&ui, "Password (solo per salvarla o cambiarla)"));
    ui.set_mail_password_other("env:SMTP_PASSWORD".into());
    assert!(!has(&ui, "Password (solo per salvarla o cambiarla)"));
    assert!(all_labels(&ui).join("\n").contains("env:SMTP_PASSWORD"));
}

/// What the server says about itself and the answer of the last action are on the page.
#[test]
fn the_mail_page_shows_the_server_state_and_the_last_answer() {
    let ui = window();
    ui.set_page(14);
    ui.set_mail_server_text("Il server di notifica non risponde su 127.0.0.1:3000.".into());
    ui.set_mail_message("Messaggio di prova inviato a admin@example.com.".into());
    ui.set_mail_ok(true);
    let text = all_labels(&ui).join("\n");
    assert!(text.contains("non risponde su 127.0.0.1:3000"), "{text}");
    assert!(
        text.contains("Messaggio di prova inviato a admin@example.com."),
        "{text}"
    );
}

// ---------------------------------------------------------------- commands in the editor

/// Below the Expert level the two command boxes are there but not writable, and say why; at Expert they are.
#[test]
fn the_command_boxes_are_locked_below_expert_and_say_which_level_unlocks_them() {
    let ui = window();
    ui.set_page(4);
    ui.set_edit_tab(5);
    ui.set_can_edit_commands(false);
    ui.set_lock_commands("Richiede il livello Esperto".into());
    for label in ["Comando prima della copia", "Comando dopo la copia"] {
        let field = one(&ui, label);
        assert!(
            !field.accessible_enabled().unwrap_or(true),
            "«{label}» must be locked"
        );
    }
    assert!(all_labels(&ui)
        .join("\n")
        .contains("Richiede il livello Esperto"));
    ui.set_can_edit_commands(true);
    for label in ["Comando prima della copia", "Comando dopo la copia"] {
        assert!(
            one(&ui, label).accessible_enabled().unwrap_or(false),
            "«{label}» at Expert"
        );
    }
}

/// A command can always be taken off, even where it cannot be written: its «Togli» is there only when there
/// is a command, is enabled whatever the level, and empties the box.
#[test]
fn a_command_can_always_be_taken_off() {
    let ui = window();
    ui.set_page(4);
    ui.set_edit_tab(5);
    ui.set_can_edit_commands(false);
    assert!(!has(&ui, "Togli"), "no command, nothing to take off");
    let mut form = ui.get_form();
    form.pre_command = "net stop db".into();
    ui.set_form(form);
    let button = one(&ui, "Togli");
    assert!(
        button.accessible_enabled().unwrap_or(false),
        "not locked by the level"
    );
    button.invoke_accessible_default_action();
    assert_eq!(ui.get_form().pre_command.as_str(), "", "the box is emptied");
}

/// A new command is not written on the first press: its exact text is shown with what it means, the button
/// becomes the confirmation, and «Annulla» goes back without writing.
#[test]
fn a_new_command_is_confirmed_before_it_is_written() {
    let ui = window();
    ui.set_page(4);
    let (calls, sink) = capture::<&'static str>();
    let (a, b, c) = (sink.clone(), sink.clone(), sink.clone());
    ui.on_edit_save(move || a.borrow_mut().push("save"));
    ui.on_edit_confirm(move || b.borrow_mut().push("confirm"));
    ui.on_edit_cancel_confirm(move || c.borrow_mut().push("cancel"));

    press(&ui, "Salva come proposta");
    assert_eq!(*calls.borrow(), vec!["save"]);

    ui.set_edit_confirm_text(
        "Stai per scrivere un comando che verrà eseguito con i tuoi diritti a ogni esecuzione.\n  • «foto» - Prima della copia: net stop db".into(),
    );
    ui.set_edit_confirming(true);
    let text = all_labels(&ui).join("\n");
    assert!(text.contains("net stop db"), "{text}");
    assert!(
        !has(&ui, "Salva come proposta"),
        "the save button became the confirmation"
    );
    press(&ui, "Confermo: scrivi la proposta");
    press(&ui, "Annulla");
    assert_eq!(*calls.borrow(), vec!["save", "confirm", "cancel"]);
}
