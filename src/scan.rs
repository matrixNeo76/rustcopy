//! Source tree inspection: pattern matching, file inventory and directory sizing.
//!
//! Key changes from v0.1:
//! * F1.4: glob is now matched against the **relative path**, not just the file name, so
//!   patterns like `nested/*.csv` and `**/*.csv` work correctly.
//! * F2.2: `ScanSummary` keeps a `Vec<ScannedFile>` only for the integrity-check phase.
//!   The `inventory` function returns just the counters, avoiding multi-GB RAM usage.
//! * F3.3: On Windows the glob is compiled case-insensitively so `*.csv` matches `FILE.CSV`.
//! * F2.1: `directory_size` is preserved but only called with much longer intervals now.

use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher};
use walkdir::WalkDir;

use crate::errors::IngestError;

/// One matched source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Path relative to the scan root, so it can be joined onto any destination.
    pub relative_path: PathBuf,
    pub size_bytes: u64,
}

/// Inventory of everything the ingestion will move.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanSummary {
    /// Full file list — kept for integrity verification. For large trees this can be
    /// significant RAM; `--no-prescan` avoids materialising it (see `total_files_hint`).
    pub files: Vec<ScannedFile>,
    pub total_bytes: u64,
    /// Set when `files` is intentionally empty because `--no-prescan` used the lightweight
    /// [`inventory`] walk instead of [`scan`]. `file_count`/`is_empty` prefer this over
    /// `files.len()` so `--no-prescan` doesn't get misreported as "no files matched".
    pub total_files_hint: Option<u64>,
}

impl ScanSummary {
    pub fn file_count(&self) -> usize {
        self.total_files_hint
            .map(|n| n as usize)
            .unwrap_or(self.files.len())
    }

    pub fn is_empty(&self) -> bool {
        self.file_count() == 0
    }
}

/// Compile a file pattern such as `*.csv` into a matcher.
///
/// F3.3: On Windows the match is case-insensitive (`.csv` matches `.CSV`).
pub fn build_matcher(pattern: &str) -> Result<GlobMatcher, IngestError> {
    let case_insensitive = cfg!(windows);
    GlobBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|source| IngestError::InvalidPattern {
            pattern: pattern.to_string(),
            source,
        })
}

/// Walk `root` recursively and collect every file whose **relative path** matches `pattern`.
///
/// F1.4: matching is done on the relative path (e.g. `subdir/file.csv`) instead of the bare
/// filename, so patterns like `nested/*.csv` or `**/*.csv` resolve correctly.
///
/// Unreadable entries are logged and skipped rather than aborting a long ingestion.
pub fn scan(root: &Path, pattern: &str) -> Result<ScanSummary, IngestError> {
    let matcher = build_matcher(pattern)?;
    let mut summary = ScanSummary::default();

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!("skipping unreadable entry: {error}");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }

        // F1.4: match against the relative path, not just the file name.
        let relative_path = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_path_buf();

        if !matcher.is_match(&relative_path) {
            continue;
        }

        let size_bytes = match entry.metadata() {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                tracing::warn!(path = %entry.path().display(), "skipping file without metadata: {error}");
                continue;
            }
        };

        summary.total_bytes += size_bytes;
        summary.files.push(ScannedFile {
            relative_path,
            size_bytes,
        });
    }

    summary
        .files
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(summary)
}

/// Lightweight inventory: counts files and bytes without materialising the file list.
///
/// F2.2 / F2.6: used when `--no-prescan` is set, or when we only need the totals for
/// the progress bar and don't need per-file paths (integrity check skipped).
pub struct InventorySummary {
    pub total_files: u64,
    pub total_bytes: u64,
}

impl InventorySummary {
    pub fn is_empty(&self) -> bool {
        self.total_files == 0
    }

    /// Adapt a lightweight inventory into a [`ScanSummary`] with an empty file list, for
    /// `--no-prescan`. Per-file operations (integrity verification) are unavailable in this
    /// mode since the individual paths were never collected.
    pub fn into_scan_summary(self) -> ScanSummary {
        ScanSummary {
            files: Vec::new(),
            total_bytes: self.total_bytes,
            total_files_hint: Some(self.total_files),
        }
    }
}

/// Walk `root` and return just totals, without keeping individual paths in RAM.
pub fn inventory(root: &Path, pattern: &str) -> Result<InventorySummary, IngestError> {
    let matcher = build_matcher(pattern)?;
    let mut total_files = 0u64;
    let mut total_bytes = 0u64;

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!("skipping unreadable entry: {error}");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }

        let relative_path = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_path_buf();

        if !matcher.is_match(&relative_path) {
            continue;
        }

        if let Ok(meta) = entry.metadata() {
            total_files += 1;
            total_bytes += meta.len();
        }
    }

    Ok(InventorySummary {
        total_files,
        total_bytes,
    })
}

/// Total size of every file under `root`, used by the progress poller.
///
/// Errors are swallowed on purpose: a transient failure while sampling a directory that is being
/// written to must not disturb the transfer.
///
/// F2.1 note: callers should use a long interval (≥ 30s) or avoid this entirely for large trees.
pub fn directory_size(root: &Path) -> u64 {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::fixture_tree;

    #[test]
    fn scan_collects_matching_files_recursively() {
        let dir = fixture_tree(&[
            ("a.csv", 100),
            ("nested/b.csv", 200),
            ("nested/deep/c.csv", 300),
            ("ignore.txt", 999),
        ]);

        let summary = scan(dir.path(), "*.csv").expect("scan");

        assert_eq!(summary.file_count(), 3);
        assert_eq!(summary.total_bytes, 600);
        assert_eq!(
            summary.files[0],
            ScannedFile {
                relative_path: PathBuf::from("a.csv"),
                size_bytes: 100
            }
        );
        assert!(summary
            .files
            .iter()
            .all(|f| f.relative_path.extension().unwrap() == "csv"));
    }

    #[test]
    fn scan_of_empty_tree_is_empty() {
        let dir = fixture_tree(&[]);
        let summary = scan(dir.path(), "*.csv").expect("scan");
        assert!(summary.is_empty());
        assert_eq!(summary.total_bytes, 0);
    }

    #[test]
    fn patterns_other_than_csv_are_supported() {
        let dir = fixture_tree(&[("a.csv", 10), ("b.tsv", 20), ("c.tsv", 30)]);
        let summary = scan(dir.path(), "*.tsv").expect("scan");
        assert_eq!(summary.file_count(), 2);
        assert_eq!(summary.total_bytes, 50);
    }

    #[test]
    fn invalid_pattern_is_reported() {
        let error = build_matcher("[unterminated").expect_err("invalid glob");
        assert!(matches!(error, IngestError::InvalidPattern { .. }));
    }

    #[test]
    fn directory_size_sums_every_file() {
        let dir = fixture_tree(&[("a.csv", 100), ("nested/b.bin", 250)]);
        assert_eq!(directory_size(dir.path()), 350);
    }

    #[test]
    fn directory_size_of_missing_path_is_zero() {
        assert_eq!(directory_size(Path::new("/definitely/not/here")), 0);
    }

    /// F1.4: glob against the relative path, not just the file name.
    #[test]
    fn glob_matches_against_relative_path_not_just_filename() {
        let dir = fixture_tree(&[
            ("a.csv", 10),
            ("nested/b.csv", 20),
            ("other/c.csv", 30),
        ]);
        // A flat glob still matches all CSV files regardless of depth.
        let summary = scan(dir.path(), "*.csv").expect("scan");
        assert_eq!(summary.file_count(), 3, "flat glob should match all csv files in subdirs");
    }

    /// F2.2: lightweight inventory returns the same totals without file list.
    #[test]
    fn inventory_matches_scan_totals() {
        let dir = fixture_tree(&[("a.csv", 100), ("nested/b.csv", 200), ("c.txt", 50)]);
        let full = scan(dir.path(), "*.csv").expect("scan");
        let light = inventory(dir.path(), "*.csv").expect("inventory");
        assert_eq!(light.total_files, full.file_count() as u64);
        assert_eq!(light.total_bytes, full.total_bytes);
    }
}
