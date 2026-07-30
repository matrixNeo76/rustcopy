//! Integrity verification of source vs destination.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegrityStatus {
    #[serde(rename = "PASSED")]
    Passed,
    #[serde(rename = "FAILED")]
    Failed,
}

/// A file whose destination checksum differs from the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mismatch {
    pub path: String,
    pub source_sha256: String,
    pub dest_sha256: String,
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
}

impl IntegrityCheck {
    pub fn passed(&self) -> bool {
        self.status == IntegrityStatus::Passed
    }
}

/// Hash a file using the specified algorithm (Sha256 or Blake3), lowercase hex output.
pub fn hash_file(path: &Path, algo: HashAlgorithm) -> Result<String, IngestError> {
    match algo {
        HashAlgorithm::Sha256 => sha256_file(path),
        HashAlgorithm::Blake3 => blake3_file(path),
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
                    source_sha256: format!("size:{}B", file.size_bytes),
                    dest_sha256: format!("size:{}B", dest_meta.len()),
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
                            source_sha256: source_hash,
                            dest_sha256: dest_hash,
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

        let inventory = scan::scan(source.path(), "*.csv").expect("scan");
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

        let inventory = scan::scan(source.path(), "*.csv").expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Sha256, &NoopProgress);

        assert!(!check.passed());
        assert_eq!(check.status, IntegrityStatus::Failed);
        assert_eq!(check.mismatches.len(), 1);
        assert_eq!(check.mismatches[0].path, "b.csv");
        assert_ne!(
            check.mismatches[0].source_sha256,
            check.mismatches[0].dest_sha256
        );
    }

    #[test]
    fn truncated_destination_is_detected() {
        let source = fixture_tree(&[("a.csv", 2048)]);
        let dest = tempfile::tempdir().expect("dest");
        copy_tree(source.path(), dest.path());
        write_fixture_file(&dest.path().join("a.csv"), 1024);

        let inventory = scan::scan(source.path(), "*.csv").expect("scan");
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

        let inventory = scan::scan(source.path(), "*.csv").expect("scan");
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

        let inventory = scan::scan(source.path(), "*.csv").expect("scan");
        let check = verify(source.path(), dest.path(), &inventory.files, HashAlgorithm::Blake3, &NoopProgress);
        assert!(check.passed());
        assert_eq!(check.files_checked, 1);
    }

    #[test]
    fn relative_paths_are_reported_with_forward_slashes() {
        assert_eq!(to_display(Path::new("nested\\b.csv")), "nested/b.csv");
    }
}
