//! What the tasks table says in its "Tipo" and "Quando" columns (PIANO_GUI_ATTIVITA.md 3.1), as pure
//! functions: the numbers and the decisions come from `gui_api` (`list_jobs`, `schedule_details`); this
//! only turns them into the words on screen, so it is tested without a window or Task Scheduler.

use robocopy_ingest::gui_api::JobSummary;
use robocopy_ingest::schedule::ScheduledTask;

use crate::jobs_view::kind_text;

/// The "Tipo" column: the kind of copy for a one-job configuration, a count when there are several
/// (each has its own kind, and the table has one cell).
pub fn kind_label(jobs: &[JobSummary]) -> String {
    match jobs {
        [] => String::new(),
        [one] => kind_text(one.backup_type.as_deref()).to_string(),
        many => format!("{} job", many.len()),
    }
}

/// The "Quando" and "Prossima" columns of one task, from the Windows scheduled tasks that run it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScheduleCell {
    /// "Non pianificata", "Ogni giorno alle 22:00", "Sospesa"...
    pub when: String,
    /// At least one Windows task runs this configuration.
    pub scheduled: bool,
    /// Every Windows task that runs it is switched off.
    pub suspended: bool,
    /// When Task Scheduler runs it next, verbatim; empty when unknown or suspended.
    pub next_run: String,
}

pub const NOT_SCHEDULED: &str = "Non pianificata";
pub const SUSPENDED: &str = "Sospesa";

pub fn schedule_cell(found: &[ScheduledTask]) -> ScheduleCell {
    if found.is_empty() {
        return ScheduleCell {
            when: NOT_SCHEDULED.to_string(),
            ..ScheduleCell::default()
        };
    }
    if found.iter().all(|task| task.disabled) {
        return ScheduleCell {
            when: SUSPENDED.to_string(),
            scheduled: true,
            suspended: true,
            next_run: String::new(),
        };
    }
    let active = || found.iter().filter(|task| !task.disabled);
    let when = active()
        .map(|task| task.trigger.trim())
        .find(|text| !text.is_empty())
        .unwrap_or("Pianificata")
        .to_string();
    let next_run = active()
        .map(|task| task.next_run.trim())
        // "N/D" is Task Scheduler's own "no next run": not worth printing.
        .find(|text| !text.is_empty() && !text.eq_ignore_ascii_case("N/D"))
        .unwrap_or_default()
        .to_string();
    ScheduleCell {
        when,
        scheduled: true,
        suspended: false,
        next_run,
    }
}

/// The queue of "Esegui tutte": the tasks in the order of the list, **one at a time** (several copies to the
/// same NAS at once slow every one of them down), each recorded as it ends. A pure state machine, so the
/// order, a task that cannot start and a stop are tested without a window or a copy.
#[derive(Debug, Default)]
pub struct RunAll {
    pending: std::collections::VecDeque<std::path::PathBuf>,
    results: Vec<(String, Option<robocopy_ingest::sessions::SessionState>)>,
    current: Option<String>,
    stopped: bool,
}

impl RunAll {
    pub fn new(paths: impl IntoIterator<Item = std::path::PathBuf>) -> Self {
        Self {
            pending: paths.into_iter().collect(),
            ..Self::default()
        }
    }

    /// The next task to start, which becomes the current one; `None` when none is left.
    pub fn next(&mut self) -> Option<std::path::PathBuf> {
        let next = self.pending.pop_front();
        self.current = next.as_ref().map(|path| {
            path.file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        next
    }

    /// Records how the current task ended (`None`: it could not be started).
    pub fn finished(&mut self, state: Option<robocopy_ingest::sessions::SessionState>) {
        let name = self.current.take().unwrap_or_default();
        self.results.push((name, state));
    }

    /// The person stopped it: nothing still waiting will start.
    pub fn stop(&mut self) {
        self.pending.clear();
        self.stopped = true;
    }

    pub fn summary(&self) -> String {
        run_all_summary(&self.results, self.stopped)
    }
}

/// How "Esegui tutte" ended, in one sentence: how many tasks, how they went, and which need a look.
/// `results` is each task run in turn with how it ended (`None`: it could not be started); `stopped`
/// is true when the person stopped it, which also means the tasks after the one stopped never ran.
pub fn run_all_summary(
    results: &[(String, Option<robocopy_ingest::sessions::SessionState>)],
    stopped: bool,
) -> String {
    use robocopy_ingest::sessions::SessionState;
    if results.is_empty() {
        return "Esegui tutte: nessuna attività attiva da eseguire.".to_string();
    }
    let clean = results
        .iter()
        .filter(|(_, state)| matches!(state, Some(SessionState::Clean | SessionState::DryRun)))
        .count();
    let look: Vec<&str> = results
        .iter()
        .filter(|(_, state)| !matches!(state, Some(SessionState::Clean | SessionState::DryRun)))
        .map(|(name, _)| name.as_str())
        .collect();
    let mut text = format!(
        "Esegui tutte: {} attività, {} riuscit{}",
        results.len(),
        clean,
        if clean == 1 { "a" } else { "e" }
    );
    if !look.is_empty() {
        text.push_str(&format!(", da controllare: {}", look.join(", ")));
    }
    if stopped {
        text.push_str(". Interrotta: le attività rimaste non sono partite");
    }
    text.push('.');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(backup_type: Option<&str>) -> JobSummary {
        JobSummary {
            name: "j".to_string(),
            source: Some("C:/a".to_string()),
            dest: Some("D:/b".to_string()),
            backup_type: backup_type.map(str::to_string),
            mirror: false,
            verify_integrity: false,
            fast_verify: false,
            unconfigured: false,
            report_path: None,
            history_job_name: None,
            encrypt_enabled: false,
            keep_generations: None,
            exclude_count: 0,
            threads: None,
        }
    }

    fn task(trigger: &str, next: &str, disabled: bool) -> ScheduledTask {
        ScheduledTask {
            name: "t".into(),
            command: String::new(),
            next_run: next.into(),
            status: String::new(),
            trigger: trigger.into(),
            disabled,
        }
    }

    #[test]
    fn the_queue_runs_the_tasks_in_order_one_at_a_time_and_records_each() {
        use robocopy_ingest::sessions::SessionState::{Clean, NeedsLook};
        let mut queue = RunAll::new([
            std::path::PathBuf::from("C:/t/notte.toml"),
            std::path::PathBuf::from("C:/t/foto.toml"),
        ]);
        assert_eq!(
            queue.next(),
            Some(std::path::PathBuf::from("C:/t/notte.toml"))
        );
        queue.finished(Some(Clean));
        assert_eq!(
            queue.next(),
            Some(std::path::PathBuf::from("C:/t/foto.toml"))
        );
        queue.finished(Some(NeedsLook));
        assert_eq!(queue.next(), None, "nothing left");
        assert_eq!(
            queue.summary(),
            "Esegui tutte: 2 attività, 1 riuscita, da controllare: foto."
        );
    }

    /// A task that could not be started is counted and the next one still goes.
    #[test]
    fn a_task_that_cannot_start_does_not_stop_the_others() {
        use robocopy_ingest::sessions::SessionState::Clean;
        let mut queue = RunAll::new([
            std::path::PathBuf::from("C:/t/rotta.toml"),
            std::path::PathBuf::from("C:/t/buona.toml"),
        ]);
        queue.next();
        queue.finished(None);
        assert_eq!(
            queue.next(),
            Some(std::path::PathBuf::from("C:/t/buona.toml"))
        );
        queue.finished(Some(Clean));
        assert_eq!(queue.next(), None);
        assert_eq!(
            queue.summary(),
            "Esegui tutte: 2 attività, 1 riuscita, da controllare: rotta."
        );
    }

    /// Stopping the one that runs stops the whole queue: the ones waiting never start.
    #[test]
    fn stopping_clears_what_is_waiting_and_says_so() {
        use robocopy_ingest::sessions::SessionState::Interrupted;
        let mut queue = RunAll::new([
            std::path::PathBuf::from("C:/t/a.toml"),
            std::path::PathBuf::from("C:/t/b.toml"),
            std::path::PathBuf::from("C:/t/c.toml"),
        ]);
        queue.next();
        queue.stop();
        queue.finished(Some(Interrupted));
        assert_eq!(queue.next(), None, "b and c never start");
        let text = queue.summary();
        assert!(
            text.contains("1 attività")
                && text.contains("Interrotta")
                && text.contains("da controllare: a"),
            "{text}"
        );
    }

    #[test]
    fn an_empty_queue_says_there_was_nothing_to_run() {
        let mut queue = RunAll::new(Vec::new());
        assert_eq!(queue.next(), None);
        assert!(queue.summary().contains("nessuna attività attiva"));
    }

    #[test]
    fn the_run_all_summary_counts_and_names_what_needs_a_look() {
        use robocopy_ingest::sessions::SessionState::{Clean, Interrupted, NeedsLook};
        assert_eq!(
            run_all_summary(&[], false),
            "Esegui tutte: nessuna attività attiva da eseguire."
        );
        let all_clean = [
            ("a".to_string(), Some(Clean)),
            ("b".to_string(), Some(Clean)),
        ];
        assert_eq!(
            run_all_summary(&all_clean, false),
            "Esegui tutte: 2 attività, 2 riuscite."
        );
        let mixed = [
            ("notte".to_string(), Some(Clean)),
            ("foto".to_string(), Some(NeedsLook)),
            ("vecchia".to_string(), None),
        ];
        assert_eq!(
            run_all_summary(&mixed, false),
            "Esegui tutte: 3 attività, 1 riuscita, da controllare: foto, vecchia."
        );
        let stopped = [("a".to_string(), Some(Interrupted))];
        let text = run_all_summary(&stopped, true);
        assert!(
            text.contains("Interrotta") && text.contains("da controllare: a"),
            "{text}"
        );
    }

    #[test]
    fn the_kind_is_the_jobs_own_or_a_count() {
        assert_eq!(kind_label(&[]), "");
        assert_eq!(kind_label(&[job(None)]), "Copia semplice");
        assert_eq!(kind_label(&[job(Some("full"))]), "Completa");
        assert_eq!(kind_label(&[job(Some("incremental"))]), "Incrementale");
        assert_eq!(kind_label(&[job(Some("differential"))]), "Differenziale");
        assert_eq!(kind_label(&[job(None), job(Some("full"))]), "2 job");
    }

    #[test]
    fn nothing_scheduled_says_so() {
        let cell = schedule_cell(&[]);
        assert_eq!(cell.when, NOT_SCHEDULED);
        assert!(!cell.scheduled && !cell.suspended && cell.next_run.is_empty());
    }

    #[test]
    fn a_scheduled_task_shows_its_trigger_and_its_next_run() {
        let cell = schedule_cell(&[task("Ogni giorno alle 22:00", "14/10/2026 22:00:00", false)]);
        assert_eq!(cell.when, "Ogni giorno alle 22:00");
        assert!(cell.scheduled && !cell.suspended);
        assert_eq!(cell.next_run, "14/10/2026 22:00:00");
    }

    #[test]
    fn a_trigger_that_could_not_be_read_still_says_it_is_scheduled() {
        let cell = schedule_cell(&[task("", "N/D", false)]);
        assert_eq!(cell.when, "Pianificata");
        assert_eq!(cell.next_run, "", "N/D is not a time");
    }

    #[test]
    fn a_task_switched_off_is_suspended_and_has_no_next_run() {
        let cell = schedule_cell(&[task("Ogni giorno alle 22:00", "14/10/2026 22:00:00", true)]);
        assert_eq!(cell.when, SUSPENDED);
        assert!(cell.scheduled && cell.suspended);
        assert_eq!(cell.next_run, "");
    }

    /// Two Windows tasks run one configuration and only one is off: it is still running on a schedule.
    #[test]
    fn one_task_off_and_one_on_is_not_suspended() {
        let cell = schedule_cell(&[
            task("Ogni 6 ore", "", true),
            task("Ogni giorno alle 22:00", "14/10/2026 22:00:00", false),
        ]);
        assert!(!cell.suspended);
        assert_eq!(cell.when, "Ogni giorno alle 22:00");
    }
}
