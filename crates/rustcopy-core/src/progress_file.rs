//! Publishing a run's progress to a supervisor that is not a terminal.
//!
//! The CLI already knows where it is: [`crate::progress::ThroughputProgress`] keeps bytes, files
//! and elapsed time behind lock-free atomics, and the terminal bar samples them. What was missing
//! is a way for a *different process* — the desktop console, a service wrapper — to see the same
//! numbers, because a progress bar drawn with ANSI escapes is for a person, not for a program.
//!
//! # Why a rewritten line and not an appended one
//!
//! `generations` and `history` append NDJSON because they need the **history**: every generation
//! and every past run matters later. Progress needs only **now**. Appending would leave a file
//! growing for the length of a backup — 43 000 lines over a twelve-hour run — holding values
//! nobody reads twice.
//!
//! So this is one line, rewritten in place through [`crate::atomic_write`], which writes a sibling
//! temp file and renames it (D14). A reader therefore never observes a half-written line, without
//! either side taking a lock.
//!
//! # Why the phase is part of the payload
//!
//! A backup is not one long copy. It inventories, then transfers, then verifies, and verification
//! of a large tree can take minutes during which no byte is copied. A window rendering only
//! "bytes copied" would sit at 100% throughout it and look hung — the reader would conclude the
//! run had frozen at the exact moment it was working hardest.
//!
//! # Why the totals are optional
//!
//! During the inventory the total is not yet known. `Some(0)` and `None` are different claims, and
//! rendering an unknown total as 0% would show a run stuck at zero for the twenty minutes a
//! 1.34M-file prescan takes.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Which part of a run is currently working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Walking the source. The totals below are not known yet.
    Inventory,
    /// Copying.
    Transfer,
    /// Re-reading and hashing what was copied.
    Verification,
}

impl Phase {
    /// What to show a person, decided here rather than in a frontend: which phase a run is in is a
    /// fact about the backup, and naming it is not a rendering choice.
    pub fn describe(self) -> &'static str {
        match self {
            Phase::Inventory => "inventario della sorgente",
            Phase::Transfer => "copia in corso",
            Phase::Verification => "verifica dei byte copiati",
        }
    }
}

/// One sample of a run's progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressSample {
    pub phase: Phase,
    /// Bytes accounted for so far.
    pub bytes_done: u64,
    /// The run's total, when it is known. `None` during the inventory — not `Some(0)`, which a
    /// reader would render as a run stuck at zero.
    pub bytes_total: Option<u64>,
    pub files_done: u64,
    pub files_total: Option<u64>,
    pub elapsed_seconds: f64,
    pub throughput_mbps: f64,
}

impl ProgressSample {
    /// Fraction complete, when that can honestly be computed.
    ///
    /// `None` whenever the total is unknown or zero: a percentage invented from a missing total is
    /// worse than no percentage, because it looks like knowledge.
    pub fn fraction(&self) -> Option<f64> {
        match self.bytes_total {
            Some(total) if total > 0 => Some((self.bytes_done as f64 / total as f64).min(1.0)),
            _ => None,
        }
    }

    /// Writes this sample to `path`, replacing whatever was there.
    ///
    /// Through [`crate::atomic_write`], so a reader polling the file cannot catch it half-written.
    /// The caller decides what a failure means; in the CLI it means a line in the log and nothing
    /// else, because losing progress must never fail a backup that is otherwise succeeding.
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        let line = serde_json::to_string(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        crate::atomic_write(path, line.as_bytes())
    }

    /// Reads the last published sample, if there is one.
    ///
    /// A missing or unparseable file yields `None` rather than an error: a supervisor asking "where
    /// is the run" before the first sample lands is asking a reasonable question, and the answer is
    /// "not yet", not a failure.
    pub fn read_from(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(bytes_done: u64, bytes_total: Option<u64>) -> ProgressSample {
        ProgressSample {
            phase: Phase::Transfer,
            bytes_done,
            bytes_total,
            files_done: 3,
            files_total: Some(10),
            elapsed_seconds: 1.5,
            throughput_mbps: 42.0,
        }
    }

    /// An unknown total is not a zero total. Rendering the first as the second shows a run stuck at
    /// 0% for however long the prescan takes — twenty minutes on the real 1.34M-file profile.
    #[test]
    fn an_unknown_total_yields_no_percentage_rather_than_zero() {
        assert_eq!(sample(500, None).fraction(), None);
        assert_eq!(sample(500, Some(0)).fraction(), None);
        assert_eq!(sample(500, Some(1000)).fraction(), Some(0.5));
    }

    /// Robocopy's reported bytes can exceed the inventory's total (it counts directory entries the
    /// inventory does not). A bar past 100% reads as a bug in the tool rather than as an accounting
    /// difference.
    #[test]
    fn progress_past_the_total_is_capped_rather_than_shown_above_one() {
        assert_eq!(sample(2000, Some(1000)).fraction(), Some(1.0));
    }

    #[test]
    fn a_sample_round_trips_through_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("progress.json");

        let written = sample(700, Some(1000));
        written.write_to(&path).expect("writes");

        assert_eq!(ProgressSample::read_from(&path), Some(written));
    }

    /// Asking where a run is before its first sample is a reasonable question with the answer "not
    /// yet". Neither a missing file nor a corrupt one may look like a failure of the backup.
    #[test]
    fn a_missing_or_unreadable_sample_reads_as_absent_not_as_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");

        assert_eq!(
            ProgressSample::read_from(&dir.path().join("nope.json")),
            None
        );

        let torn = dir.path().join("torn.json");
        std::fs::write(&torn, b"{\"phase\":\"tran").expect("write");
        assert_eq!(ProgressSample::read_from(&torn), None);
    }

    /// The rewrite leaves nothing behind: a supervisor polling the directory must not find a
    /// growing pile of temp files next to the one it reads.
    #[test]
    fn rewriting_leaves_exactly_one_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("progress.json");

        for done in [100, 200, 300] {
            sample(done, Some(1000)).write_to(&path).expect("writes");
        }

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1, "one file, not one per write");
        assert_eq!(
            ProgressSample::read_from(&path)
                .expect("readable")
                .bytes_done,
            300,
            "and it holds the latest sample, not the first"
        );
    }
}
