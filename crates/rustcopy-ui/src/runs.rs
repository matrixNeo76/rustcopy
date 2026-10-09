//! The runs of the history page: what one line holds, which lines an outcome filter lets through, and
//! the CSV of what is shown. Pure, so it is unit-tested (CATALOGO_COMPORTAMENTI_GUI.md L24).

use crate::csv;

/// One past run, with everything the list and the CSV show.
#[derive(Clone, Debug)]
pub struct RunLine {
    pub at: chrono::DateTime<chrono::Utc>,
    pub job: String,
    pub files: u64,
    pub bytes: u64,
    pub throughput_mbps: f64,
    pub exit_code: i32,
    pub meaning: String,
    /// 0 clean, 1 dry run, 2 needs a look.
    pub outcome: i32,
}

impl RunLine {
    pub fn outcome_text(&self) -> &'static str {
        match self.outcome {
            0 => "Riuscita",
            1 => "Di prova",
            _ => "Da controllare",
        }
    }
}

/// Which runs the outcome filter lets through (the order of the combo box: all, clean, needs a look,
/// dry run).
pub fn passes_filter(filter: i32, run: &RunLine) -> bool {
    match filter {
        1 => run.outcome == 0,
        2 => run.outcome == 2,
        3 => run.outcome == 1,
        _ => true,
    }
}

/// The CSV of `runs`, in the order given.
pub fn history_csv(runs: &[RunLine]) -> String {
    let rows: Vec<Vec<String>> = runs
        .iter()
        .map(|run| {
            vec![
                run.at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                run.job.clone(),
                run.outcome_text().to_string(),
                run.exit_code.to_string(),
                run.meaning.clone(),
                run.files.to_string(),
                run.bytes.to_string(),
                format!("{:.2}", run.throughput_mbps),
            ]
        })
        .collect();
    csv::to_csv(
        &[
            "Quando (UTC)",
            "Job",
            "Esito",
            "Codice uscita",
            "Significato",
            "File copiati",
            "Byte copiati",
            "Throughput (MB/s)",
        ],
        &rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(outcome: i32, code: i32) -> RunLine {
        RunLine {
            at: chrono::DateTime::parse_from_rfc3339("2026-10-09T01:02:03Z")
                .expect("date")
                .with_timezone(&chrono::Utc),
            job: "foto".to_string(),
            files: 12,
            bytes: 3456,
            throughput_mbps: 98.765,
            exit_code: code,
            meaning: "ok, tutto".to_string(),
            outcome,
        }
    }

    #[test]
    fn each_filter_keeps_only_its_outcome_and_all_keeps_everything() {
        let runs = [run(0, 0), run(1, 0), run(2, 8)];
        let kept = |filter| {
            runs.iter()
                .filter(|r| passes_filter(filter, r))
                .map(|r| r.outcome)
                .collect::<Vec<_>>()
        };
        assert_eq!(kept(0), vec![0, 1, 2]);
        assert_eq!(kept(1), vec![0]);
        assert_eq!(kept(2), vec![2]);
        assert_eq!(kept(3), vec![1]);
    }

    #[test]
    fn the_csv_has_a_header_and_one_line_per_run_with_text_quoted() {
        let csv = history_csv(&[run(0, 0), run(2, 8)]);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("Quando (UTC),Job,Esito"), "{}", lines[0]);
        assert_eq!(
            lines[1],
            "2026-10-09T01:02:03Z,foto,Riuscita,0,\"ok, tutto\",12,3456,98.77"
        );
        assert!(lines[2].contains(",Da controllare,8,"), "{}", lines[2]);
    }
}
