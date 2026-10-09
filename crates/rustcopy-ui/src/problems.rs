//! The problems a run left, paged and exportable (CATALOGO_COMPORTAMENTI_GUI.md L39). A report can list up to
//! ten thousand files per category; the window shows a hundred at a time and the CSV holds **all** of them,
//! so a person can hand the whole list to someone else. The lists and their totals come from
//! `gui_api::read_report_page`; this only walks them and says where the page is.

use std::path::Path;

use robocopy_ingest::gui_api::{self, ErrorPage};

/// How many problems the window shows at a time.
pub const PAGE: usize = 100;

/// The largest page the core hands out; the export walks the lists in pages of this size.
const EXPORT_PAGE: usize = gui_api::MAX_ERROR_PAGE;

/// Where the page is, in the three lists the window pages together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageInfo {
    /// Any list has at least one problem.
    pub any: bool,
    pub has_prev: bool,
    pub has_next: bool,
    /// For example `Problemi 101–200 di 3412`.
    pub text: String,
}

/// The position of the page `offset` is on, given the three lists as the core returned them for it.
pub fn page_info(pages: [&ErrorPage; 3]) -> PageInfo {
    let longest = pages.iter().map(|p| p.total).max().unwrap_or(0);
    let offset = pages.first().map_or(0, |p| p.offset);
    let shown_end = pages
        .iter()
        .map(|p| p.offset + p.entries.len())
        .max()
        .unwrap_or(0);
    PageInfo {
        any: longest > 0,
        has_prev: offset > 0,
        has_next: pages.iter().any(|p| p.total > p.offset + p.entries.len()),
        text: if longest == 0 {
            String::new()
        } else {
            format!(
                "Problemi {}–{} di {}",
                (offset + 1).min(longest),
                shown_end.min(longest).max(offset.min(longest)),
                longest
            )
        },
    }
}

/// The offset of the page before / after the one at `offset` (never below zero).
pub fn neighbour(offset: usize, forward: bool) -> usize {
    if forward {
        offset + PAGE
    } else {
        offset.saturating_sub(PAGE)
    }
}

/// The three lists' names, in the order the report lists them.
const CATEGORIES: [&str; 3] = [
    "Differenze trovate",
    "Mancanti in destinazione",
    "Illeggibili",
];

/// Every problem of the report at `path`, as `(category, path)`, walking each list page by page.
pub fn all_problems(path: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut offset = 0;
    loop {
        let view =
            gui_api::read_report_page(path, offset, EXPORT_PAGE).map_err(|e| e.to_string())?;
        let lists = [&view.mismatches, &view.missing_in_dest, &view.unreadable];
        let mut more = false;
        for (category, page) in CATEGORIES.iter().zip(lists) {
            for entry in &page.entries {
                out.push(((*category).to_string(), entry.clone()));
            }
            more |= page.total > page.offset + page.entries.len();
        }
        if !more {
            return Ok(out);
        }
        offset += EXPORT_PAGE;
    }
}

/// The CSV of a list of problems.
pub fn problems_csv(rows: &[(String, String)]) -> String {
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|(category, path)| vec![category.clone(), path.clone()])
        .collect();
    crate::csv::to_csv(&["Categoria", "Percorso"], &table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(total: usize, offset: usize, shown: usize) -> ErrorPage {
        ErrorPage {
            entries: (0..shown).map(|i| format!("f{i}")).collect(),
            total,
            offset,
            truncated_at_source: false,
        }
    }

    #[test]
    fn the_first_page_of_many_has_a_next_and_no_previous() {
        let (a, b, c) = (page(342, 0, 100), page(0, 0, 0), page(5, 0, 5));
        let info = page_info([&a, &b, &c]);
        assert!(info.any && info.has_next && !info.has_prev);
        assert_eq!(info.text, "Problemi 1–100 di 342");
    }

    #[test]
    fn the_last_page_has_a_previous_and_no_next() {
        let (a, b, c) = (page(342, 300, 42), page(0, 300, 0), page(5, 300, 0));
        let info = page_info([&a, &b, &c]);
        assert!(info.has_prev && !info.has_next);
        assert_eq!(info.text, "Problemi 301–342 di 342");
    }

    #[test]
    fn a_report_with_no_problem_says_nothing_and_offers_no_paging() {
        let (a, b, c) = (page(0, 0, 0), page(0, 0, 0), page(0, 0, 0));
        let info = page_info([&a, &b, &c]);
        assert!(!info.any && !info.has_next && !info.has_prev);
        assert_eq!(info.text, "");
    }

    /// A report written by the real CLI (trimmed), with the given lists in its integrity check.
    fn report_json(missing: usize, unreadable: usize) -> String {
        let list = |n: usize, prefix: &str| {
            (0..n)
                .map(|i| format!("\"{prefix}{i}\""))
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            r#"{{"schema_version":2,"timestamp":"2026-10-09T18:46:27Z","tool_version":"7.8.1","host_platform":"windows","host_metadata":{{"hostname":"h","os_name":"windows","logical_cpus":4}},"source":"src","dest":"dst","total_files":3,"total_bytes":12,"robocopy_transfer":{{"engine":"robocopy","elapsed_seconds":0.05,"throughput_mbps":0.0,"bytes_copied":12,"files_copied":3,"exit_code":1,"exit_code_meaning":"files copied","retry_attempts_used":0,"dry_run":false}},"integrity_check":{{"files_checked":3,"bytes_hashed":12,"mismatches":[],"missing_in_dest":[{}],"unreadable":[{}],"status":"FAILED","truncated":false,"total_errors":0,"skipped_unchanged":0}},"phase_timing":{{"inventory_seconds":0.0,"transfer_seconds":0.0,"verification_seconds":0.0,"total_seconds":0.0}},"configuration":{{"threads":4,"retries":3,"retry_wait_seconds":5,"pattern":"*","verify_integrity":true,"compare_baseline":false,"dry_run":false,"mirror":false,"exclude_files":[],"exclude_dirs":[],"hash_algo":"sha256","fast_verify":false,"exclude_junctions":false,"vss_snapshot":false}},"log_lines_dropped":0,"encrypted":false,"decrypted":false}}"#,
            list(missing, "m"),
            list(unreadable, "u"),
        )
    }

    #[test]
    fn the_export_walks_every_page_of_every_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        // More than one export page in one list, a short one in another.
        std::fs::write(&path, report_json(EXPORT_PAGE * 2 + 37, 5)).unwrap();
        let all = all_problems(&path).unwrap();
        let missing = all
            .iter()
            .filter(|(c, _)| c == "Mancanti in destinazione")
            .count();
        let unreadable = all.iter().filter(|(c, _)| c == "Illeggibili").count();
        assert_eq!(missing, EXPORT_PAGE * 2 + 37);
        assert_eq!(unreadable, 5);
        assert_eq!(all.len(), missing + unreadable);
        // No entry twice: paging by offset must not repeat a page.
        let mut seen: Vec<&String> = all.iter().map(|(_, p)| p).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), all.len());
    }

    #[test]
    fn neighbours_step_by_a_page_and_never_go_below_zero() {
        assert_eq!(neighbour(0, true), PAGE);
        assert_eq!(neighbour(PAGE, false), 0);
        assert_eq!(neighbour(30, false), 0);
    }

    #[test]
    fn the_csv_has_a_category_and_a_path_per_line_and_neutralises_a_formula_name() {
        let rows = vec![
            ("Differenze trovate".to_string(), r"D:\a\b.txt".to_string()),
            ("Illeggibili".to_string(), "=cmd()".to_string()),
        ];
        let csv = problems_csv(&rows);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("Categoria,Percorso"));
        assert_eq!(lines[1], r"Differenze trovate,D:\a\b.txt");
        assert_eq!(lines[2], "Illeggibili,'=cmd()");
    }
}
