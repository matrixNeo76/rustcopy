//! Restoring the state of a generation backup (`--restore-generation`).
//!
//! A generation backup (`--backup-type`, F34) keeps one folder per run, `<dest>/<id>/`, holding
//! only what that run copied: a `Full` has everything, an `Incremental` only what changed since
//! the run before it, a `Differential` only what changed since the last `Full`. Nothing in a single
//! folder is "the backup as it was": to get a state back, the folders have to be layered in the
//! order the runs depended on each other, and the files that were gone at that moment left out.
//! That used to be done by hand. `--restore-from` cannot do it: it reverses source and destination
//! of one report, and knows nothing of a chain.
//!
//! The two decisions that make a restore correct are pure and live here, away from the disk:
//!
//! * [`chain_for`]: which generations a state is built from. The chain follows how each type was
//!   *made*, not how it is named: an incremental was diffed against the generation before it, a
//!   differential against the last full one. Layering a differential over the incrementals that
//!   happen to sit between would be wrong: a file changed in one of them and put back by the time
//!   of the differential is not in the differential's folder, and the stale version would win.
//! * [`assign`]: which folder each file comes from. The manifest lists every file as it was at
//!   the restored generation; each is taken from the newest folder in the chain that physically
//!   holds a copy of the right size. A file in a folder but not in that listing was deleted from the
//!   source by then and is not restored; a listed file with no matching copy is reported, never
//!   guessed.
//!
//! Restoring never overwrites or deletes anything at the target (the CLI filters existing files out
//! before copying), and the target may not be inside the backup.

use std::collections::HashMap;
use std::path::{Component, Path};

use crate::errors::IngestError;
use crate::generations::{BackupType, GenerationFile, GenerationIndex, GenerationManifest};
use crate::scan::ScannedFile;

/// The word that selects the most recent generation.
pub const LATEST: &str = "latest";

fn refuse(reason: impl Into<String>) -> IngestError {
    IngestError::GenerationRestore(reason.into())
}

/// The positions (oldest first) of the generations a state is built from, `target` last.
///
/// `types` is the manifest in order. See the module documentation for why a differential is only
/// layered over its `Full`.
pub fn chain_for(types: &[BackupType], target: usize) -> Result<Vec<usize>, IngestError> {
    let Some(kind) = types.get(target) else {
        return Err(refuse("no such generation"));
    };
    match kind {
        BackupType::Full => Ok(vec![target]),
        BackupType::Differential => {
            let full = types[..target]
                .iter()
                .rposition(|t| *t == BackupType::Full)
                .ok_or_else(|| {
                    refuse("this differential generation has no full generation before it")
                })?;
            Ok(vec![full, target])
        }
        BackupType::Incremental => {
            if target == 0 {
                return Err(refuse(
                    "this incremental generation has no generation before it",
                ));
            }
            let mut chain = chain_for(types, target - 1)?;
            chain.push(target);
            Ok(chain)
        }
    }
}

/// What a restore would copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    /// The id of the restored generation.
    pub target_id: String,
    /// The ids layered to build it, oldest first.
    pub chain: Vec<String>,
    /// One entry per generation of the chain (same order): the files to take from that folder.
    pub steps: Vec<Vec<ScannedFile>>,
    /// Files the generation lists that no folder of the chain holds in the right size.
    pub missing: Vec<String>,
}

impl RestorePlan {
    pub fn total_files(&self) -> usize {
        self.steps.iter().map(Vec::len).sum()
    }

    pub fn total_bytes(&self) -> u64 {
        self.steps
            .iter()
            .flat_map(|files| files.iter())
            .map(|f| f.size_bytes)
            .sum()
    }
}

/// Assigns each file of `listing` to the newest folder that holds it. `stored[i]` is what folder
/// `i` of the chain physically contains. Returns, per folder, the files to take from it, and the
/// listed files nobody holds.
pub fn assign(
    listing: &[GenerationFile],
    stored: &[Vec<ScannedFile>],
) -> (Vec<Vec<ScannedFile>>, Vec<String>) {
    // Later folders overwrite earlier ones: the newest copy wins.
    let mut newest: HashMap<&Path, (usize, &ScannedFile)> = HashMap::new();
    for (position, files) in stored.iter().enumerate() {
        for file in files {
            newest.insert(file.relative_path.as_path(), (position, file));
        }
    }

    let mut steps: Vec<Vec<ScannedFile>> = vec![Vec::new(); stored.len()];
    let mut missing = Vec::new();
    for wanted in listing {
        match newest.get(wanted.relative_path.as_path()) {
            Some((position, held)) if held.size_bytes == wanted.size_bytes => {
                steps[*position].push((*held).clone());
            }
            _ => missing.push(wanted.relative_path.to_string_lossy().into_owned()),
        }
    }
    (steps, missing)
}

/// The position of the generation `wanted` names: [`LATEST`], or an id.
pub fn resolve_target(ids: &[String], wanted: &str) -> Result<usize, IngestError> {
    if ids.is_empty() {
        return Err(refuse("this destination holds no generation"));
    }
    if wanted.eq_ignore_ascii_case(LATEST) {
        return Ok(ids.len() - 1);
    }
    ids.iter().position(|id| id == wanted).ok_or_else(|| {
        refuse(format!(
            "no generation {wanted:?} here (--list-generations shows them)"
        ))
    })
}

/// Reads the manifest and the generation folders and builds the plan for `wanted`.
pub fn plan_restore(
    backup_root: &Path,
    job_name: Option<&str>,
    wanted: &str,
) -> Result<RestorePlan, IngestError> {
    let index = GenerationIndex::load(backup_root, job_name)?;
    let ids: Vec<String> = index.entries.iter().map(|e| e.id.clone()).collect();
    let types: Vec<BackupType> = index.entries.iter().map(|e| e.backup_type).collect();
    let target = resolve_target(&ids, wanted)?;
    let chain = chain_for(&types, target)?;

    let generation = GenerationManifest::load_generation(backup_root, job_name, &ids[target])?
        .ok_or_else(|| refuse("the manifest lost this generation while it was being read"))?;

    let mut stored = Vec::with_capacity(chain.len());
    for position in &chain {
        let folder = backup_root.join(&ids[*position]);
        if !folder.is_dir() {
            return Err(refuse(format!(
                "the folder of generation {} is missing, so {} cannot be rebuilt",
                ids[*position], ids[target]
            )));
        }
        let scanned = crate::scan::scan(&folder, "*", false, &[], &[], None, None)?;
        stored.push(scanned.files.to_vec());
    }

    let (steps, missing) = assign(&generation.files, &stored);
    Ok(RestorePlan {
        target_id: ids[target].clone(),
        chain: chain.iter().map(|p| ids[*p].clone()).collect(),
        steps,
        missing,
    })
}

/// `true` when `target` is `backup_root` or inside it. Compared on normalised components, ignoring
/// case: Windows paths are case-insensitive and `..`/`.` and mixed separators must not be a way
/// round the check. Restoring *into* the backup would mix restored files into generation folders.
pub fn target_is_inside_backup(backup_root: &Path, target: &Path) -> bool {
    let root = normalised(backup_root);
    let target = normalised(target);
    target.len() >= root.len() && target[..root.len()] == root[..]
}

fn normalised(path: &Path) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            other => parts.push(other.as_os_str().to_string_lossy().to_lowercase()),
        }
    }
    parts
}

/// What a finished restore means: the exit code. `copy_failed` is a copy error; `missing` the
/// files the generation lists that the backup cannot supply; `size_mismatches` restored files whose
/// size on disk is not the listed one.
pub fn exit_code(copy_failed: bool, missing: usize, size_mismatches: usize) -> u8 {
    if copy_failed {
        crate::runner::EXIT_INGESTION_PROBLEM
    } else if missing > 0 || size_mismatches > 0 {
        crate::runner::EXIT_INTEGRITY_FAILED
    } else {
        crate::runner::EXIT_SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use BackupType::{Differential as D, Full as F, Incremental as I};

    fn listed(path: &str, size: u64) -> GenerationFile {
        GenerationFile {
            relative_path: PathBuf::from(path),
            size_bytes: size,
            modified_timestamp: 0,
        }
    }

    fn held(path: &str, size: u64) -> ScannedFile {
        ScannedFile {
            relative_path: PathBuf::from(path),
            size_bytes: size,
            modified_timestamp: 0,
        }
    }

    #[test]
    fn a_full_generation_is_its_own_chain() {
        assert_eq!(chain_for(&[F, I, F], 2).unwrap(), vec![2]);
    }

    #[test]
    fn an_incremental_chain_runs_from_its_full_through_every_generation_between() {
        assert_eq!(chain_for(&[F, I, I, I], 3).unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(chain_for(&[F, I, F, I], 3).unwrap(), vec![2, 3]);
    }

    #[test]
    fn a_differential_is_layered_over_its_full_only() {
        // The incrementals between are not part of it: a file they changed and the differential
        // put back would otherwise keep its stale version.
        assert_eq!(chain_for(&[F, I, I, D], 3).unwrap(), vec![0, 3]);
    }

    #[test]
    fn an_incremental_after_a_differential_builds_on_that_differential() {
        assert_eq!(chain_for(&[F, D, I], 2).unwrap(), vec![0, 1, 2]);
        assert_eq!(chain_for(&[F, D, D, I], 3).unwrap(), vec![0, 2, 3]);
    }

    #[test]
    fn a_chain_with_no_full_at_its_root_is_refused() {
        assert!(chain_for(&[I], 0).is_err());
        assert!(chain_for(&[D], 0).is_err());
        assert!(chain_for(&[F], 5).is_err());
    }

    #[test]
    fn the_newest_copy_of_a_file_wins() {
        let listing = [listed("a.txt", 5), listed("b.txt", 3)];
        let stored = vec![
            vec![held("a.txt", 2), held("b.txt", 3)], // full
            vec![held("a.txt", 5)],                   // incremental changed a
        ];
        let (steps, missing) = assign(&listing, &stored);
        assert!(missing.is_empty());
        assert_eq!(steps[0].len(), 1, "only b comes from the full");
        assert_eq!(steps[0][0].relative_path, PathBuf::from("b.txt"));
        assert_eq!(steps[1][0].size_bytes, 5, "a comes from the incremental");
    }

    #[test]
    fn a_file_deleted_from_the_source_by_then_is_not_restored() {
        let listing = [listed("keep.txt", 1)];
        let stored = vec![vec![held("keep.txt", 1), held("gone.txt", 9)]];
        let (steps, missing) = assign(&listing, &stored);
        assert!(missing.is_empty());
        assert_eq!(steps[0].len(), 1);
        assert_eq!(steps[0][0].relative_path, PathBuf::from("keep.txt"));
    }

    #[test]
    fn a_listed_file_nobody_holds_or_held_in_the_wrong_size_is_reported_not_guessed() {
        let listing = [listed("lost.txt", 4), listed("wrong.txt", 4)];
        let stored = vec![vec![held("wrong.txt", 99)]];
        let (steps, missing) = assign(&listing, &stored);
        assert!(steps[0].is_empty());
        assert_eq!(missing, vec!["lost.txt", "wrong.txt"]);
    }

    #[test]
    fn latest_and_ids_resolve_and_anything_else_is_refused() {
        let ids = vec!["g1".to_string(), "g2".to_string()];
        assert_eq!(resolve_target(&ids, "latest").unwrap(), 1);
        assert_eq!(resolve_target(&ids, "LATEST").unwrap(), 1);
        assert_eq!(resolve_target(&ids, "g1").unwrap(), 0);
        assert!(resolve_target(&ids, "g3").is_err());
        assert!(resolve_target(&[], "latest").is_err());
    }

    #[test]
    fn a_target_inside_or_equal_to_the_backup_is_refused_whatever_the_spelling() {
        let root = Path::new("D:/Backup/job");
        assert!(target_is_inside_backup(root, Path::new("D:/Backup/job")));
        assert!(target_is_inside_backup(
            root,
            Path::new("d:/backup/JOB/restore")
        ));
        assert!(target_is_inside_backup(
            root,
            Path::new("D:/Backup/other/../job/x")
        ));
        assert!(!target_is_inside_backup(root, Path::new("D:/Backup/jobs")));
        assert!(!target_is_inside_backup(root, Path::new("D:/Backup")));
        assert!(!target_is_inside_backup(root, Path::new("E:/Backup/job")));
    }

    #[cfg(windows)]
    #[test]
    fn backslashes_and_mixed_separators_are_the_same_place() {
        let root = Path::new(r"D:\Backup\job");
        assert!(target_is_inside_backup(root, Path::new(r"D:\Backup/job\x")));
        assert!(!target_is_inside_backup(root, Path::new(r"D:\Backup\jobs")));
    }

    #[test]
    fn the_exit_code_separates_a_failed_copy_from_a_backup_that_cannot_supply_everything() {
        assert_eq!(exit_code(false, 0, 0), 0);
        assert_eq!(exit_code(true, 5, 0), crate::runner::EXIT_INGESTION_PROBLEM);
        assert_eq!(exit_code(false, 1, 0), crate::runner::EXIT_INTEGRITY_FAILED);
        assert_eq!(exit_code(false, 0, 1), crate::runner::EXIT_INTEGRITY_FAILED);
    }
}
