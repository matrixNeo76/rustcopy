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

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)
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

pub fn default_cache_path() -> PathBuf {
    PathBuf::from("./.ingest_cache")
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
}
