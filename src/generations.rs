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
    /// Copies every file that is new or changed (by size+mtime) since the *last Full* generation
    /// — always diffs against the same reference point regardless of how many differentials have
    /// run since, unlike `Incremental` which chains off the immediately preceding generation.
    /// Requires at least one prior `Full` generation to exist (an intervening `Incremental` does
    /// not count as the reference).
    Differential,
}

impl BackupType {
    pub fn as_str(self) -> &'static str {
        match self {
            BackupType::Full => "full",
            BackupType::Incremental => "incremental",
            BackupType::Differential => "differential",
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
    /// D12: `job_name` namespaces the filename (via [`crate::namespaced_path`]) for one job in a
    /// F33 `[[jobs]]` batch, so two jobs sharing the same `dest_root` don't merge their generation
    /// histories into one manifest — `latest()`/`latest_full()` have no source/job identity of
    /// their own (see the `Generation` doc comment), so a shared manifest would let one job's
    /// incremental/differential diff against another job's unrelated source tree, and
    /// `--keep-generations` could prune a still-needed `Full` generation from the wrong job's
    /// chain. `None` (the single-job path) keeps the plain `.rustcopy_generations.json` filename.
    pub fn path_for(dest_root: &Path, job_name: Option<&str>) -> PathBuf {
        let base = dest_root.join(MANIFEST_FILE_NAME);
        match job_name {
            Some(name) => crate::namespaced_path(&base, name),
            None => base,
        }
    }

    /// Loads the manifest from `<dest_root>/.rustcopy_generations.json` (or its namespaced
    /// variant, see [`Self::path_for`]), or an empty manifest if it doesn't exist yet (the
    /// destination's first-ever generation).
    pub fn load_or_default(dest_root: &Path, job_name: Option<&str>) -> Result<Self, IngestError> {
        let path = Self::path_for(dest_root, job_name);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path).map_err(|error| IngestError::io(&path, error))?;
        serde_json::from_str(&content).map_err(|error| {
            IngestError::io(&path, std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
    }

    pub fn save(&self, dest_root: &Path, job_name: Option<&str>) -> Result<(), IngestError> {
        let path = Self::path_for(dest_root, job_name);
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

    /// The most recently recorded `Full` generation — what a differential backup diffs against,
    /// regardless of how many `Incremental`/`Differential` generations ran since.
    pub fn latest_full(&self) -> Option<&Generation> {
        self.generations
            .iter()
            .rev()
            .find(|generation| generation.backup_type == BackupType::Full)
    }

    pub fn push(&mut self, generation: Generation) {
        self.generations.push(generation);
    }

    /// Groups the recorded generations into "cycles": a cycle starts at a `Full` generation and
    /// includes every `Incremental`/`Differential` generation that follows it, up to (but not
    /// including) the next `Full`. This is the unit F35 retention rotates by — rotating by raw
    /// generation instead risks deleting a `Full` that a later `Incremental`/`Differential` still
    /// depends on for restoration, orphaning the chain. Generations are expected to always start
    /// with a `Full` in practice (`Incremental`/`Differential` both refuse to run without a prior
    /// reference generation), but this doesn't assume that invariant: any generations preceding
    /// the first `Full` (if that ever happened) still form a leading pseudo-cycle rather than
    /// being silently dropped.
    pub fn cycles(&self) -> Vec<&[Generation]> {
        let mut result = Vec::new();
        let mut start = 0;
        for (i, generation) in self.generations.iter().enumerate() {
            if generation.backup_type == BackupType::Full && i != start {
                result.push(&self.generations[start..i]);
                start = i;
            }
        }
        if start < self.generations.len() {
            result.push(&self.generations[start..]);
        }
        result
    }

    /// The ids of every generation belonging to a cycle older than the `keep_cycles` most recent
    /// ones — what F35 retention should delete. Empty when there are `keep_cycles` cycles or
    /// fewer (nothing to prune yet).
    pub fn generations_to_prune(&self, keep_cycles: usize) -> Vec<String> {
        let cycles = self.cycles();
        if cycles.len() <= keep_cycles {
            return Vec::new();
        }
        let prune_count = cycles.len() - keep_cycles;
        cycles[..prune_count]
            .iter()
            .flat_map(|cycle| cycle.iter().map(|generation| generation.id.clone()))
            .collect()
    }

    /// Removes every generation whose id is in `ids`, in place — the manifest-side half of
    /// pruning (the caller is responsible for also deleting the corresponding `<dest>/<id>/`
    /// folders on disk).
    pub fn retain_generations(&mut self, ids: &std::collections::HashSet<String>) {
        self.generations.retain(|generation| !ids.contains(&generation.id));
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
        manifest.save(dir.path(), None).expect("save");
        let loaded = GenerationManifest::load_or_default(dir.path(), None).expect("load");
        assert_eq!(loaded, manifest);
        assert_eq!(loaded.latest().unwrap().backup_type, BackupType::Full);
    }

    #[test]
    fn missing_manifest_loads_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = GenerationManifest::load_or_default(dir.path(), None).expect("load");
        assert!(manifest.generations.is_empty());
        assert!(manifest.latest().is_none());
    }

    #[test]
    fn namespaced_manifest_does_not_collide_with_the_default_or_another_job() {
        let dir = tempfile::tempdir().expect("tempdir");

        let mut photos = GenerationManifest::default();
        photos.push(Generation {
            id: "20260804T000000000Z_full".to_string(),
            backup_type: BackupType::Full,
            created_at: "2026-08-04T00:00:00Z".to_string(),
            files_copied: 1,
            files: vec![generation_file("photo1.jpg", 10, 100)],
        });
        photos.save(dir.path(), Some("photos")).expect("save photos manifest");

        let mut documents = GenerationManifest::default();
        documents.push(Generation {
            id: "20260804T000001000Z_full".to_string(),
            backup_type: BackupType::Full,
            created_at: "2026-08-04T00:00:01Z".to_string(),
            files_copied: 1,
            files: vec![generation_file("doc1.pdf", 20, 200)],
        });
        documents.save(dir.path(), Some("documents")).expect("save documents manifest");

        // D12: each job's manifest round-trips independently...
        let loaded_photos = GenerationManifest::load_or_default(dir.path(), Some("photos")).expect("load photos");
        let loaded_documents =
            GenerationManifest::load_or_default(dir.path(), Some("documents")).expect("load documents");
        assert_eq!(loaded_photos, photos);
        assert_eq!(loaded_documents, documents);
        assert_ne!(loaded_photos, loaded_documents);

        // ...and neither wrote to (or is visible through) the unnamespaced default path.
        let default_manifest = GenerationManifest::load_or_default(dir.path(), None).expect("load default");
        assert!(default_manifest.generations.is_empty());
    }

    #[test]
    fn generation_ids_for_different_types_are_distinguishable() {
        assert!(new_generation_id(BackupType::Full).ends_with("_full"));
        assert!(new_generation_id(BackupType::Incremental).ends_with("_incremental"));
        assert!(new_generation_id(BackupType::Differential).ends_with("_differential"));
    }

    fn generation(id: &str, backup_type: BackupType) -> Generation {
        Generation {
            id: id.to_string(),
            backup_type,
            created_at: "2026-08-05T00:00:00Z".to_string(),
            files_copied: 0,
            files: vec![generation_file("a.csv", 10, 100)],
        }
    }

    #[test]
    fn latest_full_skips_incremental_and_differential_generations() {
        let mut manifest = GenerationManifest::default();
        manifest.push(generation("1_full", BackupType::Full));
        manifest.push(generation("2_incremental", BackupType::Incremental));
        manifest.push(generation("3_differential", BackupType::Differential));
        manifest.push(generation("4_incremental", BackupType::Incremental));

        let latest_full = manifest.latest_full().expect("a full generation exists");
        assert_eq!(latest_full.id, "1_full");
        // `latest()` still returns the truly most recent generation regardless of type.
        assert_eq!(manifest.latest().unwrap().id, "4_incremental");
    }

    #[test]
    fn latest_full_picks_the_most_recent_full_when_several_exist() {
        let mut manifest = GenerationManifest::default();
        manifest.push(generation("1_full", BackupType::Full));
        manifest.push(generation("2_differential", BackupType::Differential));
        manifest.push(generation("3_full", BackupType::Full));
        manifest.push(generation("4_differential", BackupType::Differential));

        assert_eq!(manifest.latest_full().unwrap().id, "3_full");
    }

    #[test]
    fn latest_full_is_none_when_only_non_full_generations_exist() {
        let mut manifest = GenerationManifest::default();
        manifest.push(generation("1_incremental", BackupType::Incremental));
        assert!(manifest.latest_full().is_none());
    }

    fn manifest_with(ids_and_types: &[(&str, BackupType)]) -> GenerationManifest {
        let mut manifest = GenerationManifest::default();
        for (id, backup_type) in ids_and_types {
            manifest.push(generation(id, *backup_type));
        }
        manifest
    }

    #[test]
    fn cycles_groups_full_plus_its_following_incremental_and_differential() {
        use BackupType::*;
        let manifest = manifest_with(&[
            ("1_full", Full),
            ("2_incremental", Incremental),
            ("3_differential", Differential),
            ("4_full", Full),
            ("5_incremental", Incremental),
        ]);

        let cycles = manifest.cycles();
        assert_eq!(cycles.len(), 2);
        let first_ids: Vec<_> = cycles[0].iter().map(|g| g.id.as_str()).collect();
        assert_eq!(first_ids, vec!["1_full", "2_incremental", "3_differential"]);
        let second_ids: Vec<_> = cycles[1].iter().map(|g| g.id.as_str()).collect();
        assert_eq!(second_ids, vec!["4_full", "5_incremental"]);
    }

    #[test]
    fn generations_to_prune_is_empty_when_cycle_count_is_within_the_keep_limit() {
        use BackupType::*;
        let manifest = manifest_with(&[("1_full", Full), ("2_incremental", Incremental)]);
        assert!(manifest.generations_to_prune(1).is_empty());
        assert!(manifest.generations_to_prune(5).is_empty());
    }

    #[test]
    fn generations_to_prune_returns_every_generation_in_older_cycles_only() {
        use BackupType::*;
        let manifest = manifest_with(&[
            ("1_full", Full),
            ("2_incremental", Incremental),
            ("3_full", Full),
            ("4_differential", Differential),
            ("5_full", Full),
        ]);

        // Keep the 2 most recent cycles (3_full+4_differential, and 5_full): only the first
        // cycle (1_full+2_incremental) is old enough to prune.
        let mut to_prune = manifest.generations_to_prune(2);
        to_prune.sort();
        assert_eq!(to_prune, vec!["1_full".to_string(), "2_incremental".to_string()]);
    }

    #[test]
    fn retain_generations_removes_only_the_pruned_ids() {
        use BackupType::*;
        let mut manifest = manifest_with(&[
            ("1_full", Full),
            ("2_incremental", Incremental),
            ("3_full", Full),
        ]);

        let ids: std::collections::HashSet<String> =
            ["1_full".to_string(), "2_incremental".to_string()].into_iter().collect();
        manifest.retain_generations(&ids);

        let remaining: Vec<_> = manifest.generations.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(remaining, vec!["3_full"]);
    }
}
