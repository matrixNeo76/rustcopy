//! "Sposta": copy, verify, then -- only after a confirmation -- delete the originals (GUI Slint plan,
//! phase 6; SPEC_GUI_SLINT.md 10.1, available from the Standard safety level).
//!
//! The copy and the verification are ordinary runs; this module is the **second step**. It is built so
//! that the dangerous part cannot happen by accident:
//!
//! * a [`VerifiedCopy`] is the only way in, and it can only be made from a run that was not a dry run,
//!   ended successfully and whose verification **passed** -- the type is the proof;
//! * [`plan_move`] looks at the disk **now** and lists only the source files whose copy in the
//!   destination exists with the same size and modification time. A file that is new, changed, missing
//!   in the destination or not a plain file stays where it is, and the plan says how many;
//! * [`execute_move`] needs [`Confirmation::Confirmed`], re-reads the level, and checks each file again
//!   right before deleting it: if it changed since the plan, it is left alone;
//! * nothing is ever removed recursively: only the planned files, then directories that are *empty*;
//! * the safety level is checked **here**, in the core, not only by hiding a button.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::errors::IngestError;
use crate::safety::{Confirmation, SafetyLevel};

/// How far two modification times may differ and still count as the same: file systems keep time at
/// different granularities (FAT, SMB and NTFS round differently).
const MTIME_TOLERANCE: Duration = Duration::from_secs(2);

/// How many examples of files left in place the plan keeps, for the person to read.
const KEPT_EXAMPLES: usize = 20;

/// Proof that the copy being moved really was verified. Cannot be built any other way.
#[derive(Debug, Clone, Copy)]
pub struct VerifiedCopy(());

impl VerifiedCopy {
    /// The rule, on the facts a report states: verification ran (`files_checked` is present) and
    /// passed, the run was not a dry run, and its exit code counts as success.
    pub fn check(
        integrity_status: Option<&str>,
        files_checked: Option<usize>,
        dry_run: bool,
        exit_code_is_success: Option<bool>,
    ) -> Result<Self, IngestError> {
        let refuse = |why: &str| {
            Err(IngestError::CopyPlanInvalid(format!(
                "Non cancello niente: {why}"
            )))
        };
        if dry_run {
            return refuse("quella copia era una simulazione.");
        }
        if exit_code_is_success != Some(true) {
            return refuse("la copia non è finita bene.");
        }
        if files_checked.is_none() {
            return refuse("la copia non è stata verificata.");
        }
        if integrity_status != Some("Passed") {
            return refuse("la verifica ha trovato differenze.");
        }
        Ok(VerifiedCopy(()))
    }

    /// From a report as the console reads it.
    pub fn from_report(view: &crate::gui_api::ReportView) -> Result<Self, IngestError> {
        Self::check(
            view.integrity_status.as_deref(),
            view.files_checked,
            view.dry_run,
            view.exit_code_is_success,
        )
    }
}

/// Why a source file is not going to be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepReason {
    /// No copy of it in the destination.
    MissingInDestination,
    /// The destination has a file with that name, but of another size or date.
    DifferentInDestination,
    /// Not a plain file (a link, a junction, a device): never touched.
    NotPlainFile,
}

/// One source file that has a matching copy and may be deleted.
#[derive(Debug, Clone)]
pub struct PlannedDelete {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
}

/// What a move would delete and what it would leave, as the disk is right now.
#[derive(Debug, Clone, Default)]
pub struct MovePlan {
    pub delete: Vec<PlannedDelete>,
    pub delete_bytes: u64,
    /// The source folders, so empty ones can be removed afterwards.
    pub roots: Vec<PathBuf>,
    pub kept_missing: u64,
    pub kept_different: u64,
    pub kept_not_plain: u64,
    /// A few of the files left in place, with the reason.
    pub kept_examples: Vec<(PathBuf, KeepReason)>,
}

impl MovePlan {
    pub fn kept(&self) -> u64 {
        self.kept_missing + self.kept_different + self.kept_not_plain
    }

    fn keep(&mut self, path: PathBuf, reason: KeepReason) {
        match reason {
            KeepReason::MissingInDestination => self.kept_missing += 1,
            KeepReason::DifferentInDestination => self.kept_different += 1,
            KeepReason::NotPlainFile => self.kept_not_plain += 1,
        }
        if self.kept_examples.len() < KEPT_EXAMPLES {
            self.kept_examples.push((path, reason));
        }
    }
}

/// What a move did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MoveOutcome {
    pub deleted: u64,
    pub deleted_bytes: u64,
    /// Left alone because the file changed (or its copy went away) after the plan was made.
    pub skipped_changed: u64,
    /// Could not be deleted (in use, read-only, no permission).
    pub failed: u64,
    pub folders_removed: u64,
    pub first_error: Option<String>,
}

fn same_time(a: SystemTime, b: SystemTime) -> bool {
    let gap = a
        .duration_since(b)
        .or_else(|_| b.duration_since(a))
        .unwrap_or_default();
    gap <= MTIME_TOLERANCE
}

fn is_plain_file(meta: &std::fs::Metadata) -> bool {
    if !meta.is_file() || meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

/// Lists what a move of `pairs` (source folder, the destination folder it was copied into, as
/// [`crate::runner::plan_copy`] returns them) would delete, judged on the disk right now. Reads only.
pub fn plan_move(
    level: SafetyLevel,
    pairs: &[(PathBuf, PathBuf)],
    _proof: VerifiedCopy,
) -> Result<MovePlan, IngestError> {
    require_level(level)?;
    let mut plan = MovePlan::default();
    for (source_root, dest_root) in pairs {
        plan.roots.push(source_root.clone());
        // Links and junctions are listed as entries but never followed.
        for entry in walkdir::WalkDir::new(source_root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_dir() {
                continue;
            }
            let path = entry.path().to_path_buf();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                plan.keep(path, KeepReason::NotPlainFile);
                continue;
            };
            if !is_plain_file(&meta) {
                plan.keep(path, KeepReason::NotPlainFile);
                continue;
            }
            // A single file was copied *into* the destination folder under its own name.
            let dest = if source_root.is_file() {
                match path.file_name() {
                    Some(name) => dest_root.join(name),
                    None => {
                        plan.keep(path, KeepReason::NotPlainFile);
                        continue;
                    }
                }
            } else {
                let Ok(relative) = path.strip_prefix(source_root) else {
                    plan.keep(path, KeepReason::NotPlainFile);
                    continue;
                };
                dest_root.join(relative)
            };
            let Ok(dest_meta) = std::fs::symlink_metadata(&dest) else {
                plan.keep(path, KeepReason::MissingInDestination);
                continue;
            };
            let (Ok(modified), Ok(dest_modified)) = (meta.modified(), dest_meta.modified()) else {
                plan.keep(path, KeepReason::DifferentInDestination);
                continue;
            };
            if !is_plain_file(&dest_meta)
                || dest_meta.len() != meta.len()
                || !same_time(modified, dest_modified)
            {
                plan.keep(path, KeepReason::DifferentInDestination);
                continue;
            }
            plan.delete_bytes += meta.len();
            plan.delete.push(PlannedDelete {
                source: path,
                dest,
                size: meta.len(),
                modified,
            });
        }
    }
    Ok(plan)
}

fn require_level(level: SafetyLevel) -> Result<(), IngestError> {
    if level.can_move() {
        Ok(())
    } else {
        Err(IngestError::CopyPlanInvalid(format!(
            "Il livello di sicurezza «{}» non permette di spostare: servono almeno «{}».",
            level.name(),
            SafetyLevel::Standard.name()
        )))
    }
}

/// Deletes exactly the files in `plan`, each only if it is still what the plan saw (same size and time,
/// and its copy still there), then removes the source folders that are left **empty**. Needs
/// [`Confirmation::Confirmed`] and a level that allows moving. Never removes anything recursively.
pub fn execute_move(
    level: SafetyLevel,
    plan: &MovePlan,
    confirmation: Confirmation,
) -> Result<MoveOutcome, IngestError> {
    require_level(level)?;
    if confirmation != Confirmation::Confirmed {
        return Err(IngestError::CopyPlanInvalid(
            "Cancellare gli originali richiede una conferma.".to_string(),
        ));
    }
    let mut outcome = MoveOutcome::default();
    for item in &plan.delete {
        let unchanged = std::fs::symlink_metadata(&item.source).is_ok_and(|meta| {
            is_plain_file(&meta)
                && meta.len() == item.size
                && meta.modified().is_ok_and(|m| same_time(m, item.modified))
        });
        let copy_still_there = std::fs::symlink_metadata(&item.dest)
            .is_ok_and(|meta| is_plain_file(&meta) && meta.len() == item.size);
        if !unchanged || !copy_still_there {
            outcome.skipped_changed += 1;
            continue;
        }
        match std::fs::remove_file(&item.source) {
            Ok(()) => {
                outcome.deleted += 1;
                outcome.deleted_bytes += item.size;
            }
            Err(error) => {
                outcome.failed += 1;
                outcome
                    .first_error
                    .get_or_insert_with(|| format!("{}: {error}", item.source.display()));
            }
        }
    }
    for root in &plan.roots {
        outcome.folders_removed += remove_empty_folders(root);
    }
    Ok(outcome)
}

/// Removes empty folders under `root`, deepest first, and `root` itself if it ends up empty. A folder
/// with anything in it is left: `remove_dir` refuses a non-empty one, and nothing here asks otherwise.
fn remove_empty_folders(root: &Path) -> u64 {
    let mut folders: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .map(walkdir::DirEntry::into_path)
        .collect();
    // `contents_first` already lists children before parents; the root comes last.
    folders.dedup();
    folders
        .into_iter()
        .filter(|dir| std::fs::remove_dir(dir).is_ok())
        .count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proof() -> VerifiedCopy {
        VerifiedCopy::check(Some("Passed"), Some(3), false, Some(true)).expect("verified")
    }

    fn put(path: &Path, bytes: &[u8], seconds: u64) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, bytes).expect("write");
        let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + seconds);
        std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open")
            .set_modified(when)
            .expect("set mtime");
    }

    /// A source with `a.txt`, `sub/b.txt`, `sub/deep/c.txt` and a destination holding identical copies.
    fn tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let base = tempfile::tempdir().expect("tempdir");
        let source = base.path().join("src");
        let dest = base.path().join("dst");
        for (relative, content) in [
            ("a.txt", "aaa"),
            ("sub/b.txt", "bb"),
            ("sub/deep/c.txt", "c"),
        ] {
            put(&source.join(relative), content.as_bytes(), 0);
            put(&dest.join(relative), content.as_bytes(), 0);
        }
        (base, source, dest)
    }

    #[test]
    fn a_copy_is_proven_only_when_it_ran_for_real_and_the_verification_passed() {
        assert!(VerifiedCopy::check(Some("Passed"), Some(1), false, Some(true)).is_ok());
        assert!(
            VerifiedCopy::check(Some("Passed"), Some(1), true, Some(true)).is_err(),
            "dry run"
        );
        assert!(
            VerifiedCopy::check(Some("Passed"), Some(1), false, Some(false)).is_err(),
            "failed run"
        );
        assert!(
            VerifiedCopy::check(Some("Passed"), Some(1), false, None).is_err(),
            "no outcome"
        );
        assert!(
            VerifiedCopy::check(None, None, false, Some(true)).is_err(),
            "never verified"
        );
        assert!(
            VerifiedCopy::check(Some("Failed"), Some(1), false, Some(true)).is_err(),
            "differences"
        );
        assert!(
            VerifiedCopy::check(Some("Passed"), None, false, Some(true)).is_err(),
            "no files checked"
        );
    }

    #[test]
    fn a_level_below_standard_cannot_plan_or_execute() {
        let (_base, source, dest) = tree();
        let pairs = [(source, dest)];
        assert!(plan_move(SafetyLevel::Prudent, &pairs, proof()).is_err());
        let plan = plan_move(SafetyLevel::Standard, &pairs, proof()).expect("plan");
        assert!(execute_move(SafetyLevel::Prudent, &plan, Confirmation::Confirmed).is_err());
    }

    #[test]
    fn only_files_with_an_identical_copy_are_planned_and_the_rest_stays() {
        let (_base, source, dest) = tree();
        put(&source.join("new.txt"), b"only here", 0); // no copy
        put(&source.join("changed.txt"), b"1234", 0);
        put(&dest.join("changed.txt"), b"12", 0); // other size
        put(&source.join("later.txt"), b"xx", 500);
        put(&dest.join("later.txt"), b"xx", 0); // other date

        let plan = plan_move(SafetyLevel::Standard, &[(source, dest)], proof()).expect("plan");
        assert_eq!(plan.delete.len(), 3, "a.txt, b.txt, c.txt");
        assert_eq!(plan.delete_bytes, 3 + 2 + 1);
        assert_eq!(plan.kept_missing, 1);
        assert_eq!(plan.kept_different, 2);
        assert_eq!(plan.kept(), 3);
        assert_eq!(plan.kept_examples.len(), 3);
    }

    #[test]
    fn nothing_is_deleted_without_a_confirmation() {
        let (_base, source, dest) = tree();
        let plan =
            plan_move(SafetyLevel::Standard, &[(source.clone(), dest)], proof()).expect("plan");
        assert!(execute_move(SafetyLevel::Standard, &plan, Confirmation::NotConfirmed).is_err());
        assert!(source.join("a.txt").exists());
        // Planning alone never touches anything either.
        assert!(source.join("sub").join("deep").join("c.txt").exists());
    }

    #[test]
    fn a_confirmed_move_deletes_the_planned_files_and_only_empty_folders() {
        let (_base, source, dest) = tree();
        put(&source.join("sub").join("keep.txt"), b"new", 0); // no copy: stays, and keeps `sub` alive
        let plan = plan_move(
            SafetyLevel::Standard,
            &[(source.clone(), dest.clone())],
            proof(),
        )
        .expect("plan");
        let outcome =
            execute_move(SafetyLevel::Standard, &plan, Confirmation::Confirmed).expect("move");

        assert_eq!(outcome.deleted, 3);
        assert_eq!(outcome.deleted_bytes, 6);
        assert_eq!(outcome.skipped_changed, 0);
        assert_eq!(outcome.failed, 0);
        assert!(!source.join("a.txt").exists());
        assert!(!source.join("sub").join("b.txt").exists());
        assert!(
            !source.join("sub").join("deep").exists(),
            "an emptied folder goes"
        );
        assert!(
            source.join("sub").join("keep.txt").exists(),
            "a file with no copy stays"
        );
        assert!(
            source.join("sub").exists(),
            "a folder that still holds a file stays"
        );
        assert!(dest.join("a.txt").exists(), "the copies are never touched");
        assert_eq!(outcome.folders_removed, 1);
    }

    #[test]
    fn a_move_that_empties_the_source_removes_the_source_folder_too() {
        let (_base, source, dest) = tree();
        let plan =
            plan_move(SafetyLevel::Standard, &[(source.clone(), dest)], proof()).expect("plan");
        let outcome =
            execute_move(SafetyLevel::Standard, &plan, Confirmation::Confirmed).expect("move");
        assert_eq!(outcome.deleted, 3);
        assert!(!source.exists());
    }

    #[test]
    fn a_file_that_changed_after_the_plan_is_left_alone() {
        let (_base, source, dest) = tree();
        let plan = plan_move(
            SafetyLevel::Standard,
            &[(source.clone(), dest.clone())],
            proof(),
        )
        .expect("plan");
        // The person edits a file, and another one loses its copy, between the plan and the confirmation.
        put(&source.join("a.txt"), b"edited meanwhile", 900);
        std::fs::remove_file(dest.join("sub").join("b.txt")).expect("remove copy");

        let outcome =
            execute_move(SafetyLevel::Standard, &plan, Confirmation::Confirmed).expect("move");
        assert_eq!(outcome.deleted, 1, "only c.txt was still safe");
        assert_eq!(outcome.skipped_changed, 2);
        assert!(source.join("a.txt").exists());
        assert!(source.join("sub").join("b.txt").exists());
    }
    #[test]
    fn a_single_file_source_is_matched_by_name_inside_the_destination_folder() {
        let (_base, source, dest) = tree();
        let file = source.join("a.txt");
        let plan = plan_move(
            SafetyLevel::Standard,
            &[(file.clone(), dest.clone())],
            proof(),
        )
        .expect("plan");
        // `dest` holds an identical `a.txt` at its top, so exactly that one file is planned.
        assert_eq!(plan.delete.len(), 1);
        assert_eq!(plan.delete[0].dest, dest.join("a.txt"));
        let outcome =
            execute_move(SafetyLevel::Standard, &plan, Confirmation::Confirmed).expect("move");
        assert_eq!(outcome.deleted, 1);
        assert!(!file.exists());
        assert!(
            source.join("sub").join("b.txt").exists(),
            "the rest of the folder is untouched"
        );
    }
}
