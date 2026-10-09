//! The lines of the technical report of a copy (phase 4e): phases, file counts, verification, the
//! problems found and the notices a run left. Everything is read from `gui_api::ReportView`, which is
//! the core's; this module only chooses an order and a wording, and says "and N more" when a list
//! was cut. Pure, so it is tested without a window.

use robocopy_ingest::gui_api::{ErrorPage, ReportView};

use crate::format::{human_bytes, human_duration};

/// `Heading` opens a section, `Line` is a label and a value, `Problem` is a line that needs a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Heading,
    Line,
    Problem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    pub label: String,
    pub value: String,
}

fn heading(label: &str) -> Row {
    Row {
        kind: Kind::Heading,
        label: label.to_string(),
        value: String::new(),
    }
}

fn line(label: &str, value: impl Into<String>) -> Row {
    Row {
        kind: Kind::Line,
        label: label.to_string(),
        value: value.into(),
    }
}

fn problem(label: &str, value: impl Into<String>) -> Row {
    Row {
        kind: Kind::Problem,
        label: label.to_string(),
        value: value.into(),
    }
}

/// The paths of one error list, with how many were cut. `truncated_at_source` is the engine's own
/// cap (the list was already incomplete when written), which is not the same as this page's cut.
fn list_rows(rows: &mut Vec<Row>, title: &str, page: &ErrorPage) {
    if page.total == 0 {
        return;
    }
    rows.push(problem(title, page.total.to_string()));
    for path in &page.entries {
        rows.push(Row {
            kind: Kind::Line,
            label: String::new(),
            value: path.clone(),
        });
    }
    let shown = page.offset + page.entries.len();
    if page.total > shown {
        rows.push(line("", format!("... e altri {}", page.total - shown)));
    }
    if page.truncated_at_source {
        rows.push(line(
            "",
            "L'elenco era già incompleto quando il report è stato scritto.",
        ));
    }
}

/// The core reports the verification outcome as the name of its enum variant; say it in Italian and
/// pass an unknown name through unchanged rather than guess.
fn integrity_word(status: &str) -> String {
    match status {
        "Passed" => "superata".to_string(),
        "Failed" => "differenze trovate".to_string(),
        other => other.to_string(),
    }
}

/// The technical lines for one report.
pub fn rows_for(view: &ReportView) -> Vec<Row> {
    let mut rows = Vec::new();

    rows.push(heading("Fasi"));
    rows.push(line("Inventario", human_duration(view.inventory_seconds)));
    rows.push(line("Copia", human_duration(view.transfer_seconds)));
    if let Some(seconds) = view.verification_seconds {
        rows.push(line("Verifica", human_duration(seconds)));
    }
    if let Some(seconds) = view.baseline_seconds {
        rows.push(line(
            "Confronto con la copia semplice",
            human_duration(seconds),
        ));
    }

    rows.push(heading("File"));
    rows.push(line(
        "Nella sorgente",
        format!(
            "{} file, {}",
            view.total_files,
            human_bytes(view.total_bytes)
        ),
    ));
    rows.push(line(
        "Copiati",
        format!(
            "{} file, {}",
            view.files_copied,
            human_bytes(view.bytes_copied)
        ),
    ));
    if let Some(detail) = view.copy_detail {
        rows.push(line(
            "Già aggiornati, saltati",
            format!(
                "{} file, {}",
                detail.files_skipped,
                human_bytes(detail.bytes_skipped)
            ),
        ));
        if detail.files_mismatch > 0 {
            rows.push(problem(
                "In conflitto (dimensione o data)",
                format!("{} file", detail.files_mismatch),
            ));
        }
        if detail.files_failed > 0 {
            rows.push(problem(
                "Falliti",
                format!(
                    "{} file, {}",
                    detail.files_failed,
                    human_bytes(detail.bytes_failed)
                ),
            ));
        }
        if detail.files_extra > 0 {
            rows.push(line(
                "Solo in destinazione",
                format!("{} file", detail.files_extra),
            ));
        }
    }

    if let Some(status) = &view.integrity_status {
        rows.push(heading("Verifica dei file"));
        rows.push(line("Esito", integrity_word(status)));
        if let Some(checked) = view.files_checked {
            rows.push(line("File controllati", checked.to_string()));
        }
        if let Some(bytes) = view.bytes_hashed {
            rows.push(line("Letti per il confronto", human_bytes(bytes)));
        }
        if let Some(skipped) = view.skipped_unchanged.filter(|n| *n > 0) {
            rows.push(line("Saltati perché invariati", skipped.to_string()));
        }
        list_rows(&mut rows, "Differenze trovate", &view.mismatches);
        list_rows(&mut rows, "Mancanti in destinazione", &view.missing_in_dest);
        list_rows(&mut rows, "Illeggibili", &view.unreadable);
    }

    let notices: Vec<(&str, &Option<String>)> = vec![
        ("Errore di copia", &view.copy_error),
        ("Comando dopo la copia", &view.post_command_error),
        ("Notifica (webhook)", &view.webhook_error),
    ];
    if notices.iter().any(|(_, value)| value.is_some()) {
        rows.push(heading("Avvisi"));
        for (label, value) in notices {
            if let Some(text) = value {
                rows.push(problem(label, text.clone()));
            }
        }
    }

    rows.push(heading("Dove e con cosa"));
    rows.push(line(
        "Computer",
        format!(
            "{} ({}, {} processori logici)",
            view.host_hostname, view.host_os, view.host_cpus
        ),
    ));
    rows.push(line("Versione", view.tool_version.clone()));
    if view.encrypted {
        rows.push(line("Cifratura", "i file in destinazione sono cifrati"));
    }
    if view.decrypted {
        rows.push(line(
            "Decifratura",
            "i file in destinazione sono stati decifrati",
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use robocopy_ingest::gui_api::read_report;

    /// A real report shape, with a verification that found one mismatch.
    const REPORT: &str = r#"{"schema_version":2,"timestamp":"2026-08-31T00:00:00Z","tool_version":"7.8.1","host_platform":"windows","host_metadata":{"hostname":"HOST","os_name":"windows","logical_cpus":8},"source":"D:/src","dest":"E:/dst","total_files":10,"total_bytes":2048,"robocopy_transfer":{"engine":"robocopy","elapsed_seconds":1.5,"throughput_mbps":1.0,"bytes_copied":1024,"files_copied":4,"exit_code":1,"exit_code_meaning":"files copied","retry_attempts_used":0,"dry_run":false},"integrity_check":{"files_checked":10,"bytes_hashed":2048,"mismatches":[{"path":"a.txt","kind":"hash","algorithm":"sha256","source_digest":"x","dest_digest":"y"}],"missing_in_dest":["b.txt","c.txt"],"unreadable":[],"status":"FAILED","truncated":false,"total_errors":3,"skipped_unchanged":2},"phase_timing":{"inventory_seconds":0.5,"transfer_seconds":1.0,"verification_seconds":2.0,"total_seconds":3.5},"configuration":{"threads":8,"retries":3,"retry_wait_seconds":5,"pattern":"*","verify_integrity":true,"compare_baseline":false,"dry_run":false},"log_lines_dropped":0,"encrypted":false,"decrypted":false,"post_command_error":"exit 3"}"#;

    fn view() -> ReportView {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("r.json");
        std::fs::write(&path, REPORT).expect("write");
        read_report(&path).expect("report")
    }

    fn labels(rows: &[Row], kind: Kind) -> Vec<String> {
        rows.iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.label.clone())
            .collect()
    }

    #[test]
    fn sections_come_in_a_fixed_order() {
        let rows = rows_for(&view());
        assert_eq!(
            labels(&rows, Kind::Heading),
            vec![
                "Fasi",
                "File",
                "Verifica dei file",
                "Avvisi",
                "Dove e con cosa"
            ]
        );
    }

    #[test]
    fn a_failed_verification_lists_its_problems_and_marks_them() {
        let rows = rows_for(&view());
        let problems = labels(&rows, Kind::Problem);
        assert!(
            problems.contains(&"Differenze trovate".to_string()),
            "{problems:?}"
        );
        assert!(
            problems.contains(&"Mancanti in destinazione".to_string()),
            "{problems:?}"
        );
        assert!(
            rows.iter().any(|r| r.value == "b.txt"),
            "the missing paths are listed"
        );
        assert!(
            !problems.contains(&"Illeggibili".to_string()),
            "an empty list shows nothing"
        );
    }

    #[test]
    fn the_verification_outcome_is_said_in_italian_and_unknown_names_pass_through() {
        assert_eq!(integrity_word("Passed"), "superata");
        assert_eq!(integrity_word("Failed"), "differenze trovate");
        assert_eq!(integrity_word("Boh"), "Boh");
    }

    #[test]
    fn a_notice_the_run_left_is_a_problem_line() {
        let rows = rows_for(&view());
        let notice = rows
            .iter()
            .find(|r| r.label == "Comando dopo la copia")
            .expect("notice");
        assert_eq!(notice.kind, Kind::Problem);
        assert_eq!(notice.value, "exit 3");
    }

    #[test]
    fn a_cut_list_says_how_many_more_there_are() {
        let page = ErrorPage {
            entries: vec!["a".into(), "b".into()],
            total: 5,
            offset: 0,
            truncated_at_source: true,
        };
        let mut rows = Vec::new();
        list_rows(&mut rows, "Mancanti", &page);
        assert!(rows.iter().any(|r| r.value == "... e altri 3"), "{rows:?}");
        assert!(
            rows.iter().any(|r| r.value.contains("già incompleto")),
            "{rows:?}"
        );
    }

    #[test]
    fn skipped_files_are_shown_as_already_up_to_date() {
        let mut v = view();
        v.copy_detail = Some(robocopy_ingest::engine::CopySummaryDetail {
            files_skipped: 6,
            ..Default::default()
        });
        let rows = rows_for(&v);
        let skipped = rows
            .iter()
            .find(|r| r.label == "Già aggiornati, saltati")
            .expect("row");
        assert!(skipped.value.starts_with("6 file"), "{}", skipped.value);
    }
}
