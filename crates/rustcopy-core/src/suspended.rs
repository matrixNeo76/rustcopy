//! Which tasks the person has suspended (PIANO_GUI_ATTIVITA.md 3.4).
//!
//! A suspended task is left out of "Esegui tutte" and shown as such; its Windows schedule, if it has one, is
//! switched off at the same time (`schedule::set_enabled`). The list lives in the console's own data
//! folder, **never in the task's file**: a configuration that travels between machines cannot carry a
//! "suspended" flag that would silently stop a backup on the machine it lands on, and cannot be used to
//! hide a task from the list either. Suspending deletes nothing and a suspended task can still be run by
//! hand.

use std::path::{Path, PathBuf};

use crate::errors::IngestError;

const SUSPENDED_FILE: &str = "suspended.txt";

/// The paths of the suspended tasks, one per line, compared case-insensitively as Windows does.
#[derive(Debug, Clone)]
pub struct SuspendedSet {
    file: PathBuf,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

impl SuspendedSet {
    pub fn at(base: &Path) -> Self {
        Self {
            file: base.join(SUSPENDED_FILE),
        }
    }

    /// The list in the console's data folder.
    pub fn default_location() -> Self {
        Self::at(&crate::sessions::data_dir())
    }

    fn read(&self) -> Vec<PathBuf> {
        std::fs::read_to_string(&self.file)
            .map(|text| {
                // A hand-edited list may start with a byte-order mark, as `configs.txt` may.
                text.trim_start_matches('\u{feff}')
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether `config` is suspended.
    pub fn contains(&self, config: &Path) -> bool {
        let wanted = key(config);
        self.read().iter().any(|p| key(p) == wanted)
    }

    /// Every suspended path.
    pub fn all(&self) -> Vec<PathBuf> {
        self.read()
    }

    /// Suspends (`true`) or reactivates (`false`) `config`. Asking for the state it already has changes
    /// nothing and is not an error.
    pub fn set(&self, config: &Path, suspended: bool) -> Result<(), IngestError> {
        let wanted = key(config);
        let mut paths: Vec<PathBuf> = self
            .read()
            .into_iter()
            .filter(|p| key(p) != wanted)
            .collect();
        if suspended {
            paths.push(config.to_path_buf());
        }
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| IngestError::io(dir, e))?;
        }
        let text: String = paths
            .iter()
            .map(|p| format!("{}\n", p.to_string_lossy()))
            .collect();
        crate::atomic_write(&self.file, text.as_bytes())
            .map_err(|e| IngestError::io(self.file.clone(), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_suspended_until_asked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let set = SuspendedSet::at(dir.path());
        assert!(!set.contains(Path::new("C:/t/a.toml")));
        assert!(set.all().is_empty());
    }

    #[test]
    fn a_task_is_suspended_and_reactivated_case_insensitively() {
        let dir = tempfile::tempdir().expect("tempdir");
        let set = SuspendedSet::at(dir.path());
        set.set(Path::new("C:/T/Notte.toml"), true)
            .expect("suspend");
        assert!(
            set.contains(Path::new("c:/t/notte.toml")),
            "Windows paths ignore case"
        );
        assert!(!set.contains(Path::new("C:/t/altra.toml")));
        set.set(Path::new("c:/t/NOTTE.toml"), false)
            .expect("reactivate");
        assert!(!set.contains(Path::new("C:/T/Notte.toml")));
    }

    #[test]
    fn asking_twice_keeps_one_entry_and_is_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let set = SuspendedSet::at(dir.path());
        set.set(Path::new("C:/t/a.toml"), true).expect("first");
        set.set(Path::new("C:/t/a.toml"), true).expect("second");
        assert_eq!(set.all().len(), 1);
        set.set(Path::new("C:/t/never.toml"), false)
            .expect("not there");
        assert_eq!(set.all().len(), 1);
    }

    /// The list is a plain file in the console's folder; a hand-edited one with a byte-order mark reads.
    #[test]
    fn a_hand_edited_list_with_a_byte_order_mark_reads() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(SUSPENDED_FILE),
            "\u{feff}C:/t/a.toml\n\n  C:/t/b.toml  \n",
        )
        .expect("write");
        let set = SuspendedSet::at(dir.path());
        assert!(set.contains(Path::new("C:/t/a.toml")));
        assert!(set.contains(Path::new("C:/t/b.toml")));
    }

    /// The flag is not in the task's file: suspending touches nothing but the list.
    #[test]
    fn suspending_never_touches_the_task_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let task = dir.path().join("a.toml");
        std::fs::write(&task, "source = 'C:/a'\ndest = 'D:/b'\n").expect("task");
        let before = std::fs::read(&task).expect("read");
        SuspendedSet::at(&dir.path().join("data"))
            .set(&task, true)
            .expect("suspend");
        assert_eq!(std::fs::read(&task).expect("read"), before);
    }
}
