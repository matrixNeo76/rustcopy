//! The core of the "Nuova attività" wizard (PIANO_GUI_ATTIVITA.md 3.2): turns what a person chose into a
//! task file, and refuses what a task made from the console must never be.
//!
//! The console only collects the answers and shows the steps; every decision lives here, so it is tested
//! without a window. A wizard task is the same plain configuration a drop or "Salva come attività" makes
//! (`runner::shell_drop_config_text`, which already applies [`runner::plan_copy_with_files`]'s refusals:
//! a destination inside its source, a drive root, duplicates) **plus** the few settings the wizard
//! offers. [`WizardDraft`] is deliberately a closed struct: it has no field for mirror, a command, a
//! webhook or a key, so none of them can arrive through it however the interface is written.

use std::path::{Path, PathBuf};

use crate::config::{IngestConfig, JobConfig};
use crate::errors::IngestError;
use crate::generations::BackupType;
use crate::integrity::HashAlgorithm;
use crate::runner;

/// Everything the wizard collects.
///
/// There is deliberately no "cycles to keep": retention **deletes** the oldest versions, and a task made
/// from the console must not be able to delete (the same rule the job editor follows for
/// `keep_generations`, which it can only keep or raise, never introduce). It is set by hand in the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WizardDraft {
    pub name: String,
    pub sources: Vec<PathBuf>,
    pub dest: PathBuf,
    /// `None` is a plain copy that updates the destination.
    pub backup_type: Option<BackupType>,
    pub verify: Option<HashAlgorithm>,
    pub exclude_files: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub min_age_days: Option<u32>,
    pub max_age_days: Option<u32>,
    pub bandwidth_limit_mbps: Option<u32>,
}

fn refuse(sentence: &str) -> IngestError {
    IngestError::CopyPlanInvalid(sentence.to_string())
}

/// The sentence for what is wrong with the settings (not the folders), or `None` when they hold
/// together. Separate from [`config_text`] so the wizard can say it on the step where it applies.
pub fn settings_problem(draft: &WizardDraft) -> Option<String> {
    if let (Some(min), Some(max)) = (draft.min_age_days, draft.max_age_days) {
        if min > max {
            return Some(
                "L'età minima non può essere maggiore di quella massima: nessun file la rispetterebbe."
                    .to_string(),
            );
        }
    }
    if draft.bandwidth_limit_mbps == Some(0) {
        return Some(
            "Un limite di banda di zero non copierebbe nulla: lascia vuoto per nessun limite."
                .to_string(),
        );
    }
    let blank = |items: &[String]| items.iter().any(|item| item.trim().is_empty());
    if blank(&draft.exclude_files) || blank(&draft.exclude_dirs) {
        return Some("Un'esclusione vuota non esclude nulla: toglila.".to_string());
    }
    None
}

/// The configuration text of the task, or the sentence that says what to fix.
pub fn config_text(draft: &WizardDraft) -> Result<String, IngestError> {
    crate::validate_job_name(&draft.name)?;
    if draft.sources.is_empty() {
        return Err(refuse("Scegli almeno una cartella o un file da copiare."));
    }
    if draft.dest.as_os_str().is_empty() {
        return Err(refuse("Scegli la cartella di destinazione."));
    }
    if let Some(problem) = settings_problem(draft) {
        return Err(IngestError::CopyPlanInvalid(problem));
    }
    let items = runner::plan_copy_with_files(&draft.sources, &draft.dest)?;
    let base = runner::shell_drop_config_text(&items, Some(&draft.name), draft.verify)?;
    let mut config: IngestConfig = toml::from_str(&base).map_err(|error| {
        IngestError::io(
            PathBuf::new(),
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })?;

    let apply = |job: &mut JobConfig| {
        job.backup_type = draft.backup_type;
        if !draft.exclude_files.is_empty() {
            job.exclude_files = Some(trimmed(&draft.exclude_files));
        }
        if !draft.exclude_dirs.is_empty() {
            job.exclude_dirs = Some(trimmed(&draft.exclude_dirs));
        }
        job.min_age_days = draft.min_age_days;
        job.max_age_days = draft.max_age_days;
        job.bandwidth_limit_mbps = draft.bandwidth_limit_mbps;
    };
    match config.jobs.as_mut() {
        Some(jobs) if !jobs.is_empty() => jobs.iter_mut().for_each(apply),
        _ => apply(&mut config.defaults),
    }

    toml::to_string_pretty(&config).map_err(|error| {
        IngestError::io(
            PathBuf::new(),
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })
}

fn trimmed(items: &[String]) -> Vec<String> {
    items.iter().map(|item| item.trim().to_string()).collect()
}

/// Whether a task called `name` already exists under `tasks_dir`.
pub fn name_taken(tasks_dir: &Path, name: &str) -> bool {
    tasks_dir.join(name).join(format!("{name}.toml")).exists()
}

/// Creates the task: a new file under `tasks_dir`, never over an existing one. Returns its path.
pub fn create_task(tasks_dir: &Path, draft: &WizardDraft) -> Result<PathBuf, IngestError> {
    let text = config_text(draft)?;
    crate::sessions::write_new_task_file(tasks_dir, &draft.name, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(dir: &Path) -> WizardDraft {
        let source = dir.join("origine");
        std::fs::create_dir_all(&source).expect("source");
        WizardDraft {
            name: "documenti".into(),
            sources: vec![source],
            dest: dir.join("destinazione"),
            ..WizardDraft::default()
        }
    }

    fn parsed(text: &str) -> IngestConfig {
        toml::from_str(text).expect("the text is a configuration")
    }

    #[test]
    fn a_plain_task_is_the_same_configuration_as_save_as_task() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = draft(dir.path());
        let text = config_text(&d).expect("text");
        let config = parsed(&text);
        assert_eq!(config.defaults.name.as_deref(), Some("documenti"));
        assert_eq!(config.defaults.source.as_ref(), Some(&d.sources[0]));
        assert_eq!(config.defaults.backup_type, None);
        assert_eq!(config.defaults.mirror, None, "a wizard task never mirrors");
    }

    #[test]
    fn the_chosen_settings_land_on_the_job() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut d = draft(dir.path());
        d.backup_type = Some(BackupType::Incremental);
        d.verify = Some(HashAlgorithm::Sha256);
        d.exclude_files = vec![" *.tmp ".into()];
        d.exclude_dirs = vec!["cache".into()];
        d.min_age_days = Some(1);
        d.max_age_days = Some(90);
        d.bandwidth_limit_mbps = Some(20);
        let config = parsed(&config_text(&d).expect("text"));
        let job = &config.defaults;
        assert_eq!(job.backup_type, Some(BackupType::Incremental));
        assert_eq!(job.verify_integrity, Some(true));
        assert_eq!(job.exclude_files, Some(vec!["*.tmp".to_string()]));
        assert_eq!(job.exclude_dirs, Some(vec!["cache".to_string()]));
        assert_eq!((job.min_age_days, job.max_age_days), (Some(1), Some(90)));
        assert_eq!(job.bandwidth_limit_mbps, Some(20));
    }

    /// Several sources become several jobs, and every one of them gets the settings.
    #[test]
    fn every_job_of_a_multi_source_task_gets_the_settings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut d = draft(dir.path());
        let second = dir.path().join("altra");
        std::fs::create_dir_all(&second).expect("second");
        d.sources.push(second);
        d.backup_type = Some(BackupType::Full);
        let config = parsed(&config_text(&d).expect("text"));
        let jobs = config.jobs.expect("two jobs");
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|j| j.backup_type == Some(BackupType::Full)));
    }

    /// The closed draft cannot carry what a console-made task must never have; the text proves it.
    #[test]
    fn a_wizard_task_carries_no_mirror_command_webhook_or_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut d = draft(dir.path());
        d.backup_type = Some(BackupType::Differential);
        d.verify = Some(HashAlgorithm::Xxh3);
        let text = config_text(&d).expect("text");
        for forbidden in [
            "mirror",
            "pre_command",
            "post_command",
            "webhook",
            "encrypt",
            "force_purge",
            "keep_generations",
            "decrypt",
            "refuse_dest_links",
        ] {
            assert!(!text.contains(forbidden), "{forbidden} in:\n{text}");
        }
    }

    #[test]
    fn what_does_not_hold_together_is_a_sentence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut d = draft(dir.path());
        assert_eq!(settings_problem(&d), None);
        d.min_age_days = Some(10);
        d.max_age_days = Some(5);
        assert!(settings_problem(&d)
            .expect("problem")
            .contains("età minima"));
        d.min_age_days = None;
        d.max_age_days = None;
        d.bandwidth_limit_mbps = Some(0);
        assert!(settings_problem(&d).expect("problem").contains("banda"));
        d.bandwidth_limit_mbps = None;
        d.exclude_files = vec!["  ".into()];
        assert!(settings_problem(&d).expect("problem").contains("vuota"));
        assert!(config_text(&d).is_err(), "the same refusal stops the text");
    }

    #[test]
    fn missing_pieces_and_unsafe_folders_are_refused_with_the_plans_own_rules() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut d = draft(dir.path());

        let mut no_sources = d.clone();
        no_sources.sources.clear();
        assert!(config_text(&no_sources).is_err());

        let mut no_dest = d.clone();
        no_dest.dest = PathBuf::new();
        assert!(config_text(&no_dest).is_err());

        let mut bad_name = d.clone();
        bad_name.name = "a/b".into();
        assert!(
            config_text(&bad_name).is_err(),
            "a name that cannot be a file name"
        );

        // The destination inside its own source is the plan's refusal, reused as it is.
        d.dest = d.sources[0].join("dentro");
        assert!(config_text(&d).is_err());
    }

    #[test]
    fn a_task_is_created_once_and_never_over_an_existing_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tasks = dir.path().join("attivita");
        let d = draft(dir.path());
        assert!(!name_taken(&tasks, "documenti"));
        let file = create_task(&tasks, &d).expect("created");
        assert!(
            file.ends_with("documenti/documenti.toml")
                || file.ends_with("documenti\\documenti.toml")
        );
        assert!(name_taken(&tasks, "documenti"));
        let before = std::fs::read(&file).expect("read");

        let mut other = d.clone();
        other.backup_type = Some(BackupType::Full);
        let error = create_task(&tasks, &other).expect_err("refused");
        assert!(error.to_string().contains("Esiste già"), "{error}");
        assert_eq!(
            std::fs::read(&file).expect("read"),
            before,
            "the first task is untouched"
        );
    }
}
