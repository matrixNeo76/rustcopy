//! F34: backup generations (full / incremental).
//!
//! The central Cobian-parity concept this crate was missing entirely before F34: a persisted
//! history of past backups at a destination, so a later run can answer "what changed since the
//! last backup" without re-hashing everything. Deliberately **not** just a robocopy flag —
//! robocopy's own same-destination diffing (skip files whose size+timestamp already match)
//! overwrites in place and keeps no history, so there is nothing to roll back to and nothing for
//! a future retention policy (F35) to rotate. Each generation instead gets its own destination
//! subfolder (`<dest>/<id>/`), and the manifest at `<dest>/.rustcopy_generations.json` records,
//! for every past generation, the full source file listing (relative path + size + mtime) at the
//! time it ran — not just what was actually copied that time — so the *next* incremental always
//! diffs against a complete picture of "what the source looked like last time", not merely
//! against the previous delta.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::IngestError;
use crate::scan::ScannedFile;

/// Manifest file name, stored directly under the destination root (not inside any generation
/// subfolder, since it must survive and be readable independently of any single generation).
pub const MANIFEST_FILE_NAME: &str = ".rustcopy_generations.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum BackupType {
    /// Copies every file matching the pattern into a new generation folder.
    Full,
    /// Copies only files that are new or changed (by size+mtime) since the *immediately
    /// preceding* generation (full or incremental) into a new generation folder. Requires at
    /// least one prior generation to exist.
    Incremental,
}

impl BackupType {
    pub fn as_str(self) -> &'static str {
        match self {
            BackupType::Full => "full",
            BackupType::Incremental => "incremental",
        }
    }
}

/// A file as it existed (by size+mtime) at the time a given generation ran. Same trust model as
/// `IngestCache` (F28): fast size+mtime comparison, not a re-hash — good enough to decide "did
/// this need copying", not a substitute for `--verify-integrity`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationFile {
    pub relative_path: PathBuf,
    pub size_bytes: u64,
    pub modified_timestamp: u64,
}

/// One completed backup run at a destination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Generation {
    /// Also the name of the destination subfolder this generation's files were written to
    /// (`<dest>/<id>/`), and unique within a manifest by construction (timestamp-based).
    pub id: String,
    pub backup_type: BackupType,
    /// RFC 3339 timestamp of when this generation was recorded.
    pub created_at: String,
    /// How many files were actually copied to disk for this generation (the delta, for
    /// incremental — not necessarily `files.len()`, which is the *full* source snapshot).
    pub files_copied: usize,
    /// Full source inventory at the time this generation ran, used to diff the *next*
    /// generation against — not just the files actually copied this time.
    pub files: Vec<GenerationFile>,
}

/// Persisted history of every generation backed up to one destination.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GenerationManifest {
    pub generations: Vec<Generation>,
}

impl GenerationManifest {
    pub fn path_for(dest_root: &Path) -> PathBuf {
        dest_root.join(MANIFEST_FILE_NAME)
    }

    /// Loads the manifest from `<dest_root>/.rustcopy_generations.json`, or an empty manifest if
    /// it doesn't exist yet (the destination's first-ever generation).
    pub fn load_or_default(dest_root: &Path) -> Result<Self, IngestError> {
        let path = Self::path_for(dest_root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path).map_err(|error| IngestError::io(&path, error))?;
        serde_json::from_str(&content).map_err(|error| {
            IngestError::io(&path, std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
    }

    pub fn save(&self, dest_root: &Path) -> Result<(), IngestError> {
        let path = Self::path_for(dest_root);
        let json = serde_json::to_string_pretty(self).map_err(|error| {
            IngestError::io(&path, std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })?;
        std::fs::write(&path, json).map_err(|error| IngestError::io(&path, error))
    }

    /// The most recently recorded generation, regardless of type — what an incremental backup
    /// diffs against.
    pub fn latest(&self) -> Option<&Generation> {
        self.generations.last()
    }

    pub fn push(&mut self, generation: Generation) {
        self.generations.push(generation);
    }
}

/// A new, sortable generation id: `<UTC timestamp>_<type>`, e.g. `20260804T153000123Z_full`.
/// Millisecond precision keeps ids unique even for a script that runs several backups per second
/// (tests, mainly) without needing a counter or a lock file.
pub fn new_generation_id(backup_type: BackupType) -> String {
    format!(
        "{}_{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ"),
        backup_type.as_str()
    )
}

/// Files in `current` that are new or changed (by size or mtime) relative to `reference` — what
/// an incremental generation actually needs to copy. Order follows `current`.
pub fn changed_since<'a>(current: &'a [ScannedFile], reference: &[GenerationFile]) -> Vec<&'a ScannedFile> {
    let reference_by_path: HashMap<&Path, &GenerationFile> = reference
        .iter()
        .map(|file| (file.relative_path.as_path(), file))
        .collect();

    current
        .iter()
        .filter(|file| match reference_by_path.get(file.relative_path.as_path()) {
            None => true,
            Some(prior) => {
                prior.size_bytes != file.size_bytes
                    || prior.modified_timestamp != file.modified_timestamp
            }
        })
        .collect()
}

pub fn to_generation_files(files: &[ScannedFile]) -> Vec<GenerationFile> {
    files
        .iter()
        .map(|file| GenerationFile {
            relative_path: file.relative_path.clone(),
            size_bytes: file.size_bytes,
            modified_timestamp: file.modified_timestamp,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanned(path: &str, size: u64, mtime: u64) -> ScannedFile {
        ScannedFile {
            relative_path: PathBuf::from(path),
            size_bytes: size,
            modified_timestamp: mtime,
        }
    }

    fn generation_file(path: &str, size: u64, mtime: u64) -> GenerationFile {
        GenerationFile {
            relative_path: PathBuf::from(path),
            size_bytes: size,
            modified_timestamp: mtime,
        }
    }

    #[test]
    fn changed_since_flags_new_files() {
        let current = vec![scanned("a.csv", 10, 100), scanned("b.csv", 20, 200)];
        let reference = vec![generation_file("a.csv", 10, 100)];

        let changed = changed_since(&current, &reference);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].relative_path, PathBuf::from("b.csv"));
    }

    #[test]
    fn changed_since_flags_modified_files_by_size_or_mtime() {
        let current = vec![scanned("a.csv", 99, 100), scanned("b.csv", 20, 999)];
        let reference = vec![generation_file("a.csv", 10, 100), generation_file("b.csv", 20, 200)];

        let changed = changed_since(&current, &reference);
        let paths: Vec<_> = changed.iter().map(|f| f.relative_path.clone()).collect();
        assert_eq!(paths, vec![PathBuf::from("a.csv"), PathBuf::from("b.csv")]);
    }

    #[test]
    fn changed_since_is_empty_when_nothing_changed() {
        let current = vec![scanned("a.csv", 10, 100)];
        let reference = vec![generation_file("a.csv", 10, 100)];
        assert!(changed_since(&current, &reference).is_empty());
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let mut manifest = GenerationManifest::default();
        manifest.push(Generation {
            id: "20260804T000000000Z_full".to_string(),
            backup_type: BackupType::Full,
            created_at: "2026-08-04T00:00:00Z".to_string(),
            files_copied: 1,
            files: vec![generation_file("a.csv", 10, 100)],
        });

        let dir = tempfile::tempdir().expect("tempdir");
        manifest.save(dir.path()).expect("save");
        let loaded = GenerationManifest::load_or_default(dir.path()).expect("load");
        assert_eq!(loaded, manifest);
        assert_eq!(loaded.latest().unwrap().backup_type, BackupType::Full);
    }

    #[test]
    fn missing_manifest_loads_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = GenerationManifest::load_or_default(dir.path()).expect("load");
        assert!(manifest.generations.is_empty());
        assert!(manifest.latest().is_none());
    }

    #[test]
    fn generation_ids_for_different_types_are_distinguishable() {
        assert!(new_generation_id(BackupType::Full).ends_with("_full"));
        assert!(new_generation_id(BackupType::Incremental).ends_with("_incremental"));
    }
}
