//! One line per job of a configuration (CATALOGO_COMPORTAMENTI_GUI.md L38), the view the Tauri console's
//! Job tab gave and a configuration-per-row list could not: what each job copies and how, the settings worth
//! seeing at a glance, and how its last run went. The numbers and the decisions come from
//! `gui_api::list_jobs` and `gui_api::read_history`; this only turns them into text, so it is pure and tested.

use robocopy_ingest::gui_api::JobSummary;

/// The text of one job's line, before its last run is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLine {
    pub name: String,
    /// `origine → destinazione`, or what is missing.
    pub route: String,
    /// The kind of copy, in words.
    pub kind: String,
    /// The settings worth a glance, separated by a middle dot; empty when there are none.
    pub badges: String,
    /// This job purges at the destination: drawn apart from an ordinary copy.
    pub mirror: bool,
    /// Still holds a template placeholder: it looks configured and is not.
    pub unconfigured: bool,
}

fn kind_text(backup_type: Option<&str>) -> &'static str {
    match backup_type {
        Some("full") => "Completa",
        Some("incremental") => "Incrementale",
        Some("differential") => "Differenziale",
        _ => "Copia semplice",
    }
}

/// The line for `job`. `default_threads` is what an empty thread count resolves to on this machine, so a
/// thread count equal to it is not worth a badge.
pub fn job_line(job: &JobSummary, default_threads: u16) -> JobLine {
    let route = match (&job.source, &job.dest) {
        (Some(source), Some(dest)) => format!("{source}  →  {dest}"),
        (Some(source), None) => format!("{source}  →  (destinazione mancante)"),
        (None, Some(dest)) => format!("(origine mancante)  →  {dest}"),
        (None, None) => "(origine e destinazione mancanti)".to_string(),
    };
    let mut badges: Vec<String> = Vec::new();
    if job.verify_integrity {
        badges.push(if job.fast_verify {
            "verifica veloce".to_string()
        } else {
            "verifica".to_string()
        });
    }
    if job.encrypt_enabled {
        badges.push("cifrata".to_string());
    }
    if let Some(keep) = job.keep_generations {
        badges.push(format!("conserva {keep} cicli"));
    }
    if job.exclude_count > 0 {
        badges.push(format!("{} esclusioni", job.exclude_count));
    }
    if let Some(threads) = job.threads.filter(|t| *t != default_threads) {
        badges.push(format!("{threads} thread"));
    }
    JobLine {
        name: job.name.clone(),
        route,
        kind: kind_text(job.backup_type.as_deref()).to_string(),
        badges: badges.join("  ·  "),
        mirror: job.mirror,
        unconfigured: job.unconfigured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> JobSummary {
        JobSummary {
            name: "foto".to_string(),
            source: Some("C:/foto".to_string()),
            dest: Some("D:/backup".to_string()),
            backup_type: None,
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

    #[test]
    fn a_plain_job_has_a_route_a_kind_and_no_badges() {
        let line = job_line(&job(), 8);
        assert_eq!(line.route, "C:/foto  →  D:/backup");
        assert_eq!(line.kind, "Copia semplice");
        assert_eq!(line.badges, "");
        assert!(!line.mirror && !line.unconfigured);
    }

    #[test]
    fn the_settings_worth_a_glance_become_badges_in_a_fixed_order() {
        let mut busy = job();
        busy.backup_type = Some("incremental".to_string());
        busy.verify_integrity = true;
        busy.fast_verify = true;
        busy.encrypt_enabled = true;
        busy.keep_generations = Some(5);
        busy.exclude_count = 3;
        busy.threads = Some(2);
        let line = job_line(&busy, 8);
        assert_eq!(line.kind, "Incrementale");
        assert_eq!(
            line.badges,
            "verifica veloce  ·  cifrata  ·  conserva 5 cicli  ·  3 esclusioni  ·  2 thread"
        );
    }

    #[test]
    fn a_thread_count_equal_to_the_machines_default_is_not_a_badge() {
        let mut same = job();
        same.threads = Some(8);
        assert_eq!(job_line(&same, 8).badges, "");
    }

    #[test]
    fn a_missing_side_is_named_not_hidden_and_a_purging_job_is_flagged() {
        let mut broken = job();
        broken.source = None;
        broken.mirror = true;
        broken.unconfigured = true;
        let line = job_line(&broken, 8);
        assert!(line.route.starts_with("(origine mancante)"));
        assert!(line.mirror && line.unconfigured);
        broken.dest = None;
        assert_eq!(
            job_line(&broken, 8).route,
            "(origine e destinazione mancanti)"
        );
    }
}
