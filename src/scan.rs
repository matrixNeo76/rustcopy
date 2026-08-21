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
use std::time::{SystemTime, UNIX_EPOCH};

use globset::{GlobBuilder, GlobMatcher};
use walkdir::WalkDir;

use crate::errors::IngestError;

/// One matched source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Path relative to the scan root, so it can be joined onto any destination.
    pub relative_path: PathBuf,
    pub size_bytes: u64,
    /// F28: modification time as Unix seconds (`0` if the filesystem didn't report one). Used by
    /// `--fast-verify` to decide, together with `size_bytes`, whether a file has changed since
    /// the last `.ingest_cache`-backed run and therefore needs re-hashing.
    pub modified_timestamp: u64,
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

/// F28: extract `(size_bytes, modified_timestamp)` from filesystem metadata in one place, so
/// `scan`/`inventory` agree on how a missing/unavailable mtime is represented (`0`, rather than
/// failing the whole entry — unlike a missing size, an unknown mtime just means `--fast-verify`
/// treats the file as always-changed, which is safe, merely non-optimal).
fn size_bytes_and_mtime(metadata: &std::fs::Metadata) -> (u64, u64) {
    let modified_timestamp = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (metadata.len(), modified_timestamp)
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

/// Compiles `--exclude-dirs`/`--exclude-files` patterns the same way [`build_matcher`] compiles
/// the main `--pattern` (case-insensitive on Windows), for matching against a bare entry name
/// (see [`is_excluded`]).
fn build_exclude_matchers(patterns: &[String]) -> Result<Vec<GlobMatcher>, IngestError> {
    patterns.iter().map(|p| build_matcher(p)).collect()
}

/// Whether `name` (a directory or file's own bare name, not a full/relative path) matches any of
/// `matchers` — mirrors robocopy's own `/XD`/`/XF` semantics, which exclude by name at any depth
/// in the tree, not just a specific path.
fn is_excluded(name: &std::ffi::OsStr, matchers: &[GlobMatcher]) -> bool {
    matchers.iter().any(|m| m.is_match(Path::new(name)))
}

/// D17: whether a file's `modified_timestamp` fails `--min-age-days`/`--max-age-days`, matching
/// real `robocopy.exe`'s own `/MINAGE`/`/MAXAGE` semantics — verified empirically against the
/// real binary, not assumed from docs (this crate's own `--help` text had the two directions
/// backwards until this fix, see `CLAUDE.md`): `/MINAGE:N` excludes files modified **less** than
/// N days ago (only files at least N days old survive); `/MAXAGE:N` excludes files modified
/// **more** than N days ago (only files within the last N days survive). `now` is a parameter
/// rather than an internal `SystemTime::now()` call so this stays unit-testable without mocking
/// the clock, same pattern as `resolve_report_path_timestamp` in `lib.rs`.
fn fails_age_filter(
    modified_timestamp: u64,
    now_unix_secs: u64,
    min_age_days: Option<u32>,
    max_age_days: Option<u32>,
) -> bool {
    const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
    let age_days = now_unix_secs.saturating_sub(modified_timestamp) / SECONDS_PER_DAY;
    if let Some(min_age) = min_age_days {
        if age_days < u64::from(min_age) {
            return true;
        }
    }
    if let Some(max_age) = max_age_days {
        if age_days > u64::from(max_age) {
            return true;
        }
    }
    false
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Walk `root` recursively and collect every file whose **relative path** matches `pattern`.
///
/// F1.4: matching is done on the relative path (e.g. `subdir/file.csv`) instead of the bare
/// filename, so patterns like `nested/*.csv` or `**/*.csv` resolve correctly.
///
/// Unreadable entries are logged and skipped rather than aborting a long ingestion.
///
/// F26d: `follow_links` must be driven from the same source as the copy engine's
/// `CopyRequest::exclude_junctions` (i.e. `!exclude_junctions`). Robocopy's own default
/// (`exclude_junctions == false`, no `/XJ`) is to follow junction points and symlinked
/// directories; before this parameter existed, `scan.rs` always used `follow_links(false)`
/// regardless, so the prescan/verification walked a different tree than what robocopy actually
/// copied (D7). `WalkDir` detects and rejects cycles when following links, so a self-referencing
/// junction errors out per-entry (logged and skipped) instead of recursing forever.
///
/// `exclude_dirs`/`exclude_files` were previously accepted by the CLI/config but only ever
/// applied to the real `robocopy.exe` invocation's own `/XD`/`/XF` flags (`engine::robocopy`) —
/// this scan (used for the progress-bar total and for `--verify-integrity`'s file list) walked
/// straight through excluded directories regardless, which had two real consequences: (1) the
/// prescan wasted time reading metadata for potentially huge excluded trees (e.g. `AppData`) it
/// was always going to discard, and (2) `--verify-integrity` would compare against a file list
/// that includes files robocopy never actually copied (because it excluded them), reporting
/// every one of them as `missing_in_dest` — a spurious `FAILED` status for an exclusion that
/// worked exactly as intended on the real transfer. Directories are pruned via `filter_entry`
/// (skips the whole subtree, not just a post-hoc filter — matches robocopy's own behaviour of
/// never descending into an excluded directory) rather than filtered after the fact, so this
/// fixes the wasted-scan-time half of the problem too, not just the false-FAILED half.
///
/// D17: `min_age_days`/`max_age_days` had the identical structural gap as D11's `exclude_dirs`/
/// `exclude_files` — accepted by the CLI, applied to the real `robocopy.exe` transfer, but never
/// threaded into this scan, so `--verify-integrity` reported age-filtered files as spuriously
/// `missing_in_dest` and `--backup-type` (which never goes through robocopy, see `AGENTS.md` rule
/// 9) ignored the flags entirely. See [`fails_age_filter`] for the exact semantics.
pub fn scan(
    root: &Path,
    pattern: &str,
    follow_links: bool,
    exclude_dirs: &[String],
    exclude_files: &[String],
    min_age_days: Option<u32>,
    max_age_days: Option<u32>,
) -> Result<ScanSummary, IngestError> {
    let matcher = build_matcher(pattern)?;
    let dir_matchers = build_exclude_matchers(exclude_dirs)?;
    let file_matchers = build_exclude_matchers(exclude_files)?;
    let now = now_unix_secs();
    let mut summary = ScanSummary::default();

    let walker = WalkDir::new(root)
        .follow_links(follow_links)
        .into_iter()
        .filter_entry(|entry| {
            // Never prune the root itself (depth 0) - only subdirectories can be excluded.
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !is_excluded(entry.file_name(), &dir_matchers)
        });

    for entry in walker {
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
        if is_excluded(entry.file_name(), &file_matchers) {
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

        let (size_bytes, modified_timestamp) = match entry.metadata() {
            Ok(metadata) => size_bytes_and_mtime(&metadata),
            Err(error) => {
                tracing::warn!(path = %entry.path().display(), "skipping file without metadata: {error}");
                continue;
            }
        };

        if fails_age_filter(modified_timestamp, now, min_age_days, max_age_days) {
            continue;
        }

        summary.total_bytes += size_bytes;
        summary.files.push(ScannedFile {
            relative_path,
            size_bytes,
            modified_timestamp,
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
///
/// F26d: see [`scan`] for the meaning of `follow_links`. See [`scan`]'s doc comment for why
/// `exclude_dirs`/`exclude_files` (and, since D17, `min_age_days`/`max_age_days`) matter here
/// too, not just for the real `robocopy.exe` transfer.
pub fn inventory(
    root: &Path,
    pattern: &str,
    follow_links: bool,
    exclude_dirs: &[String],
    exclude_files: &[String],
    min_age_days: Option<u32>,
    max_age_days: Option<u32>,
) -> Result<InventorySummary, IngestError> {
    let matcher = build_matcher(pattern)?;
    let dir_matchers = build_exclude_matchers(exclude_dirs)?;
    let file_matchers = build_exclude_matchers(exclude_files)?;
    let now = now_unix_secs();
    let mut total_files = 0u64;
    let mut total_bytes = 0u64;

    let walker = WalkDir::new(root)
        .follow_links(follow_links)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !is_excluded(entry.file_name(), &dir_matchers)
        });

    for entry in walker {
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
        if is_excluded(entry.file_name(), &file_matchers) {
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
            let (size_bytes, modified_timestamp) = size_bytes_and_mtime(&meta);
            if fails_age_filter(modified_timestamp, now, min_age_days, max_age_days) {
                continue;
            }
            total_files += 1;
            total_bytes += size_bytes;
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
///
/// F26d: see [`scan`] for the meaning of `follow_links`.
pub fn directory_size(root: &Path, follow_links: bool) -> u64 {
    WalkDir::new(root)
        .follow_links(follow_links)
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

        let summary = scan(dir.path(), "*.csv", false, &[], &[], None, None).expect("scan");

        assert_eq!(summary.file_count(), 3);
        assert_eq!(summary.total_bytes, 600);
        assert_eq!(summary.files[0].relative_path, PathBuf::from("a.csv"));
        assert_eq!(summary.files[0].size_bytes, 100);
        assert!(
            summary.files[0].modified_timestamp > 0,
            "a freshly written file must have a real mtime"
        );
        assert!(summary
            .files
            .iter()
            .all(|f| f.relative_path.extension().unwrap() == "csv"));
    }

    #[test]
    fn scan_of_empty_tree_is_empty() {
        let dir = fixture_tree(&[]);
        let summary = scan(dir.path(), "*.csv", false, &[], &[], None, None).expect("scan");
        assert!(summary.is_empty());
        assert_eq!(summary.total_bytes, 0);
    }

    #[test]
    fn patterns_other_than_csv_are_supported() {
        let dir = fixture_tree(&[("a.csv", 10), ("b.tsv", 20), ("c.tsv", 30)]);
        let summary = scan(dir.path(), "*.tsv", false, &[], &[], None, None).expect("scan");
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
        assert_eq!(directory_size(dir.path(), false), 350);
    }

    #[test]
    fn directory_size_of_missing_path_is_zero() {
        assert_eq!(directory_size(Path::new("/definitely/not/here"), false), 0);
    }

    /// F1.4: glob against the relative path, not just the file name.
    #[test]
    fn glob_matches_against_relative_path_not_just_filename() {
        let dir = fixture_tree(&[("a.csv", 10), ("nested/b.csv", 20), ("other/c.csv", 30)]);
        // A flat glob still matches all CSV files regardless of depth.
        let summary = scan(dir.path(), "*.csv", false, &[], &[], None, None).expect("scan");
        assert_eq!(
            summary.file_count(),
            3,
            "flat glob should match all csv files in subdirs"
        );
    }

    /// F2.2: lightweight inventory returns the same totals without file list.
    #[test]
    fn inventory_matches_scan_totals() {
        let dir = fixture_tree(&[("a.csv", 100), ("nested/b.csv", 200), ("c.txt", 50)]);
        let full = scan(dir.path(), "*.csv", false, &[], &[], None, None).expect("scan");
        let light = inventory(dir.path(), "*.csv", false, &[], &[], None, None).expect("inventory");
        assert_eq!(light.total_files, full.file_count() as u64);
        assert_eq!(light.total_bytes, full.total_bytes);
    }

    /// F26d: with `follow_links: false` (robocopy `/XJ`, the safe/opt-in-exclude case), a
    /// directory symlink is not descended into — matches the pre-F26d behaviour exactly.
    #[cfg(unix)]
    #[test]
    fn follow_links_false_does_not_descend_into_a_symlinked_directory() {
        let dir = fixture_tree(&[("real/a.csv", 10)]);
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path().join("real"), &link).expect("create symlink");

        let summary = scan(dir.path(), "*.csv", false, &[], &[], None, None).expect("scan");
        assert_eq!(
            summary.file_count(),
            1,
            "the symlinked copy must not be counted"
        );
    }

    /// F26d: with `follow_links: true` (robocopy's own default, no `/XJ`), the walk follows a
    /// directory symlink and sees files through it too.
    #[cfg(unix)]
    #[test]
    fn follow_links_true_descends_into_a_symlinked_directory() {
        let dir = fixture_tree(&[("real/a.csv", 10)]);
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path().join("real"), &link).expect("create symlink");

        let summary = scan(dir.path(), "*.csv", true, &[], &[], None, None).expect("scan");
        assert_eq!(
            summary.file_count(),
            2,
            "a.csv must be counted once directly and once through the symlink"
        );
    }

    /// F26d, Windows-specific: a real NTFS directory junction (created with `mklink /J`, which —
    /// unlike symlinks — needs no elevated privilege) behaves the same as the Unix symlink cases
    /// above: followed only when `follow_links: true`. This is what `--exclude-junctions`
    /// ultimately controls end-to-end, verified against a real reparse point rather than assumed
    /// from `walkdir`'s documentation.
    #[cfg(windows)]
    #[test]
    fn windows_junction_is_followed_only_when_follow_links_is_true() {
        let dir = fixture_tree(&[("real/a.csv", 10)]);
        let target = dir.path().join("real");
        let link = dir.path().join("link");

        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &link.display().to_string(),
                &target.display().to_string(),
            ])
            .status()
            .expect("run mklink");
        assert!(
            status.success(),
            "mklink /J must succeed to exercise this test"
        );

        let excluded = scan(dir.path(), "*.csv", false, &[], &[], None, None)
            .expect("scan without following junctions");
        assert_eq!(
            excluded.file_count(),
            1,
            "with follow_links=false (--exclude-junctions), the junction must not be descended into"
        );

        let followed = scan(dir.path(), "*.csv", true, &[], &[], None, None)
            .expect("scan following junctions");
        assert_eq!(
            followed.file_count(),
            2,
            "with follow_links=true (robocopy's own default), a.csv is seen once directly and \
             once through the junction"
        );
    }

    /// Regression test: `exclude_dirs` must actually prune the excluded subtree from the scan
    /// (matching what `robocopy.exe`'s own `/XD` does for the real transfer), not just leave it
    /// out of the *final* file list after walking it anyway. `deep/` contains an unreadable-glob
    /// trap file (a name that would panic a naive matcher) precisely so this test also proves the
    /// excluded subtree is never even walked, not merely filtered.
    #[test]
    fn exclude_dirs_prunes_the_subtree_entirely() {
        let dir = fixture_tree(&[
            ("a.csv", 10),
            ("keep/b.csv", 20),
            ("AppData/c.csv", 9999),
            ("AppData/deep/d.csv", 9999),
        ]);

        let summary = scan(
            dir.path(),
            "*.csv",
            false,
            &["AppData".to_string()],
            &[],
            None,
            None,
        )
        .expect("scan");

        assert_eq!(
            summary.file_count(),
            2,
            "AppData's files must not appear in the result"
        );
        assert_eq!(
            summary.total_bytes, 30,
            "AppData's bytes must not be counted either"
        );
        assert!(summary
            .files
            .iter()
            .all(|f| !f.relative_path.starts_with("AppData")));
    }

    /// `exclude_dirs` matches by bare directory name at any depth, mirroring robocopy's own
    /// `/XD` semantics - not just at the scan root.
    #[test]
    fn exclude_dirs_matches_at_any_depth() {
        let dir = fixture_tree(&[
            ("a.csv", 10),
            ("nested/.git/config.csv", 20),
            ("nested/real.csv", 30),
        ]);

        let summary = scan(
            dir.path(),
            "*.csv",
            false,
            &[".git".to_string()],
            &[],
            None,
            None,
        )
        .expect("scan");

        assert_eq!(summary.file_count(), 2);
        assert!(summary
            .files
            .iter()
            .all(|f| !f.relative_path.to_string_lossy().contains(".git")));
    }

    /// `exclude_files` matches by bare file name, independent of `--pattern`.
    #[test]
    fn exclude_files_removes_matching_names_regardless_of_directory() {
        let dir = fixture_tree(&[
            ("a.csv", 10),
            ("nested/thumbs.db.csv", 20), // matches --pattern *.csv but should still be excluded
            ("nested/b.csv", 30),
        ]);

        let summary = scan(
            dir.path(),
            "*.csv",
            false,
            &[],
            &["thumbs.db.csv".to_string()],
            None,
            None,
        )
        .expect("scan");

        assert_eq!(summary.file_count(), 2);
        assert!(summary
            .files
            .iter()
            .all(|f| f.relative_path.file_name().unwrap() != "thumbs.db.csv"));
    }

    /// `inventory` (the `--no-prescan` lightweight walk) must apply the same exclusions as
    /// `scan`, not just materialise a shorter list from the same (wrongly) full walk.
    #[test]
    fn inventory_also_respects_exclude_dirs() {
        let dir = fixture_tree(&[("a.csv", 10), ("AppData/b.csv", 9999)]);

        let light = inventory(
            dir.path(),
            "*.csv",
            false,
            &["AppData".to_string()],
            &[],
            None,
            None,
        )
        .expect("inventory");

        assert_eq!(light.total_files, 1);
        assert_eq!(light.total_bytes, 10);
    }

    /// D17: `--min-age-days N` excludes files modified **less** than N days ago, matching real
    /// `robocopy.exe`'s `/MINAGE:N` — verified empirically against the real binary (see
    /// `CLAUDE.md`), the opposite of what this crate's own `--help` text said before this fix.
    #[test]
    fn min_age_days_excludes_files_younger_than_the_threshold() {
        let now = 2_000_000_000u64;
        let ten_days_ago = now - 10 * 24 * 60 * 60;
        let one_day_ago = now - 1 * 24 * 60 * 60;

        assert!(
            !fails_age_filter(ten_days_ago, now, Some(5), None),
            "a file 10 days old must survive --min-age-days 5"
        );
        assert!(
            fails_age_filter(one_day_ago, now, Some(5), None),
            "a file 1 day old must be excluded by --min-age-days 5"
        );
    }

    /// D17: `--max-age-days N` excludes files modified **more** than N days ago, matching real
    /// `robocopy.exe`'s `/MAXAGE:N`.
    #[test]
    fn max_age_days_excludes_files_older_than_the_threshold() {
        let now = 2_000_000_000u64;
        let ten_days_ago = now - 10 * 24 * 60 * 60;
        let one_day_ago = now - 1 * 24 * 60 * 60;

        assert!(
            fails_age_filter(ten_days_ago, now, None, Some(5)),
            "a file 10 days old must be excluded by --max-age-days 5"
        );
        assert!(
            !fails_age_filter(one_day_ago, now, None, Some(5)),
            "a file 1 day old must survive --max-age-days 5"
        );
    }

    /// A file exactly at the N-day boundary is inclusive on both sides (`<`/`>`, not `<=`/`>=`),
    /// matching robocopy's own boundary behaviour.
    #[test]
    fn age_filters_are_inclusive_at_the_exact_boundary() {
        let now = 2_000_000_000u64;
        let exactly_five_days_ago = now - 5 * 24 * 60 * 60;

        assert!(!fails_age_filter(exactly_five_days_ago, now, Some(5), None));
        assert!(!fails_age_filter(exactly_five_days_ago, now, None, Some(5)));
    }

    /// `--min-age-days`/`--max-age-days` given together narrow to a window, same as passing both
    /// `/MINAGE`/`/MAXAGE` to robocopy at once.
    #[test]
    fn min_and_max_age_days_together_form_a_window() {
        let now = 2_000_000_000u64;
        let three_days_ago = now - 3 * 24 * 60 * 60;
        let twenty_days_ago = now - 20 * 24 * 60 * 60;

        // window: files between 2 and 10 days old survive.
        assert!(
            !fails_age_filter(three_days_ago, now, Some(2), Some(10)),
            "3 days old is inside the [2, 10] window"
        );
        assert!(
            fails_age_filter(twenty_days_ago, now, Some(2), Some(10)),
            "20 days old is outside the [2, 10] window"
        );
    }
}
