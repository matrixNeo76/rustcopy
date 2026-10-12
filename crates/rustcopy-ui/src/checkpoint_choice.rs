//! Choosing which interrupted run to resume (CATALOGO_COMPORTAMENTI_GUI.md L47). A lavoro can leave
//! more than one checkpoint (stopped, resumed, stopped again); the core lists them newest first and
//! refuses any that is not the lavoro's own. This only gives each one a line a person can tell apart,
//! so it is pure and unit-tested.

use robocopy_ingest::gui_api::CheckpointSummary;

use crate::format::when_short;

/// The longest reason shown whole; a longer one is cut (a stop request carries the path of the stop
/// file, which says nothing a person can use).
const REASON_LIMIT: usize = 48;

/// A reason in words a person can read. The two the CLI writes itself are put in Italian; anything
/// else is shown as written, cut when long.
fn readable_reason(reason: &str) -> String {
    let reason = reason.trim();
    if reason.starts_with("stop requested") {
        return "fermata dalla console".to_string();
    }
    if let Some(rest) = reason.strip_prefix("interrupted by Ctrl+C") {
        return format!("interrotta con Ctrl+C{rest}");
    }
    if reason.chars().count() > REASON_LIMIT {
        let cut: String = reason.chars().take(REASON_LIMIT).collect();
        return format!("{cut}...");
    }
    reason.to_string()
}

/// One line per checkpoint: when it was written and why.
pub fn label(checkpoint: &CheckpointSummary) -> String {
    let when = when_short(checkpoint.timestamp);
    let reason = readable_reason(&checkpoint.reason);
    if reason.is_empty() {
        when
    } else {
        format!("{when}  -  {reason}")
    }
}

/// The path to resume for the choice at `index`: the chosen one, or the newest when `index` is out of
/// range (a stale selection must never resume nothing, and never resumes a path that is not in the list).
pub fn path_at(paths: &[String], index: usize) -> Option<&str> {
    paths
        .get(index)
        .or_else(|| paths.first())
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn checkpoint(reason: &str) -> CheckpointSummary {
        CheckpointSummary {
            path: "C:/x/a.checkpoint.json".to_string(),
            timestamp: chrono::Utc
                .with_ymd_and_hms(2026, 10, 9, 21, 46, 0)
                .unwrap(),
            source: "S".to_string(),
            dest: "D".to_string(),
            reason: reason.to_string(),
        }
    }

    #[test]
    fn a_line_says_when_and_why() {
        let line = label(&checkpoint("interrupted by Ctrl+C"));
        assert!(line.ends_with("  -  interrotta con Ctrl+C"), "{line}");
        assert!(line.contains("09/10") || line.contains("9/10"), "{line}");
    }

    #[test]
    fn a_stop_request_does_not_show_the_path_of_the_stop_file() {
        let line = label(&checkpoint(
            "stop requested via C:/Users/x/AppData/Local/Temp/rustcopy/many.stop-1",
        ));
        assert!(line.ends_with("  -  fermata dalla console"), "{line}");
        assert!(!line.contains("Temp"), "{line}");
    }

    #[test]
    fn an_unknown_long_reason_is_cut_and_a_short_one_kept() {
        let line = label(&checkpoint(&"x".repeat(200)));
        assert!(line.ends_with("..."), "{line}");
        assert!(line.chars().count() < 90, "{line}");
        assert!(label(&checkpoint("disk removed")).ends_with("  -  disk removed"));
    }

    #[test]
    fn a_checkpoint_with_no_reason_is_just_its_date() {
        let line = label(&checkpoint("  "));
        assert!(!line.contains(" - "), "{line}");
    }

    #[test]
    fn the_chosen_index_picks_its_path_and_a_stale_one_falls_back_to_the_newest() {
        let paths = vec!["new".to_string(), "old".to_string()];
        assert_eq!(path_at(&paths, 1), Some("old"));
        assert_eq!(path_at(&paths, 9), Some("new"));
        assert_eq!(path_at(&[], 0), None);
    }
}
