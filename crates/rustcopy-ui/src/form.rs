//! The editor's form: plain strings and booleans a window can bind to, and the conversion to and from
//! the core's `JobDraft` (CATALOGO_COMPORTAMENTI_GUI.md J01-J07, PIANO_GUI_SLINT.md phase 4c).
//!
//! Everything about what a draft may contain, and every refusal (mirror cannot be turned on, retention
//! cannot be lowered or introduced, ...), is `robocopy_ingest::job_editor`'s: this module only turns
//! what a person typed into typed fields, and says what was not a number. It deliberately keeps the
//! pieces the form does not own (`name`, and an encryption key written by hand) untouched.

use robocopy_ingest::generations::BackupType;
use robocopy_ingest::integrity::HashAlgorithm;
use robocopy_ingest::job_editor::JobDraft;

/// The credential-store prefix the form writes; any other form of `encrypt_aes256` is carried through.
pub const KEYRING_PREFIX: &str = "keyring:";

/// What the editor shows. Text boxes are `String`s; an empty one means "not set".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FormValues {
    pub name: String,
    pub source: String,
    pub dest: String,
    pub pattern: String,
    pub threads: String,
    pub retries: String,
    pub retry_wait_seconds: String,
    pub bandwidth_limit_mbps: String,
    pub verify_integrity: bool,
    pub fast_verify: bool,
    pub ignore_transient_missing: bool,
    pub exclude_junctions: bool,
    pub compare_baseline: bool,
    pub dry_run: bool,
    pub long_paths: bool,
    pub preserve_timestamps: bool,
    pub preserve_acl: bool,
    pub no_prescan: bool,
    pub mirror: bool,
    /// 0 = the engine's default, 1 = SHA-256, 2 = BLAKE3, 3 = xxHash3.
    pub hash_algo: i32,
    /// 0 = a plain copy, 1 = full, 2 = incremental, 3 = differential.
    pub backup_type: i32,
    pub keep_generations: String,
    /// One pattern per line.
    pub exclude_files: String,
    /// One pattern per line.
    pub exclude_dirs: String,
    pub min_age_days: String,
    pub max_age_days: String,
    pub report_path: String,
    pub log_path: String,
    pub html_report_path: String,
    /// The credential-store name (`keyring:NAME`), when encryption is set that way.
    pub encrypt_keyring: String,
    /// `true` when encryption was set by hand in another form (`env:`, `file:`, a literal key): the
    /// form shows it read-only and never rewrites it.
    pub encrypt_other: bool,
}

fn text<T: ToString>(value: &Option<T>) -> String {
    value.as_ref().map(ToString::to_string).unwrap_or_default()
}

fn lines(items: &[String]) -> String {
    items.join("\n")
}

fn algo_index(algorithm: Option<HashAlgorithm>) -> i32 {
    match algorithm {
        None => 0,
        Some(HashAlgorithm::Sha256) => 1,
        Some(HashAlgorithm::Blake3) => 2,
        Some(HashAlgorithm::Xxh3) => 3,
    }
}

fn algo_from(index: i32) -> Option<HashAlgorithm> {
    match index {
        1 => Some(HashAlgorithm::Sha256),
        2 => Some(HashAlgorithm::Blake3),
        3 => Some(HashAlgorithm::Xxh3),
        _ => None,
    }
}

fn backup_index(kind: Option<BackupType>) -> i32 {
    match kind {
        None => 0,
        Some(BackupType::Full) => 1,
        Some(BackupType::Incremental) => 2,
        Some(BackupType::Differential) => 3,
    }
}

fn backup_from(index: i32) -> Option<BackupType> {
    match index {
        1 => Some(BackupType::Full),
        2 => Some(BackupType::Incremental),
        3 => Some(BackupType::Differential),
        _ => None,
    }
}

impl FormValues {
    /// What the form shows for a draft the core read from a file.
    pub fn from_draft(draft: &JobDraft) -> Self {
        let (encrypt_keyring, encrypt_other) = match draft.encrypt_aes256.as_deref() {
            None => (String::new(), false),
            Some(value) => match value.strip_prefix(KEYRING_PREFIX) {
                Some(name) => (name.to_string(), false),
                None => (String::new(), true),
            },
        };
        Self {
            name: draft.name.clone(),
            source: draft.source.clone(),
            dest: draft.dest.clone(),
            pattern: draft.pattern.clone().unwrap_or_default(),
            threads: text(&draft.threads),
            retries: text(&draft.retries),
            retry_wait_seconds: text(&draft.retry_wait_seconds),
            bandwidth_limit_mbps: text(&draft.bandwidth_limit_mbps),
            verify_integrity: draft.verify_integrity,
            fast_verify: draft.fast_verify,
            ignore_transient_missing: draft.ignore_transient_missing,
            exclude_junctions: draft.exclude_junctions,
            compare_baseline: draft.compare_baseline,
            dry_run: draft.dry_run,
            long_paths: draft.long_paths,
            preserve_timestamps: draft.preserve_timestamps,
            preserve_acl: draft.preserve_acl,
            no_prescan: draft.no_prescan,
            mirror: draft.mirror,
            hash_algo: algo_index(draft.hash_algo),
            backup_type: backup_index(draft.backup_type),
            keep_generations: text(&draft.keep_generations),
            exclude_files: lines(&draft.exclude_files),
            exclude_dirs: lines(&draft.exclude_dirs),
            min_age_days: text(&draft.min_age_days),
            max_age_days: text(&draft.max_age_days),
            report_path: draft.report_path.clone().unwrap_or_default(),
            log_path: draft.log_path.clone().unwrap_or_default(),
            html_report_path: draft.html_report_path.clone().unwrap_or_default(),
            encrypt_keyring,
            encrypt_other,
        }
    }

    /// The draft this form describes, built over `original` so that what the form does not own is
    /// carried through. A box that is not a whole number is an error naming the field; whether the
    /// *values* are allowed is the core's to say when the proposal is built.
    pub fn to_draft(&self, original: &JobDraft) -> Result<JobDraft, String> {
        let encrypt_aes256 = if self.encrypt_other {
            original.encrypt_aes256.clone()
        } else if self.encrypt_keyring.trim().is_empty() {
            None
        } else {
            Some(format!("{KEYRING_PREFIX}{}", self.encrypt_keyring.trim()))
        };
        Ok(JobDraft {
            // The name is the job's identity (reports, cache and generations are named by it): the
            // form never renames.
            name: original.name.clone(),
            source: self.source.trim().to_string(),
            dest: self.dest.trim().to_string(),
            pattern: non_empty(&self.pattern),
            threads: number::<u16>(&self.threads, "Thread")?,
            retries: number::<u32>(&self.retries, "Tentativi")?,
            retry_wait_seconds: number::<u64>(&self.retry_wait_seconds, "Attesa fra i tentativi")?,
            bandwidth_limit_mbps: number::<u32>(&self.bandwidth_limit_mbps, "Limite di banda")?,
            verify_integrity: self.verify_integrity,
            fast_verify: self.fast_verify,
            ignore_transient_missing: self.ignore_transient_missing,
            exclude_junctions: self.exclude_junctions,
            compare_baseline: self.compare_baseline,
            dry_run: self.dry_run,
            long_paths: self.long_paths,
            preserve_timestamps: self.preserve_timestamps,
            preserve_acl: self.preserve_acl,
            no_prescan: self.no_prescan,
            mirror: self.mirror,
            hash_algo: algo_from(self.hash_algo),
            backup_type: backup_from(self.backup_type),
            keep_generations: number::<usize>(&self.keep_generations, "Cicli da conservare")?,
            exclude_files: split_lines(&self.exclude_files),
            exclude_dirs: split_lines(&self.exclude_dirs),
            min_age_days: number::<u32>(&self.min_age_days, "Età minima")?,
            max_age_days: number::<u32>(&self.max_age_days, "Età massima")?,
            report_path: non_empty(&self.report_path),
            log_path: non_empty(&self.log_path),
            html_report_path: non_empty(&self.html_report_path),
            encrypt_aes256,
        })
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn split_lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn number<T: std::str::FromStr>(value: &str, label: &str) -> Result<Option<T>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<T>()
        .map(Some)
        .map_err(|_| format!("{label}: «{trimmed}» non è un numero intero valido."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> JobDraft {
        JobDraft {
            name: "foto".to_string(),
            source: r"C:\Dati\Foto".to_string(),
            dest: r"D:\Archivio\Foto".to_string(),
            pattern: Some("*.jpg".to_string()),
            threads: Some(8),
            retries: Some(3),
            retry_wait_seconds: Some(5),
            verify_integrity: true,
            fast_verify: false,
            ignore_transient_missing: false,
            exclude_junctions: true,
            compare_baseline: false,
            dry_run: false,
            long_paths: false,
            preserve_timestamps: true,
            preserve_acl: false,
            no_prescan: false,
            hash_algo: Some(HashAlgorithm::Blake3),
            backup_type: Some(BackupType::Incremental),
            exclude_files: vec!["*.tmp".to_string(), "thumbs.db".to_string()],
            exclude_dirs: vec![".git".to_string()],
            min_age_days: Some(1),
            max_age_days: None,
            bandwidth_limit_mbps: None,
            report_path: Some("rapporto.json".to_string()),
            log_path: None,
            html_report_path: None,
            mirror: false,
            keep_generations: Some(7),
            encrypt_aes256: None,
        }
    }

    #[test]
    fn a_draft_survives_a_round_trip_through_the_form_unchanged() {
        let original = draft();
        let form = FormValues::from_draft(&original);
        assert_eq!(form.to_draft(&original).expect("round trip"), original);
    }

    #[test]
    fn an_empty_box_means_not_set_and_never_zero() {
        let original = draft();
        let mut form = FormValues::from_draft(&original);
        form.threads = "  ".to_string();
        form.keep_generations = String::new();
        form.pattern = String::new();
        let built = form.to_draft(&original).expect("draft");
        assert_eq!(
            built.threads, None,
            "an empty box must not become 0 threads"
        );
        assert_eq!(built.keep_generations, None);
        assert_eq!(built.pattern, None);
    }

    #[test]
    fn a_box_that_is_not_a_number_names_the_field() {
        let original = draft();
        let mut form = FormValues::from_draft(&original);
        form.threads = "otto".to_string();
        let error = form.to_draft(&original).expect_err("must refuse");
        assert!(
            error.starts_with("Thread:") && error.contains("otto"),
            "{error}"
        );
        form.threads = "-1".to_string();
        assert!(
            form.to_draft(&original).is_err(),
            "a negative count is not valid"
        );
    }

    #[test]
    fn exclusions_are_one_per_line_and_blank_lines_are_dropped() {
        let original = draft();
        let mut form = FormValues::from_draft(&original);
        assert_eq!(form.exclude_files, "*.tmp\nthumbs.db");
        form.exclude_files = "  *.bak \n\n   \n*.log\r\n".to_string();
        let built = form.to_draft(&original).expect("draft");
        assert_eq!(
            built.exclude_files,
            vec!["*.bak".to_string(), "*.log".to_string()]
        );
    }

    #[test]
    fn the_name_is_never_changed_by_the_form() {
        let original = draft();
        let mut form = FormValues::from_draft(&original);
        form.name = "altro".to_string();
        assert_eq!(form.to_draft(&original).expect("draft").name, "foto");
    }

    #[test]
    fn encryption_by_keyring_name_round_trips_and_other_forms_are_carried_untouched() {
        let mut original = draft();
        original.backup_type = None;
        original.encrypt_aes256 = Some("keyring:nas".to_string());
        let form = FormValues::from_draft(&original);
        assert_eq!(form.encrypt_keyring, "nas");
        assert!(!form.encrypt_other);
        assert_eq!(
            form.to_draft(&original)
                .expect("draft")
                .encrypt_aes256
                .as_deref(),
            Some("keyring:nas")
        );

        original.encrypt_aes256 = Some("env:RUSTCOPY_KEY".to_string());
        let mut form = FormValues::from_draft(&original);
        assert!(
            form.encrypt_other,
            "a hand-written form is flagged, not parsed"
        );
        assert_eq!(form.encrypt_keyring, "");
        form.encrypt_keyring = "tentativo".to_string(); // must be ignored while it is read-only
        assert_eq!(
            form.to_draft(&original)
                .expect("draft")
                .encrypt_aes256
                .as_deref(),
            Some("env:RUSTCOPY_KEY")
        );
    }

    #[test]
    fn choice_indexes_map_both_ways() {
        for index in 0..=3 {
            assert_eq!(algo_index(algo_from(index)), index);
            assert_eq!(backup_index(backup_from(index)), index);
        }
        assert_eq!(
            algo_from(9),
            None,
            "an unknown index is the default, not a panic"
        );
    }

    #[test]
    fn mirror_is_carried_so_an_unrelated_edit_cannot_switch_it_off() {
        let mut original = draft();
        original.backup_type = None;
        original.mirror = true;
        let mut form = FormValues::from_draft(&original);
        assert!(form.mirror);
        form.threads = "16".to_string();
        assert!(form.to_draft(&original).expect("draft").mirror);
    }
}
