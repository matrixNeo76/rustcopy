//! Incremental Deduplication and State Cache manager for robocopy-ingest-cli.
//!
//! Stores file path, file size, and last modified timestamps in a `.ingest_cache`
//! state file to skip redundant transfers and hashing on unchanged files.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileCacheEntry {
    pub file_size: u64,
    pub modified_timestamp: u64,
    pub hash_checksum: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestCache {
    pub entries: HashMap<String, FileCacheEntry>,
}

impl IngestCache {
    pub fn load_from(path: &Path) -> Self {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(cache) = serde_json::from_str(&content) {
                return cache;
            }
        }
        Self::default()
    }

    /// D14: writes via `crate::atomic_write` (temp file + rename) rather than a bare `fs::write`,
    /// so a crash or interrupted write mid-save can never leave a corrupt `.ingest_cache` at
    /// `path` — a corrupt cache doesn't fail a run outright (`load_from` silently falls back to
    /// `Self::default()` on a parse error), but it silently discards every previously-verified
    /// file's fast-verify trust, forcing a full re-verification on the next run for no reason.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = serde_json::to_string_pretty(self)?;
        crate::atomic_write(path, content.as_bytes())
    }

    pub fn should_skip(&self, relative_path: &str, current_size: u64, current_mtime: u64) -> bool {
        if let Some(entry) = self.entries.get(relative_path) {
            entry.file_size == current_size && entry.modified_timestamp == current_mtime
        } else {
            false
        }
    }

    pub fn update(&mut self, relative_path: String, size: u64, mtime: u64, hash: Option<String>) {
        self.entries.insert(
            relative_path,
            FileCacheEntry {
                file_size: size,
                modified_timestamp: mtime,
                hash_checksum: hash,
            },
        );
    }
}

/// F28: the cache lives next to the destination it describes (`<dest>/.ingest_cache`), not the
/// current working directory — a fixed CWD-relative path would collide between unrelated backup
/// jobs and wouldn't survive being run from a different directory.
///
/// D12: `job_name` namespaces the filename (via [`crate::namespaced_path`]) for one job in a F33
/// `[[jobs]]` batch, so two jobs sharing the same `dest` don't silently read/write each other's
/// fast-verify cache. `None` (the single-job path) keeps the plain `.ingest_cache` filename.
pub fn default_cache_path(dest: &Path, job_name: Option<&str>) -> PathBuf {
    let base = dest.join(".ingest_cache");
    match job_name {
        Some(name) => crate::namespaced_path(&base, name),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_skips_unchanged_files() {
        let mut cache = IngestCache::default();
        cache.update("file1.csv".to_string(), 100, 12345, None);

        assert!(cache.should_skip("file1.csv", 100, 12345));
        assert!(!cache.should_skip("file1.csv", 200, 12345));
        assert!(!cache.should_skip("file2.csv", 100, 12345));
    }

    #[test]
    fn cache_round_trips_through_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".ingest_cache");
        let mut cache = IngestCache::default();
        cache.update("a.csv".to_string(), 100, 12345, None);
        cache.save_to(&path).expect("save");

        let loaded = IngestCache::load_from(&path);
        assert!(loaded.should_skip("a.csv", 100, 12345));
    }

    /// D14: `save_to` goes through `crate::atomic_write` (temp file + rename), not a bare
    /// `fs::write` — confirms the sibling `.rustcopy-tmp` file is actually cleaned up by the real
    /// `save_to` call, not just by `atomic_write`'s own unit tests in isolation.
    #[test]
    fn save_to_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".ingest_cache");
        let mut cache = IngestCache::default();
        cache.update("a.csv".to_string(), 100, 12345, None);
        cache.save_to(&path).expect("save");

        let tmp_path = dir.path().join(".ingest_cache.rustcopy-tmp");
        assert!(!tmp_path.exists(), "temp file must be renamed away, not left behind");
    }

    #[test]
    fn load_from_a_missing_file_yields_an_empty_cache() {
        let cache = IngestCache::load_from(Path::new("/definitely/not/here/.ingest_cache"));
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn default_cache_path_lives_next_to_the_destination() {
        let dest = Path::new("D:/warehouse");
        assert_eq!(default_cache_path(dest, None), PathBuf::from("D:/warehouse/.ingest_cache"));
    }

    #[test]
    fn default_cache_path_namespaces_by_job_name() {
        let dest = Path::new("D:/warehouse");
        assert_eq!(
            default_cache_path(dest, Some("photos")),
            PathBuf::from("D:/warehouse/.ingest_cache.photos")
        );
    }

    #[test]
    fn default_cache_path_differs_between_two_jobs_sharing_a_dest() {
        let dest = Path::new("D:/warehouse");
        assert_ne!(
            default_cache_path(dest, Some("photos")),
            default_cache_path(dest, Some("documents"))
        );
    }
}
