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
    /// or the positional `jobN` fallback.
    ///
    /// A name matching no stored job **creates** one; it does not rename an existing job, and the
    /// editor deliberately offers no rename. A job's name is its identity: `run_jobs` namespaces
    /// that job's report, `.ingest_cache` and generation manifest by it (D12). Renaming would
    /// therefore orphan the generation chain — the next incremental would find no reference
    /// generation — and start a fresh history under the new name, with the old files left behind.
    /// That is not something an edit box can do safely, so the frontend keeps this field read-only
    /// for a job that already exists.
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

/// Pins a boolean only when it is a real change.
///
/// A `[[jobs]]` entry inherits every field it does not set. Writing every field back at job level
/// would flatten that inheritance — and, worse, write the *draft's* idea of a field the job never
/// had. See [`apply_draft`] for the measurement that made this necessary.
fn pin_bool(own: Option<bool>, inherited: Option<bool>, value: bool) -> Option<bool> {
    if own.is_some() || value != inherited.unwrap_or(false) {
        Some(value)
    } else {
        None
    }
}

/// [`pin_bool`] for the optional fields.
fn pin<T: PartialEq>(own: Option<&T>, inherited: Option<&T>, value: Option<T>) -> Option<T> {
    if own.is_some() || value.as_ref() != inherited {
        value
    } else {
        None
    }
}

/// Applies a draft over the stored job, enforcing the rules above.
///
/// `own` is what this `[[jobs]]` entry itself sets, `None` for a job the file does not have yet.
/// `inherited` is the file's top-level defaults, which is the layer that makes this function
/// harder than it looks.
///
/// # Why inheritance has to be preserved rather than flattened
///
/// The first version of this function wrote every owned field back at job level. Measured on a
/// two-line config (`verify_integrity = true` at the top, one job overriding only `retries`),
/// changing that job's retry count produced `source = ""` **and** `verify_integrity = false` in the
/// proposal: an empty source that breaks the job outright, and integrity checking silently switched
/// off. That is precisely the failure this module's header argues against — a semantic change
/// nobody is looking for, arriving under an unrelated edit — reproduced by the module that argues
/// against it.
///
/// So a field is written at job level only when the job already pinned it, or when the draft's
/// value genuinely differs from what the job would inherit. Everything else keeps inheriting.
pub fn apply_draft(
    own: Option<&JobConfig>,
    inherited: &JobConfig,
    draft: &JobDraft,
) -> Result<JobConfig, IngestError> {
    let base = own.cloned().unwrap_or_default();
    // The rules below are about what the job *effectively* does, so they read the merged view: a
    // job inheriting `mirror = true` is a mirroring job even though its own entry says nothing.
    let effective = base.merged_over(inherited);

    if draft.mirror && !effective.mirror.unwrap_or(false) {
        return Err(IngestError::EditorCannotEnableMirror(draft.name.clone()));
    }

    match (effective.keep_generations, draft.keep_generations) {
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

    // Checked against the *resulting* mirror value, not the stored one: a job being turned off in
    // this same edit is no longer mirroring, and forbidding the combination there would be
    // pedantry rather than safety.
    if draft.no_prescan && !effective.no_prescan.unwrap_or(false) && draft.mirror {
        return Err(IngestError::EditorCannotDisablePrescanOnMirror(
            draft.name.clone(),
        ));
    }

    // The CLI rejects this range at startup (`IngestError::InvalidThreads`). Catching it here
    // means the editor cannot write a file that only fails hours later, on a scheduled run.
    if let Some(threads) = draft.threads {
        if threads == 0 || threads > 128 {
            return Err(IngestError::InvalidThreads(threads));
        }
    }

    let list = |items: &[String]| {
        if items.is_empty() {
            None
        } else {
            Some(items.to_vec())
        }
    };
    let path_of = |value: &Option<String>| {
        value
            .as_ref()
            .filter(|text| !text.trim().is_empty())
            .map(PathBuf::from)
    };
    // An empty box in the form means "not set", not "set to the empty path" — which is the shape
    // that produced `source = ""` above.
    let required_path = |value: &str| {
        if value.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    };

    Ok(JobConfig {
        name: Some(draft.name.clone()),
        source: pin(
            base.source.as_ref(),
            inherited.source.as_ref(),
            required_path(&draft.source),
        ),
        dest: pin(
            base.dest.as_ref(),
            inherited.dest.as_ref(),
            required_path(&draft.dest),
        ),
        pattern: pin(
            base.pattern.as_ref(),
            inherited.pattern.as_ref(),
            draft.pattern.clone(),
        ),
        threads: pin(
            base.threads.as_ref(),
            inherited.threads.as_ref(),
            draft.threads,
        ),
        retries: pin(
            base.retries.as_ref(),
            inherited.retries.as_ref(),
            draft.retries,
        ),
        retry_wait_seconds: pin(
            base.retry_wait_seconds.as_ref(),
            inherited.retry_wait_seconds.as_ref(),
            draft.retry_wait_seconds,
        ),
        verify_integrity: pin_bool(
            base.verify_integrity,
            inherited.verify_integrity,
            draft.verify_integrity,
        ),
        fast_verify: pin_bool(base.fast_verify, inherited.fast_verify, draft.fast_verify),
        ignore_transient_missing: pin_bool(
            base.ignore_transient_missing,
            inherited.ignore_transient_missing,
            draft.ignore_transient_missing,
        ),
        exclude_junctions: pin_bool(
            base.exclude_junctions,
            inherited.exclude_junctions,
            draft.exclude_junctions,
        ),
        html_report_path: pin(
            base.html_report_path.as_ref(),
            inherited.html_report_path.as_ref(),
            path_of(&draft.html_report_path),
        ),
        hash_algo: pin(
            base.hash_algo.as_ref(),
            inherited.hash_algo.as_ref(),
            draft.hash_algo,
        ),
        compare_baseline: pin_bool(
            base.compare_baseline,
            inherited.compare_baseline,
            draft.compare_baseline,
        ),
        report_path: pin(
            base.report_path.as_ref(),
            inherited.report_path.as_ref(),
            path_of(&draft.report_path),
        ),
        log_path: pin(
            base.log_path.as_ref(),
            inherited.log_path.as_ref(),
            path_of(&draft.log_path),
        ),
        dry_run: pin_bool(base.dry_run, inherited.dry_run, draft.dry_run),
        backup_type: pin(
            base.backup_type.as_ref(),
            inherited.backup_type.as_ref(),
            draft.backup_type,
        ),
        keep_generations: pin(
            base.keep_generations.as_ref(),
            inherited.keep_generations.as_ref(),
            draft.keep_generations,
        ),
        mirror: pin_bool(base.mirror, inherited.mirror, draft.mirror),
        exclude_files: pin(
            base.exclude_files.as_ref(),
            inherited.exclude_files.as_ref(),
            list(&draft.exclude_files),
        ),
        exclude_dirs: pin(
            base.exclude_dirs.as_ref(),
            inherited.exclude_dirs.as_ref(),
            list(&draft.exclude_dirs),
        ),
        min_age_days: pin(
            base.min_age_days.as_ref(),
            inherited.min_age_days.as_ref(),
            draft.min_age_days,
        ),
        max_age_days: pin(
            base.max_age_days.as_ref(),
            inherited.max_age_days.as_ref(),
            draft.max_age_days,
        ),
        bandwidth_limit_mbps: pin(
            base.bandwidth_limit_mbps.as_ref(),
            inherited.bandwidth_limit_mbps.as_ref(),
            draft.bandwidth_limit_mbps,
        ),
        no_prescan: pin_bool(base.no_prescan, inherited.no_prescan, draft.no_prescan),
        long_paths: pin_bool(base.long_paths, inherited.long_paths, draft.long_paths),
        preserve_timestamps: pin_bool(
            base.preserve_timestamps,
            inherited.preserve_timestamps,
            draft.preserve_timestamps,
        ),
        preserve_acl: pin_bool(
            base.preserve_acl,
            inherited.preserve_acl,
            draft.preserve_acl,
        ),
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
                // A single-job file has no layer above it, so nothing can be inherited and every
                // field the draft sets is written outright.
                config.defaults = apply_draft(
                    existing.map(|cfg| &cfg.defaults),
                    &JobConfig::default(),
                    draft,
                )?;
                return Ok(config);
            }
            _ => return Err(IngestError::EditorCannotSplitSingleJobConfig(current_label)),
        }
    }

    let mut jobs = stored_jobs;
    let defaults = config.defaults.clone();
    for draft in drafts {
        // Matched on the label `list_jobs` shows, so the name the operator saw is the name that
        // finds the job — including the positional fallback for an unnamed entry.
        match jobs
            .iter()
            .enumerate()
            .position(|(index, job)| label_of(job, index) == draft.name)
        {
            Some(index) => jobs[index] = apply_draft(Some(&jobs[index]), &defaults, draft)?,
            // A name matching nothing stored is a new job, appended. Jobs already there and not
            // named by any draft stay exactly as they were: omission never deletes.
            None => jobs.push(apply_draft(None, &defaults, draft)?),
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
    let proposal = build_proposal(existing, drafts)?;
    let rendered = toml::to_string_pretty(&proposal).map_err(|error| {
        IngestError::io(
            out_path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })?;

    // `create_new` is the refusal, not a check preceding one. Asking `exists()` first and then
    // writing leaves a window in which the file can appear between the two, and `atomic_write`
    // finishes with a rename, which replaces whatever is there — so the pair would have reported
    // a refusal it did not actually enforce. Here the operating system decides, atomically.
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(IngestError::EditorWouldOverwrite(out_path.to_path_buf()))
        }
        Err(error) => return Err(IngestError::io(out_path, error)),
    };

    use std::io::Write as _;
    file.write_all(rendered.as_bytes())
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
            // Merged, not raw: an inherited `source` read as `None` here would reach the form as
            // an empty box, and be written back as an override that empties it.
            draft_from(&job.merged_over(&config.defaults), &name)
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

        let error = apply_draft(Some(&config.defaults), &JobConfig::default(), &draft)
            .expect_err("must be refused");
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
        let result = apply_draft(Some(&config.defaults), &JobConfig::default(), &draft)
            .expect("an ordinary edit");

        assert_eq!(result.mirror, Some(true), "mirroring must not be disarmed");
        assert_eq!(result.retries, Some(9));
    }

    /// Turning a deletion off needs no gate.
    #[test]
    fn the_editor_may_turn_mirroring_off() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\nmirror = true\n");
        let mut draft = draft_for(&config, "job1");
        draft.mirror = false;

        let result = apply_draft(Some(&config.defaults), &JobConfig::default(), &draft)
            .expect("narrowing is allowed");
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
        let error = apply_draft(Some(&config.defaults), &JobConfig::default(), &introduce)
            .expect_err("lowering refused");
        assert!(
            matches!(
                error,
                IngestError::EditorCannotLowerRetention { from: 7, to: 3, .. }
            ),
            "got {error:?}"
        );

        let mut raise = draft_for(&config, "job1");
        raise.keep_generations = Some(12);
        apply_draft(Some(&config.defaults), &JobConfig::default(), &raise)
            .expect("keeping more deletes less");

        let bare = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let mut fresh = draft_for(&bare, "job1");
        fresh.keep_generations = Some(2);
        let error = apply_draft(Some(&bare.defaults), &JobConfig::default(), &fresh)
            .expect_err("introduction refused");
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

        let error = apply_draft(Some(&config.defaults), &JobConfig::default(), &draft)
            .expect_err("must be refused");
        assert!(
            matches!(error, IngestError::EditorCannotDisablePrescanOnMirror(_)),
            "got {error:?}"
        );

        // The same edit on a job that is not mirroring is ordinary.
        let plain = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let mut draft = draft_for(&plain, "job1");
        draft.no_prescan = true;
        apply_draft(Some(&plain.defaults), &JobConfig::default(), &draft)
            .expect("no mirror, no diff to protect");
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

        let result = apply_draft(Some(&config.defaults), &JobConfig::default(), &draft)
            .expect("an ordinary edit");

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
        let jobs = proposal.jobs.clone().expect("jobs");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[1].name.as_deref(), Some("due"));
        assert!(
            !jobs[1]
                .merged_over(&proposal.defaults)
                .mirror
                .unwrap_or(false),
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

    /// Measured, not argued. Before inheritance was preserved, changing one job's retry count on
    /// this exact two-line configuration wrote `source = ""` and `verify_integrity = false` into
    /// the proposal — an empty source that breaks the job, and integrity checking silently
    /// switched off — because every owned field was written back at job level whether or not the
    /// job had ever set it.
    #[test]
    fn an_unrelated_edit_leaves_every_inherited_field_inherited() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("jobs.toml");
        std::fs::write(
            &path,
            "source = \"D:/src\"\nverify_integrity = true\n\n\
             [[jobs]]\nname = \"uno\"\ndest = \"E:/1\"\nretries = 2\n",
        )
        .expect("write");

        let mut drafts = read_drafts(&path).expect("reads");
        assert_eq!(
            drafts[0].source, "D:/src",
            "the form must open on the effective value, not on the empty raw one"
        );
        assert!(drafts[0].verify_integrity);

        drafts[0].retries = Some(9);
        let config = IngestConfig::load_from(&path).expect("loads");
        let proposal = build_proposal(Some(&config), &drafts).expect("builds");
        let job = &proposal.jobs.as_ref().expect("jobs")[0];

        assert_eq!(job.retries, Some(9), "the edit lands");
        assert_eq!(job.source, None, "and the inherited source stays inherited");
        assert_eq!(
            job.verify_integrity, None,
            "as does verification, which must not be switched off by an unrelated edit"
        );
        assert!(
            job.merged_over(&proposal.defaults)
                .verify_integrity
                .unwrap_or(false),
            "so the job still verifies"
        );
    }

    /// The rules read the merged view, not the raw entry: a job that inherits `mirror = true` is a
    /// mirroring job even though its own section says nothing about it.
    #[test]
    fn an_inherited_mirror_counts_as_mirroring_for_the_rules() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("jobs.toml");
        std::fs::write(
            &path,
            "source = \"D:/src\"\nmirror = true\n\n[[jobs]]\nname = \"uno\"\ndest = \"E:/1\"\n",
        )
        .expect("write");

        let mut drafts = read_drafts(&path).expect("reads");
        assert!(
            drafts[0].mirror,
            "the form shows what the job effectively does"
        );

        // Removing the prescan from it must be refused, exactly as if the job set mirror itself.
        drafts[0].no_prescan = true;
        let config = IngestConfig::load_from(&path).expect("loads");
        let error = build_proposal(Some(&config), &drafts).expect_err("must be refused");
        assert!(
            matches!(error, IngestError::EditorCannotDisablePrescanOnMirror(_)),
            "got {error:?}"
        );
    }

    /// The CLI rejects this range at startup. Catching it here means the editor cannot produce a
    /// file that only fails hours later, on a scheduled run nobody is watching.
    #[test]
    fn a_thread_count_outside_the_supported_range_is_refused() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");

        for threads in [0u16, 129] {
            let mut draft = draft_for(&config, "job1");
            draft.threads = Some(threads);
            let error = apply_draft(Some(&config.defaults), &JobConfig::default(), &draft)
                .expect_err("must be refused");
            assert!(
                matches!(error, IngestError::InvalidThreads(value) if value == threads),
                "got {error:?}"
            );
        }

        let mut draft = draft_for(&config, "job1");
        draft.threads = Some(128);
        apply_draft(Some(&config.defaults), &JobConfig::default(), &draft)
            .expect("the bound holds");
    }

    /// An empty box means "not set", not "set to the empty path". This is the shape that produced
    /// `source = ""`.
    #[test]
    fn an_empty_field_is_left_unset_rather_than_written_empty() {
        let config = config_from("source = \"D:/src\"\ndest = \"E:/dst\"\n");
        let mut draft = draft_for(&config, "job1");
        draft.report_path = Some("   ".to_string());

        let result =
            apply_draft(Some(&config.defaults), &JobConfig::default(), &draft).expect("applies");
        assert_eq!(result.report_path, None);
    }
}
