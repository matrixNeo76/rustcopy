//! The write side of the job editor (F54).
//!
//! Every other GUI-facing surface in this crate is read-only, and that is what made the guarantee
//! "this version cannot damage a backup" true. This module ends that, so it carries the rules that
//! keep the loss of that guarantee bounded.
//!
//! # The one rule
//!
//! **The editor may narrow risk. It may never widen it.**
//!
//! Concretely, four things follow from it, each enforced here and tested:
//!
//! 1. `mirror` cannot go from off to on. A job that purges the destination cannot be *born* in a
//!    user interface. It can be preserved (see below) and it can be turned off, because disabling
//!    a deletion needs no gate.
//! 2. `keep_generations` cannot be introduced where there was none, and cannot be lowered: keeping
//!    fewer cycles deletes more.
//! 3. `no_prescan` cannot be turned on for a job that mirrors — `--mirror` uses the prescan to know
//!    what it would delete, and without it the safety diff cannot run at all.
//! 4. Omission never deletes. A job the caller does not mention is carried through untouched, and
//!    a field the draft does not carry keeps whatever the file had.
//!
//! # Why preservation matters as much as prohibition
//!
//! Refusing to *write* `mirror` at all would have been the obvious reading of "the UI must not do
//! destructive things", and it would have been worse: round-tripping an existing `mirror = true`
//! job through the editor would silently switch mirroring **off**. The operator thinks they changed
//! a retry count; in fact the destination stops being a faithful copy and starts growing, and
//! nothing says so. A silent semantic change during an edit is a worse failure than the one that
//! prohibition was meant to prevent, because nobody is looking for it.
//!
//! So the fields this editor does not own are not dropped — they are copied through verbatim:
//! `pre_command`, `post_command` and `webhook_url` among them. Those belong to F55's write half,
//! which is still an open decision, and carrying them untouched is what keeps it genuinely open.
//!
//! # Never in place
//!
//! [`propose_config`] always writes a **new** file and refuses to overwrite an existing one. The
//! operator performs the substitution. A GUI that rewrites the production TOML in place can, with
//! one bad write, take out the configuration of every job at once; a GUI that writes a proposal
//! next to it cannot.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{IngestConfig, JobConfig};
use crate::errors::IngestError;
use crate::generations::BackupType;
use crate::integrity::HashAlgorithm;

/// The editable shape of one job.
///
/// Deliberately **not** a `JobConfig`: this is the subset a user interface may write, and the
/// difference between the two types is the security boundary made structural. A field absent here
/// cannot be set through this path however the frontend is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobDraft {
    /// The job this edits, by the same label [`crate::gui_api::list_jobs`] shows — its own name,
    /// or the positional `jobN` fallback. A name matching no stored job creates a new one.
    pub name: String,
    pub source: String,
    pub dest: String,
    pub pattern: Option<String>,
    pub threads: Option<u16>,
    pub retries: Option<u32>,
    pub retry_wait_seconds: Option<u64>,
    pub verify_integrity: bool,
    pub fast_verify: bool,
    pub ignore_transient_missing: bool,
    pub exclude_junctions: bool,
    pub compare_baseline: bool,
    pub dry_run: bool,
    pub long_paths: bool,
    pub preserve_timestamps: bool,
    pub preserve_acl: bool,
    /// Constrained: cannot be turned on for a mirroring job (rule 3).
    pub no_prescan: bool,
    pub hash_algo: Option<HashAlgorithm>,
    pub backup_type: Option<BackupType>,
    pub exclude_files: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub min_age_days: Option<u32>,
    pub max_age_days: Option<u32>,
    pub bandwidth_limit_mbps: Option<u32>,
    pub report_path: Option<String>,
    pub log_path: Option<String>,
    pub html_report_path: Option<String>,
    /// Constrained: off cannot become on (rule 1). Present so an existing mirroring job survives
    /// an edit instead of being silently disarmed.
    pub mirror: bool,
    /// Constrained: cannot be introduced, cannot be lowered (rule 2).
    pub keep_generations: Option<usize>,
}

fn optional_string(value: &Option<PathBuf>) -> Option<String> {
    value.as_ref().map(|path| path.display().to_string())
}

/// Fills a draft from a stored job, so the form starts from what the file actually says.
///
/// `name` is passed in rather than read from the job because an unnamed `[[jobs]]` entry has no
/// name of its own and is identified positionally — the same fallback `run_jobs` applies.
pub fn draft_from(job: &JobConfig, name: &str) -> JobDraft {
    JobDraft {
        name: name.to_string(),
        source: optional_string(&job.source).unwrap_or_default(),
        dest: optional_string(&job.dest).unwrap_or_default(),
        pattern: job.pattern.clone(),
        threads: job.threads,
        retries: job.retries,
        retry_wait_seconds: job.retry_wait_seconds,
        verify_integrity: job.verify_integrity.unwrap_or(false),
        fast_verify: job.fast_verify.unwrap_or(false),
        ignore_transient_missing: job.ignore_transient_missing.unwrap_or(false),
        exclude_junctions: job.exclude_junctions.unwrap_or(false),
        compare_baseline: job.compare_baseline.unwrap_or(false),
        dry_run: job.dry_run.unwrap_or(false),
        long_paths: job.long_paths.unwrap_or(false),
        preserve_timestamps: job.preserve_timestamps.unwrap_or(false),
        preserve_acl: job.preserve_acl.unwrap_or(false),
        no_prescan: job.no_prescan.unwrap_or(false),
        hash_algo: job.hash_algo,
        backup_type: job.backup_type,
        exclude_files: job.exclude_files.clone().unwrap_or_default(),
        exclude_dirs: job.exclude_dirs.clone().unwrap_or_default(),
        min_age_days: job.min_age_days,
        max_age_days: job.max_age_days,
        bandwidth_limit_mbps: job.bandwidth_limit_mbps,
        report_path: optional_string(&job.report_path),
        log_path: optional_string(&job.log_path),
        html_report_path: optional_string(&job.html_report_path),
        mirror: job.mirror.unwrap_or(false),
        keep_generations: job.keep_generations,
    }
}

/// Applies a draft over the stored job, enforcing the rules above.
///
/// `stored` is `None` for a job the file does not have yet. Every field this module does not own
/// is copied from `stored` untouched — that copy is rule 4, and it is why an unowned field cannot
/// be lost by editing.
pub fn apply_draft(stored: Option<&JobConfig>, draft: &JobDraft) -> Result<JobConfig, IngestError> {
    let base = stored.cloned().unwrap_or_default();
    let was_mirroring = base.mirror.unwrap_or(false);

    if draft.mirror && !was_mirroring {
        return Err(IngestError::EditorCannotEnableMirror(draft.name.clone()));
    }

    match (base.keep_generations, draft.keep_generations) {
        (None, Some(_)) => {
            return Err(IngestError::EditorCannotIntroduceRetention(
                draft.name.clone(),
            ))
        }
        (Some(from), Some(to)) if to < from => {
            return Err(IngestError::EditorCannotLowerRetention {
                name: draft.name.clone(),
                from,
                to,
            })
        }
        _ => {}
    }

    // Checked against the *resulting* mirror value, not the stored one: a job being turned off
    // this same edit is no longer mirroring, and forbidding the combination there would be
    // pedantry rather than safety.
    if draft.no_prescan && !base.no_prescan.unwrap_or(false) && draft.mirror {
        return Err(IngestError::EditorCannotDisablePrescanOnMirror(
            draft.name.clone(),
        ));
    }

    let empty_to_none = |items: &Vec<String>| {
        if items.is_empty() {
            None
        } else {
            Some(items.clone())
        }
    };
    let path_of = |value: &Option<String>| value.as_ref().map(PathBuf::from);

    Ok(JobConfig {
        name: Some(draft.name.clone()),
        source: Some(PathBuf::from(&draft.source)),
        dest: Some(PathBuf::from(&draft.dest)),
        pattern: draft.pattern.clone(),
        threads: draft.threads,
        retries: draft.retries,
        retry_wait_seconds: draft.retry_wait_seconds,
        verify_integrity: Some(draft.verify_integrity),
        fast_verify: Some(draft.fast_verify),
        ignore_transient_missing: Some(draft.ignore_transient_missing),
        exclude_junctions: Some(draft.exclude_junctions),
        html_report_path: path_of(&draft.html_report_path),
        hash_algo: draft.hash_algo,
        compare_baseline: Some(draft.compare_baseline),
        report_path: path_of(&draft.report_path),
        log_path: path_of(&draft.log_path),
        dry_run: Some(draft.dry_run),
        backup_type: draft.backup_type,
        keep_generations: draft.keep_generations,
        mirror: Some(draft.mirror),
        exclude_files: empty_to_none(&draft.exclude_files),
        exclude_dirs: empty_to_none(&draft.exclude_dirs),
        min_age_days: draft.min_age_days,
        max_age_days: draft.max_age_days,
        bandwidth_limit_mbps: draft.bandwidth_limit_mbps,
        no_prescan: Some(draft.no_prescan),
        long_paths: Some(draft.long_paths),
        preserve_timestamps: Some(draft.preserve_timestamps),
        preserve_acl: Some(draft.preserve_acl),
        // Not owned by this editor, therefore carried through rather than dropped. See the module
        // header: dropping them would be a silent semantic change, which is the failure mode this
        // whole module is shaped around.
        webhook_url: base.webhook_url.clone(),
        pre_command: base.pre_command.clone(),
        post_command: base.post_command.clone(),
    })
}

/// The label a stored job answers to: its own name, else the positional fallback.
fn label_of(job: &JobConfig, index: usize) -> String {
    job.name
        .clone()
        .unwrap_or_else(|| format!("job{}", index + 1))
}

/// Builds the proposed configuration without writing anything.
///
/// Split from [`propose_config`] so the rules can be tested without touching a filesystem, and so
/// a caller can show the operator what would be written before it is.
pub fn build_proposal(
    existing: Option<&IngestConfig>,
    drafts: &[JobDraft],
) -> Result<IngestConfig, IngestError> {
    let mut config = existing.cloned().unwrap_or_default();
    let stored_jobs = config.jobs.clone().unwrap_or_default();

    if stored_jobs.is_empty() {
        // A file with no `[[jobs]]` is a single-job file, and its one job lives in the top-level
        // fields. Turning it into a multi-job file changes what every other field in it means, so
        // the editor declines rather than doing it silently.
        let current_label = existing
            .map(|cfg| label_of(&cfg.defaults, 0))
            .unwrap_or_else(|| "job1".to_string());

        match drafts {
            [] => return Ok(config),
            [draft] if existing.is_none() || draft.name == current_label => {
                config.defaults = apply_draft(existing.map(|cfg| &cfg.defaults), draft)?;
                return Ok(config);
            }
            _ => return Err(IngestError::EditorCannotSplitSingleJobConfig(current_label)),
        }
    }

    let mut jobs = stored_jobs;
    for draft in drafts {
        // Matched on the label `list_jobs` shows, so the name the operator saw is the name that
        // finds the job — including the positional fallback for an unnamed entry.
        match jobs
            .iter()
            .enumerate()
            .position(|(index, job)| label_of(job, index) == draft.name)
        {
            Some(index) => jobs[index] = apply_draft(Some(&jobs[index]), draft)?,
            // A name matching nothing stored is a new job, appended. Jobs already there and not
            // named by any draft stay exactly as they were: omission never deletes.
            None => jobs.push(apply_draft(None, draft)?),
        }
    }

    config.jobs = Some(jobs);
    Ok(config)
}

/// Writes the proposed configuration to `out_path`, which must not already exist.
///
/// Refusing to overwrite is the point, not a precaution: the editor produces a proposal and the
/// operator decides whether it replaces the running configuration.
pub fn propose_config(
    existing: Option<&IngestConfig>,
    drafts: &[JobDraft],
    out_path: &Path,
) -> Result<(), IngestError> {
    if out_path.exists() {
        return Err(IngestError::EditorWouldOverwrite(out_path.to_path_buf()));
    }

    let proposal = build_proposal(existing, drafts)?;
    let rendered = toml::to_string_pretty(&proposal).map_err(|error| {
        IngestError::io(
            out_path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })?;

    crate::atomic_write(out_path, rendered.as_bytes())
        .map_err(|error| IngestError::io(out_path, error))
}

/// Suggests a name for the proposal, beside the file it derives from.
///
/// `stamp` is supplied rather than read from the clock so the caller controls it and a test can
/// assert on the result.
pub fn suggest_proposal_path(existing: &Path, stamp: &str) -> PathBuf {
    let stem = existing
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "rustcopy".to_string());
    let extension = existing
        .extension()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "toml".to_string());

    existing.with_file_name(format!("{stem}.proposta-{stamp}.{extension}"))
}

/// Reads every job of a configuration file as an editable draft.
///
/// Named and labelled exactly like [`crate::gui_api::list_jobs`], so the job an operator picked
/// from the list is the job the form opens.
pub fn read_drafts(config_path: &Path) -> Result<Vec<JobDraft>, IngestError> {
    let config = IngestConfig::load_from(config_path)?;
    let jobs = config.jobs.clone().unwrap_or_default();

    if jobs.is_empty() {
        let name = label_of(&config.defaults, 0);
        return Ok(vec![draft_from(&config.defaults, &name)]);
    }

    Ok(jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            let name = label_of(job, index);
            draft_from(job, &name)
        })
        .collect())
}

/// Loads `existing` (when given) and writes the proposal, so a caller does not have to hold a
/// parsed configuration between two calls.
pub fn propose_config_from_path(
    existing: Option<&Path>,
    drafts: &[JobDraft],
    out_path: &Path,
) -> Result<(), IngestError> {
    let config = match existing {
        Some(path) => Some(IngestConfig::load_from(path)?),
        None => None,
    };
    propose_config(config.as_ref(), drafts, out_path)
}

/// [`suggest_proposal_path`] with this moment's stamp.
///
/// Local time, not UTC: this name is read by whoever is standing in front of the machine, and it
/// exists to be recognised rather than to be sorted against anything.
pub fn suggest_proposal_path_now(existing: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    suggest_proposal_path(existing, &stamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_from(toml_src: &str) -> IngestConfig {
        toml::from_str(toml_src).expect("the fixture must parse")
    }

    fn draft_for(config: &IngestConfig, name: &str) -> JobDraft {
        let jobs = config.jobs.clone().unwrap_or_default();
        if jobs.is_empty() {
            return draft_from(&config.defaults, name);
        }
        let (_, job) = jobs
            .iter()
            .enumerate()
            .find(|(index, job)| label_of(job, *index) == name)
            .expect("the fixture must hold that job");
        draft_from(job, name)
    }

    /// The direction that must stay closed: a job that purges the destination cannot be created in
    /// a user interface, whatever the frontend sends.
    #[test]
    fn the_editor_refuses_to_turn_mirroring_on() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let mut draft = draft_for(&config, "job1");
        draft.mirror = true;

        let error = apply_draft(Some(&config.defaults), &draft).expect_err("must be refused");
        assert!(
            matches!(error, IngestError::EditorCannotEnableMirror(ref name) if name == "job1"),
            "got {error:?}"
        );
    }

    /// The reason `mirror` is in the draft at all. Dropping it instead would have silently disarmed
    /// an existing mirroring job on any unrelated edit — a change nobody is looking for, and worse
    /// than the one the prohibition prevents.
    #[test]
    fn an_existing_mirroring_job_survives_an_unrelated_edit() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\nmirror = true\n");
        let mut draft = draft_for(&config, "job1");
        assert!(draft.mirror, "the form must start from what the file says");

        draft.retries = Some(9);
        let result = apply_draft(Some(&config.defaults), &draft).expect("an ordinary edit");

        assert_eq!(result.mirror, Some(true), "mirroring must not be disarmed");
        assert_eq!(result.retries, Some(9));
    }

    /// Turning a deletion off needs no gate.
    #[test]
    fn the_editor_may_turn_mirroring_off() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\nmirror = true\n");
        let mut draft = draft_for(&config, "job1");
        draft.mirror = false;

        let result = apply_draft(Some(&config.defaults), &draft).expect("narrowing is allowed");
        assert_eq!(result.mirror, Some(false));
    }

    /// Retention deletes whole generation cycles. Introducing it is widening; so is lowering it,
    /// which is the half that is easy to miss because the number gets *smaller*.
    #[test]
    fn retention_can_be_neither_introduced_nor_lowered() {
        let config = config_from(
            "source = \"D:/src\"\ndest = \"E:/dst\"\nbackup_type = \"full\"\nkeep_generations = 7\n",
        );

        let mut introduce = draft_for(&config, "job1");
        introduce.keep_generations = Some(3);
        let error = apply_draft(Some(&config.defaults), &introduce).expect_err("lowering refused");
        assert!(
            matches!(
                error,
                IngestError::EditorCannotLowerRetention { from: 7, to: 3, .. }
            ),
            "got {error:?}"
        );

        let mut raise = draft_for(&config, "job1");
        raise.keep_generations = Some(12);
        apply_draft(Some(&config.defaults), &raise).expect("keeping more deletes less");

        let bare = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let mut fresh = draft_for(&bare, "job1");
        fresh.keep_generations = Some(2);
        let error = apply_draft(Some(&bare.defaults), &fresh).expect_err("introduction refused");
        assert!(
            matches!(error, IngestError::EditorCannotIntroduceRetention(_)),
            "got {error:?}"
        );
    }

    /// `--mirror` uses the prescan to work out what it would delete. Removing the prescan removes
    /// the safety diff, so it is widening even though the flag itself copies nothing.
    #[test]
    fn the_prescan_cannot_be_removed_from_a_mirroring_job() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\nmirror = true\n");
        let mut draft = draft_for(&config, "job1");
        draft.no_prescan = true;

        let error = apply_draft(Some(&config.defaults), &draft).expect_err("must be refused");
        assert!(
            matches!(error, IngestError::EditorCannotDisablePrescanOnMirror(_)),
            "got {error:?}"
        );

        // The same edit on a job that is not mirroring is ordinary.
        let plain = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let mut draft = draft_for(&plain, "job1");
        draft.no_prescan = true;
        apply_draft(Some(&plain.defaults), &draft).expect("no mirror, no diff to protect");
    }

    /// The fields this editor does not own are the ones F55's write half has still to decide on.
    /// Carrying them verbatim is what keeps that decision open instead of quietly making it.
    #[test]
    fn hooks_and_the_webhook_survive_an_edit_untouched() {
        let config = config_from(
            "source = \"D:/src\"\ndest = \"E:/dst\"\n\
             webhook_url = \"https://hooks.example/services/secret\"\n\
             pre_command = \"net stop MSSQLSERVER\"\npost_command = \"net start MSSQLSERVER\"\n",
        );
        let mut draft = draft_for(&config, "job1");
        draft.threads = Some(16);

        let result = apply_draft(Some(&config.defaults), &draft).expect("an ordinary edit");

        assert_eq!(
            result.webhook_url.as_deref(),
            Some("https://hooks.example/services/secret")
        );
        assert_eq!(result.pre_command.as_deref(), Some("net stop MSSQLSERVER"));
        assert_eq!(
            result.post_command.as_deref(),
            Some("net start MSSQLSERVER")
        );
    }

    /// Omission never deletes: editing one job of a batch must not remove the others, and a
    /// proposal that quietly dropped them would stop those backups without saying so.
    #[test]
    fn jobs_not_named_by_a_draft_are_carried_through_unchanged() {
        let config = config_from(
            "source = \"D:/src\"\n\n[[jobs]]\nname = \"documenti\"\ndest = \"E:/doc\"\n\n\
             [[jobs]]\nname = \"archivio\"\ndest = \"E:/arc\"\nmirror = true\n",
        );
        let mut draft = draft_for(&config, "documenti");
        draft.retries = Some(5);

        let proposal = build_proposal(Some(&config), &[draft]).expect("builds");
        let jobs = proposal.jobs.expect("jobs survive");

        assert_eq!(jobs.len(), 2, "the untouched job is still there");
        assert_eq!(jobs[0].retries, Some(5));
        assert_eq!(
            jobs[1].mirror,
            Some(true),
            "and keeps every setting it had, including the destructive one"
        );
    }

    /// A name matching no stored job is a new job, not a silent no-op.
    #[test]
    fn an_unknown_name_appends_a_job() {
        let config =
            config_from("source = \"D:/src\"\n\n[[jobs]]\nname = \"uno\"\ndest = \"E:/1\"\n");
        let mut draft = draft_for(&config, "uno");
        draft.name = "due".to_string();
        draft.dest = "E:/2".to_string();

        let proposal = build_proposal(Some(&config), &[draft]).expect("builds");
        let jobs = proposal.jobs.expect("jobs");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[1].name.as_deref(), Some("due"));
        assert_eq!(
            jobs[1].mirror,
            Some(false),
            "and a new job is born without the destructive setting"
        );
    }

    /// Converting a single-job file into a multi-job one changes what every top-level field in it
    /// means. The editor declines instead of doing it silently.
    #[test]
    fn splitting_a_single_job_file_is_declined() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let mut second = draft_for(&config, "job1");
        second.name = "secondo".to_string();

        let error = build_proposal(Some(&config), &[second]).expect_err("must be declined");
        assert!(
            matches!(error, IngestError::EditorCannotSplitSingleJobConfig(_)),
            "got {error:?}"
        );
    }

    /// The written proposal must be a configuration the CLI can actually load — a file the editor
    /// produces but `IngestConfig::load_from` rejects would be worse than no editor.
    #[test]
    fn a_written_proposal_parses_back_as_a_configuration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = config_from(
            "source = \"D:/src\"\n\n[[jobs]]\nname = \"documenti\"\ndest = \"E:/doc\"\n",
        );
        let mut draft = draft_for(&config, "documenti");
        draft.threads = Some(24);

        let out = dir.path().join("jobs.proposta.toml");
        propose_config(Some(&config), &[draft], &out).expect("writes");

        let reloaded = IngestConfig::load_from(&out).expect("the CLI must be able to load it");
        let jobs = reloaded.jobs.expect("jobs");
        assert_eq!(jobs[0].threads, Some(24));
    }

    /// Never in place. One bad write to the production TOML takes out every job at once; a refusal
    /// costs the operator one rename.
    #[test]
    fn writing_over_an_existing_file_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("already-here.toml");
        std::fs::write(&out, "source = \"D:/keep\"\n").expect("write");

        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let draft = draft_for(&config, "job1");

        let error = propose_config(Some(&config), &[draft], &out).expect_err("must refuse");
        assert!(
            matches!(error, IngestError::EditorWouldOverwrite(_)),
            "got {error:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&out).expect("read"),
            "source = \"D:/keep\"\n",
            "and the existing file is untouched"
        );
    }

    #[test]
    fn the_suggested_name_sits_beside_the_file_it_derives_from() {
        let suggested = suggest_proposal_path(Path::new("C:/backup/jobs.toml"), "20260902-1030");
        assert_eq!(
            suggested,
            PathBuf::from("C:/backup/jobs.proposta-20260902-1030.toml")
        );
    }

    /// The draft must survive the IPC boundary, like every other type a command returns.
    #[test]
    fn a_draft_serializes_to_json() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let draft = draft_for(&config, "job1");

        let json = serde_json::to_string(&draft).expect("JobDraft must serialize");
        let back: JobDraft = serde_json::from_str(&json).expect("and round-trip");
        assert_eq!(back, draft);
    }
}
