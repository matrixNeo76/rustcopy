//! Integrity verification of source vs destination.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use xxhash_rust::xxh3::Xxh3;

use crate::errors::IngestError;
use crate::progress::ProgressSink;
use crate::scan::ScannedFile;

/// Read buffer for hashing. 1 MiB keeps syscall overhead low on 50 GB-scale datasets.
const HASH_BUFFER_BYTES: usize = 1024 * 1024;

/// Supported hashing algorithms for integrity checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
pub enum HashAlgorithm {
    #[default]
    #[serde(rename = "sha256")]
    Sha256,
    #[serde(rename = "blake3")]
    Blake3,
    /// F29a: ~5-10x faster than BLAKE3 for corruption detection, but **not cryptographic** — a
    /// motivated adversary could construct a colliding payload. Fine for "did this bit flip
    /// during transfer", not for a backup where an attacker could have tampered with the data.
    #[serde(rename = "xxh3")]
    Xxh3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegrityStatus {
    #[serde(rename = "PASSED")]
    Passed,
    #[serde(rename = "FAILED")]
    Failed,
}

/// How two files were found to differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MismatchKind {
    /// Sizes differed; hashing was skipped because it could not possibly match.
    #[serde(rename = "size")]
    Size,
    /// Sizes matched but the content hash (`algorithm` field) differed.
    #[serde(rename = "hash")]
    Hash,
}

/// F26c (closes D6): only a deserialization fallback for reports predating this field's
/// existence (see `Mismatch`'s `#[serde(default)]` fields below) — not a meaningful "default
/// kind of mismatch". `Size` is picked arbitrarily; a legacy report's `mismatches` entries carry
/// no reliable kind information at all, so nothing derived from this value should be trusted for
/// data reconstructed this way.
impl Default for MismatchKind {
    fn default() -> Self {
        MismatchKind::Size
    }
}

/// A file whose destination differs from the source.
///
/// F26c (closes D6): `kind`/`algorithm`/`source_digest`/`dest_digest` replaced an older
/// `source_sha256`/`dest_sha256` shape without `report::SCHEMA_VERSION` being bumped at the time.
/// `#[serde(default)]` on the fields below means a report written in that older shape (which
/// won't have these field names at all) still deserializes — with these fields defaulted rather
/// than the whole `IngestReport` (and therefore `--restore-from`, which parses the full report)
/// failing outright. `path` has no default: without it there's nothing identifying which file the
/// entry is even about, so failing loudly is correct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mismatch {
    pub path: String,
    #[serde(default)]
    pub kind: MismatchKind,
    /// Hash algorithm used for `source_digest`/`dest_digest` when `kind` is `Hash`; the literal
    /// string `"size"` when `kind` is `Size` and the fields below hold byte counts instead.
    #[serde(default)]
    pub algorithm: String,
    #[serde(default)]
    pub source_digest: String,
    #[serde(default)]
    pub dest_digest: String,
}

/// Maximum number of detailed error entries kept in the report to prevent OOM.
pub const MAX_REPORTED_ERRORS: usize = 10_000;

/// Outcome of the verification pass, embedded in the JSON report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityCheck {
    pub files_checked: usize,
    pub bytes_hashed: u64,
    pub mismatches: Vec<Mismatch>,
    pub missing_in_dest: Vec<String>,
    pub unreadable: Vec<String>,
    pub status: IntegrityStatus,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub total_errors: usize,
    /// F28: files `--fast-verify` skipped re-hashing because their size+mtime matched the
    /// `.ingest_cache` entry from the last run that verified them clean. `0` when `--fast-verify`
    /// wasn't used (or nothing was in the cache yet).
    #[serde(default)]
    pub skipped_unchanged: usize,
}

impl IntegrityCheck {
    pub fn passed(&self) -> bool {
        self.status == IntegrityStatus::Passed
    }
}

/// Hash a file using the specified algorithm, lowercase hex output.
pub fn hash_file(path: &Path, algo: HashAlgorithm) -> Result<String, IngestError> {
    match algo {
        HashAlgorithm::Sha256 => sha256_file(path),
        HashAlgorithm::Blake3 => blake3_file(path),
        HashAlgorithm::Xxh3 => xxh3_file(path),
    }
}

/// SHA-256 of a file, lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String, IngestError> {
    let mut file = File::open(path).map_err(|error| IngestError::io(path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER_BYTES];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| IngestError::io(path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// BLAKE3 of a file, lowercase hex (3-5x faster than SHA-256).
pub fn blake3_file(path: &Path) -> Result<String, IngestError> {
    let mut file = File::open(path).map_err(|error| IngestError::io(path, error))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; HASH_BUFFER_BYTES];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| IngestError::io(path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// XXH3-64 of a file, lowercase hex (F29a). Not cryptographic — see [`HashAlgorithm::Xxh3`].
pub fn xxh3_file(path: &Path) -> Result<String, IngestError> {
    let mut file = File::open(path).map_err(|error| IngestError::io(path, error))?;
    let mut hasher = Xxh3::new();
    let mut buffer = vec![0u8; HASH_BUFFER_BYTES];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| IngestError::io(path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:016x}", hasher.digest()))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Compare every scanned source file against its destination counterpart.
///
/// A single unreadable or mismatching file does not stop the pass: the whole picture is collected
/// so the operator sees every problem in one report.
/// Outcome of a single file verification pass.
enum FileVerificationOutcome {
    Passed { bytes: u64 },
    Mismatch(Mismatch),
    Missing(String),
    Unreadable(String),
}

/// Compare every scanned source file against its destination counterpart using Rayon thread pool.
///
/// F2.3: Parallelised across thread pool (rayon). Includes size pre-checks to skip hashing files
/// whose destination size differs from source size.
/// F4.6: Supports BLAKE3 in addition to SHA-256 via `algo`.
pub fn verify(
    source_root: &Path,
    dest_root: &Path,
    files: &[ScannedFile],
    algo: HashAlgorithm,
    sink: &dyn ProgressSink,
) -> IntegrityCheck {
    let results: Vec<FileVerificationOutcome> = files
        .par_iter()
        .map(|file| {
            let source_path = source_root.join(&file.relative_path);
            let dest_path = dest_root.join(&file.relative_path);
            let label = to_display(&file.relative_path);

            let dest_meta = match dest_path.metadata() {
                Ok(meta) => meta,
                Err(_) => {
                    tracing::error!(path = %label, "file missing in destination");
                    return FileVerificationOutcome::Missing(label);
                }
            };

            // Pre-check size: if sizes differ, don't waste time computing hashes.
            if dest_meta.len() != file.size_bytes {
                tracing::error!(
                    path = %label,
                    source_bytes = file.size_bytes,
                    dest_bytes = dest_meta.len(),
                    "size mismatch detected before hashing"
                );
                return FileVerificationOutcome::Mismatch(Mismatch {
                    path: label,
                    kind: MismatchKind::Size,
                    algorithm: "size".to_string(),
                    source_digest: format!("{}B", file.size_bytes),
                    dest_digest: format!("{}B", dest_meta.len()),
                });
            }

            // Sizes match: compute hashes in parallel.
            match (hash_file(&source_path, algo), hash_file(&dest_path, algo)) {
                (Ok(source_hash), Ok(dest_hash)) => {
                    sink.add_bytes(file.size_bytes);
                    sink.add_file();

                    if source_hash == dest_hash {
                        tracing::debug!(path = %label, hash = %source_hash, "checksum matches");
                        FileVerificationOutcome::Passed {
                            bytes: file.size_bytes,
                        }
                    } else {
                        tracing::error!(path = %label, "checksum mismatch");
                        FileVerificationOutcome::Mismatch(Mismatch {
                            path: label,
                            kind: MismatchKind::Hash,
                            algorithm: match algo {
                                HashAlgorithm::Sha256 => "sha256".to_string(),
                                HashAlgorithm::Blake3 => "blake3".to_string(),
                                HashAlgorithm::Xxh3 => "xxh3".to_string(),
                            },
                            source_digest: source_hash,
                            dest_digest: dest_hash,
                        })
                    }
                }
                (Err(error), _) | (_, Err(error)) => {
                    tracing::error!(path = %label, "unable to hash: {error}");
                    FileVerificationOutcome::Unreadable(label)
                }
            }
        })
        .collect();

    let mut check = IntegrityCheck {
        files_checked: 0,
        bytes_hashed: 0,
        mismatches: Vec::new(),
        missing_in_dest: Vec::new(),
        unreadable: Vec::new(),
        status: IntegrityStatus::Passed,
        truncated: false,
        total_errors: 0,
        skipped_unchanged: 0,
    };

    let total_err_count = results.iter().filter(|r| !matches!(r, FileVerificationOutcome::Passed { .. })).count();
    check.total_errors = total_err_count;
    if total_err_count > MAX_REPORTED_ERRORS {
        check.truncated = true;
    }

    for res in results {
        match res {
            FileVerificationOutcome::Passed { bytes } => {
                check.files_checked += 1;
                check.bytes_hashed += bytes;
            }
            FileVerificationOutcome::Mismatch(mismatch) => {
                if check.mismatches.len() < MAX_REPORTED_ERRORS {
                    check.mismatches.push(mismatch);
                }
            }
            FileVerificationOutcome::Missing(label) => {
                if check.missing_in_dest.len() < MAX_REPORTED_ERRORS {
                    check.missing_in_dest.push(label);
                }
            }
            FileVerificationOutcome::Unreadable(label) => {
                if check.unreadable.len() < MAX_REPORTED_ERRORS {
                    check.unreadable.push(label);
                }
            }
        }
    }

    if !check.mismatches.is_empty()
        || !check.missing_in_dest.is_empty()
        || !check.unreadable.is_empty()
    {
        check.status = IntegrityStatus::Failed;
    }
    check
}

fn to_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// True if `path` (a forward-slash relative path, as stored in `missing_in_dest`/`unreadable`)
/// matches a well-known transient pattern: `.log`/`.tmp` files, or anything under a `.git/objects`
/// tree. These can legitimately disappear between the copy and the verification pass (log
/// rotation, temp-file cleanup, git garbage collection) — observed for real during field testing,
/// per `ANALYSIS.md` D2. Used by `--ignore-transient-missing` (F26a).
fn is_transient_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".log") || lower.ends_with(".tmp") || lower.contains(".git/objects/")
}

/// `--ignore-transient-missing` (F26a, closes half of D2): drop transient-pattern entries from
/// `missing_in_dest`/`unreadable` and recompute `status` accordingly, so a file that merely
/// vanished for an expected reason doesn't fail the whole verification pass.
///
/// `total_errors` is left untouched: when `truncated` is set, it reflects the *real* error count
/// beyond what the (capped) vectors hold, and filtering only the retained entries can't correct
/// that upper bound — recomputing it here would silently understate a truncated run instead.
pub fn ignore_transient_missing(mut check: IntegrityCheck) -> IntegrityCheck {
    check.missing_in_dest.retain(|p| !is_transient_path(p));
    check.unreadable.retain(|p| !is_transient_path(p));
    check.status = if check.mismatches.is_empty()
        && check.missing_in_dest.is_empty()
        && check.unreadable.is_empty()
    {
        IntegrityStatus::Passed
    } else {
        IntegrityStatus::Failed
    };
    check
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::naive::NaiveCopyEngine;
    use crate::engine::{CopyEngine, CopyRequest};
    use crate::progress::NoopProgress;
    use crate::scan;
    use crate::testkit::{fixture_tree, write_fixture_file};

    fn copy_tree(source: &Path, dest: &Path) {
        NaiveCopyEngine::new()
            .copy(
                &CopyRequest {
                    source: source.to_path_buf(),
                    dest: dest.to_path_buf(),
                    pattern: "*.csv".to_string(),
                    threads: 1,
                    file_retries: 0,
                    retry_wait_seconds: 0,
                    dry_run: false,
                    mirror: false,
                    exclude_files: vec![],
                    exclude_dirs: vec![],
                    min_age_days: None,
                    max_age_days: None,
                    inter_packet_gap_ms: None,
                    prescan: true,
                    long_paths: false,
                    preserve_timestamps: false,
                    preserve_acl: false,
                    exclude_junctions: false,
                },
                &NoopProgress,
            )
            .expect("baseline copy");
    }

    #[test]
    fn sha256_matches_the_known_vector_for_an_empty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.csv");
        std::fs::write(&path, b"").expect("write");
        assert_eq!(
            sha256_file(&path).expect("hash"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_matches_the_known_vector_for_abc() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("abc.csv");
        std::fs::write(&path, b"abc").expect("write");
        assert_eq!(
            sha256_file(&path).expect("hash"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hashing_a_missing_file_is_an_error() {
        assert!(sha256_file(Path::new("/definitely/not/here.csv")).is_err());
    }

    #[test]
    fn identical_trees_pass() {
        let source = fixture_tree(&[("a.csv", 4096), ("nested/b.csv", 2_500_000)]);
        let dest = tempfile::tempdir().expect("dest");
        copy_tree(source.path(), dest.path());

        let inventory = scan::scan(source.path(), "*.csv", false, &[], &[]).expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Sha256, &NoopProgress);

        assert!(check.passed());
        assert_eq!(check.files_checked, 2);
        assert_eq!(check.bytes_hashed, 4096 + 2_500_000);
        assert!(check.mismatches.is_empty());
        assert!(check.missing_in_dest.is_empty());
    }

    #[test]
    fn corrupted_destination_is_detected() {
        let source = fixture_tree(&[("a.csv", 1024), ("b.csv", 1024)]);
        let dest = tempfile::tempdir().expect("dest");
        copy_tree(source.path(), dest.path());
        std::fs::write(dest.path().join("b.csv"), vec![0u8; 1024]).expect("corrupt");

        let inventory = scan::scan(source.path(), "*.csv", false, &[], &[]).expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Sha256, &NoopProgress);

        assert!(!check.passed());
        assert_eq!(check.status, IntegrityStatus::Failed);
        assert_eq!(check.mismatches.len(), 1);
        assert_eq!(check.mismatches[0].path, "b.csv");
        assert_ne!(
            check.mismatches[0].source_digest,
            check.mismatches[0].dest_digest
        );
    }

    #[test]
    fn truncated_destination_is_detected() {
        let source = fixture_tree(&[("a.csv", 2048)]);
        let dest = tempfile::tempdir().expect("dest");
        copy_tree(source.path(), dest.path());
        write_fixture_file(&dest.path().join("a.csv"), 1024);

        let inventory = scan::scan(source.path(), "*.csv", false, &[], &[]).expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Sha256, &NoopProgress);

        assert!(!check.passed());
        assert_eq!(check.mismatches.len(), 1);
    }

    #[test]
    fn missing_destination_files_are_listed() {
        let source = fixture_tree(&[("a.csv", 100), ("nested/b.csv", 100)]);
        let dest = tempfile::tempdir().expect("dest");
        copy_tree(source.path(), dest.path());
        std::fs::remove_file(dest.path().join("nested/b.csv")).expect("remove");

        let inventory = scan::scan(source.path(), "*.csv", false, &[], &[]).expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Sha256, &NoopProgress);

        assert!(!check.passed());
        assert_eq!(check.missing_in_dest, vec!["nested/b.csv".to_string()]);
        assert_eq!(check.files_checked, 1, "the present file is still verified");
    }

    #[test]
    fn empty_inventory_passes() {
        let dir = fixture_tree(&[]);
        let check = verify(dir.path(), dir.path(), &[], HashAlgorithm::Sha256, &NoopProgress);
        assert!(check.passed());
        assert_eq!(check.files_checked, 0);
    }

    #[test]
    fn blake3_hashing_passes_and_is_correct() {
        let source = fixture_tree(&[("a.csv", 4096)]);
        let dest = tempfile::tempdir().expect("dest");
        copy_tree(source.path(), dest.path());

        let inventory = scan::scan(source.path(), "*.csv", false, &[], &[]).expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Blake3, &NoopProgress);
        assert!(check.passed());
        assert_eq!(check.files_checked, 1);
    }

    /// F29a: xxh3 hashing catches a real corruption exactly like sha256/blake3 do.
    #[test]
    fn xxh3_hashing_passes_and_is_correct() {
        let source = fixture_tree(&[("a.csv", 4096)]);
        let dest = tempfile::tempdir().expect("dest");
        copy_tree(source.path(), dest.path());

        let inventory = scan::scan(source.path(), "*.csv", false, &[], &[]).expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Xxh3, &NoopProgress);
        assert!(check.passed());
        assert_eq!(check.files_checked, 1);
    }

    #[test]
    fn xxh3_detects_corruption() {
        let source = fixture_tree(&[("a.csv", 4096)]);
        let dest = tempfile::tempdir().expect("dest");
        copy_tree(source.path(), dest.path());
        // Corrupt one byte at the destination without changing its size, so the size pre-check
        // can't catch it — only the hash comparison can.
        let dest_file = dest.path().join("a.csv");
        let mut bytes = std::fs::read(&dest_file).expect("read");
        bytes[0] ^= 0xFF;
        std::fs::write(&dest_file, bytes).expect("corrupt");

        let inventory = scan::scan(source.path(), "*.csv", false, &[], &[]).expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Xxh3, &NoopProgress);
        assert!(!check.passed());
        assert_eq!(check.mismatches.len(), 1);
        assert_eq!(check.mismatches[0].algorithm, "xxh3");
    }

    #[test]
    fn xxh3_file_produces_a_16_char_lowercase_hex_digest() {
        let dir = fixture_tree(&[("a.csv", 100)]);
        let digest = xxh3_file(&dir.path().join("a.csv")).expect("hash");
        assert_eq!(digest.len(), 16);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn relative_paths_are_reported_with_forward_slashes() {
        assert_eq!(to_display(Path::new("nested\\b.csv")), "nested/b.csv");
    }

    /// F26c regression test (closes D6): a `Mismatch` JSON object missing `kind`/`algorithm`/
    /// `source_digest`/`dest_digest` entirely — exactly what a report written before those fields
    /// existed looks like — must still deserialize instead of failing the whole report. Only
    /// `path` is required.
    #[test]
    fn mismatch_with_missing_new_fields_still_deserializes() {
        let mismatch: Mismatch =
            serde_json::from_str(r#"{"path": "legacy.csv"}"#).expect("legacy shape must parse");
        assert_eq!(mismatch.path, "legacy.csv");
        assert_eq!(mismatch.kind, MismatchKind::Size);
        assert_eq!(mismatch.algorithm, "");
        assert_eq!(mismatch.source_digest, "");
        assert_eq!(mismatch.dest_digest, "");
    }

    fn check_with(missing: Vec<&str>, unreadable: Vec<&str>) -> IntegrityCheck {
        let mut check = IntegrityCheck {
            files_checked: 0,
            bytes_hashed: 0,
            mismatches: Vec::new(),
            missing_in_dest: missing.into_iter().map(str::to_string).collect(),
            unreadable: unreadable.into_iter().map(str::to_string).collect(),
            status: IntegrityStatus::Passed,
            truncated: false,
            total_errors: 0,
            skipped_unchanged: 0,
        };
        if !check.missing_in_dest.is_empty() || !check.unreadable.is_empty() {
            check.status = IntegrityStatus::Failed;
        }
        check
    }

    /// F26a: transient-pattern entries (`.log`, `.tmp`, `.git/objects/...`) are dropped and the
    /// check flips back to PASSED once nothing genuine remains.
    #[test]
    fn ignore_transient_missing_drops_only_transient_entries_and_recomputes_status() {
        let check = check_with(
            vec!["build.log", "nested/scratch.tmp", ".git/objects/ab/cdef"],
            vec!["another.log"],
        );
        let filtered = ignore_transient_missing(check);
        assert!(filtered.missing_in_dest.is_empty());
        assert!(filtered.unreadable.is_empty());
        assert!(filtered.passed());
    }

    /// F26a: a genuinely missing (non-transient) file must still fail verification even with
    /// `--ignore-transient-missing`.
    #[test]
    fn ignore_transient_missing_keeps_non_transient_entries_failing() {
        let check = check_with(vec!["build.log", "important.csv"], vec![]);
        let filtered = ignore_transient_missing(check);
        assert_eq!(filtered.missing_in_dest, vec!["important.csv".to_string()]);
        assert!(!filtered.passed());
    }

    #[test]
    fn is_transient_path_matches_documented_patterns() {
        assert!(is_transient_path("build.log"));
        assert!(is_transient_path("nested/scratch.TMP"));
        assert!(is_transient_path(".git/objects/ab/cdef1234"));
        assert!(!is_transient_path("important.csv"));
    }

    #[test]
    fn mismatch_missing_path_fails_to_deserialize() {
        // Unlike the other fields, `path` has no default: an entry with nothing identifying which
        // file it's about is not a usable mismatch record.
        assert!(serde_json::from_str::<Mismatch>(r#"{"kind": "size"}"#).is_err());
    }
}
