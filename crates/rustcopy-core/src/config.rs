//! TOML Configuration file parser and mapper for robocopy_ingest.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::IngestError;
use crate::generations::BackupType;
use crate::integrity::HashAlgorithm;

/// The overridable per-job settings, shared by the top-level TOML fields (single-job mode,
/// unchanged since before F33) and by each entry of a `[[jobs]]` array (F33: multi-job mode).
///
/// `name` is only meaningful inside a `[[jobs]]` entry; it is ignored when it appears at the top
/// level (harmless if present there, just unused).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
/// # Flag CLI deliberatamente **assenti** da qui
///
/// `CLAUDE.md` prescrive che ogni flag CLI abbia il suo corrispettivo TOML. Sei fanno eccezione, e
/// l'eccezione è voluta:
///
/// - `--log-level`, `--quiet`, `--log-max-bytes`, `--log-max-backups`: il subscriber di `tracing`
///   si installa **una volta per processo** (`logging::init` usa `set_global_default`), e tutti i
///   job di un batch condividono un solo file di log. Un'impostazione per-job varrebbe solo per il
///   primo job e verrebbe ignorata per gli altri — silenziosamente, che è peggio dell'assenza.
/// - `--enable-dedup`, `--cloud-sync-target`: no-op dichiarati (`AGENTS.md` regola 7). Dare loro
///   una chiave TOML suggerirebbe che facciano qualcosa.
///
/// Se un flag nuovo non compare qui, verificare in quale delle due categorie ricade prima di
/// concludere che è una svista.
pub struct JobConfig {
    pub name: Option<String>,
    pub source: Option<PathBuf>,
    pub dest: Option<PathBuf>,
    pub pattern: Option<String>,
    pub threads: Option<u16>,
    pub retries: Option<u32>,
    pub retry_wait_seconds: Option<u64>,
    pub verify_integrity: Option<bool>,
    /// F28. Per-job because two jobs can reasonably differ on whether re-hashing every run is
    /// worth its cost.
    pub fast_verify: Option<bool>,
    /// F26a. Per-job because which files count as transient depends on what the job backs up.
    pub ignore_transient_missing: Option<bool>,
    /// F26d (`/XJ`). Per-job because whether junctions are content or loops depends on the tree.
    pub exclude_junctions: Option<bool>,
    /// Namespaced per job by `run_jobs`, exactly like `report_path` — without that, two jobs would
    /// overwrite each other's dashboard, which is the collision fixed in `merged_over`'s `name`.
    pub html_report_path: Option<PathBuf>,
    pub hash_algo: Option<HashAlgorithm>,
    pub compare_baseline: Option<bool>,
    pub report_path: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub dry_run: Option<bool>,
    /// F34: `full`/`incremental`/`differential`. `None` keeps the pre-F34 plain-sync behaviour.
    pub backup_type: Option<BackupType>,
    /// F35: number of most recent backup-generation cycles to keep; older cycles are deleted.
    /// Only meaningful together with `backup_type`.
    pub keep_generations: Option<usize>,
    pub mirror: Option<bool>,
    pub exclude_files: Option<Vec<String>>,
    pub exclude_dirs: Option<Vec<String>>,
    pub min_age_days: Option<u32>,
    pub max_age_days: Option<u32>,
    pub bandwidth_limit_mbps: Option<u32>,
    pub no_prescan: Option<bool>,
    pub long_paths: Option<bool>,
    pub preserve_timestamps: Option<bool>,
    pub preserve_acl: Option<bool>,
    pub webhook_url: Option<String>,
    /// F39: shell command run before the job starts; aborts the job if it exits non-zero.
    pub pre_command: Option<String>,
    /// F39: shell command run after the job finishes; a failure is logged/recorded but does not
    /// fail the job.
    pub post_command: Option<String>,
}

impl JobConfig {
    /// Merge `self` (a `[[jobs]]` entry) over `base` (the file's top-level defaults): any field
    /// `self` sets wins, any field it leaves unset falls back to `base`. Whole-value overwrite for
    /// every field, including the list fields (`exclude_files`/`exclude_dirs`) — a job that wants
    /// both the shared defaults' excludes and its own must repeat them, keeping the merge rule
    /// uniform across all fields instead of special-casing lists as "extend".
    pub fn merged_over(&self, base: &JobConfig) -> JobConfig {
        JobConfig {
            // `name` is the one field that does **not** inherit. It identifies a single job,
            // and `run_jobs` uses it to namespace that job's cache, generation manifest and
            // report path (D12). Inheriting a shared default would give every unnamed job the
            // same identity, so they would silently share all three — the collision D12
            // exists to prevent. Left as `None`, `run_jobs` falls back to `job1`, `job2`, …
            // which is distinct by construction.
            name: self.name.clone(),
            source: self.source.clone().or_else(|| base.source.clone()),
            dest: self.dest.clone().or_else(|| base.dest.clone()),
            pattern: self.pattern.clone().or_else(|| base.pattern.clone()),
            threads: self.threads.or(base.threads),
            retries: self.retries.or(base.retries),
            retry_wait_seconds: self.retry_wait_seconds.or(base.retry_wait_seconds),
            verify_integrity: self.verify_integrity.or(base.verify_integrity),
            fast_verify: self.fast_verify.or(base.fast_verify),
            ignore_transient_missing: self
                .ignore_transient_missing
                .or(base.ignore_transient_missing),
            exclude_junctions: self.exclude_junctions.or(base.exclude_junctions),
            html_report_path: self
                .html_report_path
                .clone()
                .or_else(|| base.html_report_path.clone()),
            hash_algo: self.hash_algo.or(base.hash_algo),
            compare_baseline: self.compare_baseline.or(base.compare_baseline),
            report_path: self
                .report_path
                .clone()
                .or_else(|| base.report_path.clone()),
            log_path: self.log_path.clone().or_else(|| base.log_path.clone()),
            dry_run: self.dry_run.or(base.dry_run),
            backup_type: self.backup_type.or(base.backup_type),
            keep_generations: self.keep_generations.or(base.keep_generations),
            mirror: self.mirror.or(base.mirror),
            exclude_files: self
                .exclude_files
                .clone()
                .or_else(|| base.exclude_files.clone()),
            exclude_dirs: self
                .exclude_dirs
                .clone()
                .or_else(|| base.exclude_dirs.clone()),
            min_age_days: self.min_age_days.or(base.min_age_days),
            max_age_days: self.max_age_days.or(base.max_age_days),
            bandwidth_limit_mbps: self.bandwidth_limit_mbps.or(base.bandwidth_limit_mbps),
            no_prescan: self.no_prescan.or(base.no_prescan),
            long_paths: self.long_paths.or(base.long_paths),
            preserve_timestamps: self.preserve_timestamps.or(base.preserve_timestamps),
            preserve_acl: self.preserve_acl.or(base.preserve_acl),
            webhook_url: self
                .webhook_url
                .clone()
                .or_else(|| base.webhook_url.clone()),
            pre_command: self
                .pre_command
                .clone()
                .or_else(|| base.pre_command.clone()),
            post_command: self
                .post_command
                .clone()
                .or_else(|| base.post_command.clone()),
        }
    }
}

/// TOML Configuration schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IngestConfig {
    /// Top-level fields, unchanged since before F33. In single-job mode (no `[[jobs]]`) these are
    /// merged straight into the CLI `Args`, exactly as always. In multi-job mode they act as
    /// shared defaults each job inherits from unless it overrides them (see [`JobConfig::merged_over`]).
    #[serde(flatten)]
    pub defaults: JobConfig,
    /// F33: `[[jobs]]` array of independently-run backup jobs sharing one config file. `None` or
    /// an empty array keeps the pre-F33 single-job behaviour entirely — `defaults` is the only
    /// thing that matters and this field is never consulted.
    pub jobs: Option<Vec<JobConfig>>,
}

impl IngestConfig {
    /// Load and parse a TOML configuration file from disk.
    pub fn load_from(path: &Path) -> Result<Self, IngestError> {
        let content = fs::read_to_string(path).map_err(|err| IngestError::io(path, err))?;
        toml::from_str(&content).map_err(|err| {
            IngestError::io(
                path,
                std::io::Error::new(std::io::ErrorKind::InvalidData, err),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_toml_config() {
        let toml_str = r#"
            source = "D:/landing"
            dest = "E:/warehouse"
            pattern = "*.csv"
            threads = 16
            verify_integrity = true
            hash_algo = "blake3"
            mirror = true
            exclude_files = ["*.tmp", "thumbs.db"]
            exclude_dirs = [".git"]
        "#;

        let config: IngestConfig = toml::from_str(toml_str).expect("valid toml");
        assert_eq!(config.defaults.source, Some(PathBuf::from("D:/landing")));
        assert_eq!(config.defaults.dest, Some(PathBuf::from("E:/warehouse")));
        assert_eq!(config.defaults.threads, Some(16));
        assert_eq!(config.defaults.verify_integrity, Some(true));
        assert_eq!(config.defaults.hash_algo, Some(HashAlgorithm::Blake3));
        assert_eq!(config.defaults.mirror, Some(true));
        assert_eq!(
            config.defaults.exclude_files,
            Some(vec!["*.tmp".to_string(), "thumbs.db".to_string()])
        );
        assert!(config.jobs.is_none());
    }

    #[test]
    fn parses_a_jobs_array_alongside_top_level_defaults() {
        let toml_str = r#"
            threads = 8
            verify_integrity = true

            [[jobs]]
            name = "documents"
            source = "D:/docs"
            dest = "E:/backup/docs"

            [[jobs]]
            name = "photos"
            source = "D:/photos"
            dest = "E:/backup/photos"
            threads = 32
        "#;

        let config: IngestConfig = toml::from_str(toml_str).expect("valid toml");
        assert_eq!(config.defaults.threads, Some(8));
        let jobs = config.jobs.expect("jobs array");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].name.as_deref(), Some("documents"));
        assert_eq!(jobs[1].name.as_deref(), Some("photos"));
    }

    #[test]
    fn job_overrides_win_but_fall_back_to_defaults() {
        let defaults = JobConfig {
            threads: Some(8),
            verify_integrity: Some(true),
            pattern: Some("*.csv".to_string()),
            ..JobConfig::default()
        };
        let job = JobConfig {
            name: Some("photos".to_string()),
            source: Some(PathBuf::from("D:/photos")),
            dest: Some(PathBuf::from("E:/backup/photos")),
            threads: Some(32),
            ..JobConfig::default()
        };

        let resolved = job.merged_over(&defaults);
        assert_eq!(resolved.threads, Some(32), "job's own value wins");
        assert_eq!(
            resolved.verify_integrity,
            Some(true),
            "falls back to the shared default"
        );
        assert_eq!(
            resolved.pattern.as_deref(),
            Some("*.csv"),
            "falls back to the shared default"
        );
        assert_eq!(resolved.source, Some(PathBuf::from("D:/photos")));
    }

    /// Deliberate asymmetry with `Args::apply_job_config` (documented in `ROADMAP.md` and
    /// `PIANO_MIGLIORAMENTI.md`, not a bug): a `[[jobs]]` entry that declares its own
    /// `exclude_files`/`exclude_dirs` REPLACES the file's top-level defaults entirely, per the
    /// doc comment on `merged_over` above — every field merges the same way (whole-value
    /// overwrite), lists included, rather than special-casing lists as "extend". This is the
    /// only one of the two merge semantics that lets a job *narrow* an inherited exclude list;
    /// accumulating unconditionally (like `apply_job_config` does for CLI+TOML) would make an
    /// inherited exclude permanently un-droppable by any job. A job that wants "the shared
    /// defaults plus my own" must repeat the shared ones explicitly — see
    /// `examples/scheduled-incremental.toml` for the documented example.
    #[test]
    fn job_excludes_replace_not_extend_the_shared_defaults() {
        let defaults = JobConfig {
            exclude_files: Some(vec!["*.tmp".to_string(), "thumbs.db".to_string()]),
            exclude_dirs: Some(vec![".git".to_string()]),
            ..JobConfig::default()
        };
        let job = JobConfig {
            name: Some("archivio".to_string()),
            exclude_files: Some(vec!["*.bak".to_string()]),
            ..JobConfig::default()
        };

        let resolved = job.merged_over(&defaults);
        assert_eq!(
            resolved.exclude_files,
            Some(vec!["*.bak".to_string()]),
            "job's own exclude_files replaces the shared defaults, not extends them"
        );
        assert_eq!(
            resolved.exclude_dirs,
            Some(vec![".git".to_string()]),
            "a field the job leaves unset still falls back to the shared default"
        );
    }
    /// `name` identifies **one job**. Inheriting it from the shared defaults gives every unnamed
    /// job the same identity, and that identity is what namespaces the per-job files — so two
    /// unnamed jobs would silently share one fast-verify cache, one generation manifest and one
    /// report path. That is precisely the collision D12 exists to prevent, reintroduced through
    /// the config layer instead of the path layer.
    ///
    /// Without a shared `name`, `run_jobs` falls back to `job1`, `job2`, … which is distinct by
    /// construction.
    #[test]
    fn an_unnamed_job_does_not_inherit_the_shared_name() {
        let defaults = JobConfig {
            name: Some("nightly".to_string()),
            pattern: Some("*.csv".to_string()),
            ..JobConfig::default()
        };
        let first = JobConfig {
            source: Some(PathBuf::from("D:/a")),
            ..JobConfig::default()
        };
        let second = JobConfig {
            source: Some(PathBuf::from("D:/b")),
            ..JobConfig::default()
        };

        let a = first.merged_over(&defaults);
        let b = second.merged_over(&defaults);

        assert_eq!(a.name, None, "an unnamed job stays unnamed");
        assert_eq!(b.name, None, "an unnamed job stays unnamed");
        assert_eq!(
            a.pattern.as_deref(),
            Some("*.csv"),
            "every other field still inherits: only `name` is exempt, because only `name` is an              identity rather than a setting"
        );
    }

    /// A job that names itself keeps its own name, shared default or not.
    #[test]
    fn a_named_job_keeps_its_own_name() {
        let defaults = JobConfig {
            name: Some("nightly".to_string()),
            ..JobConfig::default()
        };
        let job = JobConfig {
            name: Some("photos".to_string()),
            ..JobConfig::default()
        };
        assert_eq!(job.merged_over(&defaults).name.as_deref(), Some("photos"));
    }
}
