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
