//! The log page (CATALOGO_COMPORTAMENTI_GUI.md L48): the last lines of the file the command line wrote
//! for a configuration. The reading is the core's (`gui_api::read_log_tail`, bounded); this decides which
//! configuration a lavoro stands for and what to say about a log that is longer than what is shown, so it
//! is pure and unit-tested.

use std::path::{Path, PathBuf};

use robocopy_ingest::gui_api::LogTail;

use crate::format::human_bytes;

/// How many lines the page shows.
pub const LINES: usize = 400;

/// The configuration a lavoro stands for: the saved task it ran, else the copy of its settings kept in
/// its own folder. `None` when there is neither (a resume has only a checkpoint).
pub fn config_of(task: Option<&Path>, folder: &Path) -> Option<PathBuf> {
    if let Some(task) = task {
        return Some(task.to_path_buf());
    }
    let own = folder.join("config.toml");
    own.is_file().then_some(own)
}

/// A line about what the page does not show; empty when it shows everything.
pub fn note(tail: &LogTail) -> String {
    if tail.truncated {
        format!(
            "Si vedono le ultime righe di un registro di {}: il resto sta nel file.",
            human_bytes(tail.total_bytes)
        )
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_stands_for_its_own_file_and_a_plain_copy_for_the_one_in_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            config_of(Some(Path::new("C:/t/a.toml")), dir.path()),
            Some(PathBuf::from("C:/t/a.toml"))
        );
        assert_eq!(config_of(None, dir.path()), None, "no config.toml yet");
        std::fs::write(dir.path().join("config.toml"), "x").unwrap();
        assert_eq!(
            config_of(None, dir.path()),
            Some(dir.path().join("config.toml"))
        );
    }

    #[test]
    fn the_note_says_so_only_when_there_is_more_than_what_is_shown() {
        let mut tail = LogTail {
            path: PathBuf::from("x.log"),
            lines: vec!["a".into()],
            truncated: false,
            total_bytes: 10,
        };
        assert_eq!(note(&tail), "");
        tail.truncated = true;
        tail.total_bytes = 5 * 1024 * 1024;
        assert!(note(&tail).contains("MB"), "{}", note(&tail));
    }
}
