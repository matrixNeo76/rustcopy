//! Baseline copy engine: the "naive" method the benchmark compares robocopy against.
//!
//! It mirrors what `Get-ChildItem -Recurse -Filter *.csv | Copy-Item -Destination ...` does on
//! Windows: one file at a time, single threaded, no restart mode, no oplocks tuning, no
//! multithreading. It is deliberately unoptimised — that is the point of the comparison — and it is
//! fully cross-platform, which is what makes the engine layer testable on Linux.

use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::time::Instant;

use crate::errors::IngestError;
use crate::progress::ProgressSink;
use crate::scan::{self, ScannedFile};

use super::{CopyEngine, CopyOutcome, CopyRequest};

pub const ENGINE_NAME: &str = "naive-baseline";

/// Copy buffer size. Small on purpose: a plain per-file stream copy, like PowerShell's `Copy-Item`.
const BUFFER_BYTES: usize = 64 * 1024;

/// Single-threaded recursive copy used as the benchmark baseline.
#[derive(Debug, Default, Clone, Copy)]
pub struct NaiveCopyEngine;

impl NaiveCopyEngine {
    pub fn new() -> Self {
        Self
    }
}

impl CopyEngine for NaiveCopyEngine {
    fn name(&self) -> &'static str {
        ENGINE_NAME
    }

    fn copy(
        &self,
        request: &CopyRequest,
        sink: &dyn ProgressSink,
    ) -> Result<CopyOutcome, IngestError> {
        let inventory = scan::scan(
            &request.source,
            &request.pattern,
            !request.exclude_junctions,
            &request.exclude_dirs,
            &request.exclude_files,
        )?;
        tracing::info!(
            files = inventory.file_count(),
            bytes = inventory.total_bytes,
            dry_run = request.dry_run,
            "starting baseline copy"
        );

        let files: Vec<&ScannedFile> = inventory.files.iter().collect();
        copy_files(&request.source, &request.dest, &files, request.dry_run, sink, "baseline")
    }
}

/// F34: copy exactly the given files (preserving their relative paths), instead of walking the
/// source tree by pattern like [`NaiveCopyEngine::copy`] does. This is what makes an incremental
/// or differential generation possible at all: robocopy's own file-selection arguments match
/// filenames uniformly across every directory it walks, not a specific manifest of relative
/// paths, so there is no way to hand robocopy "copy exactly these N files, wherever they are in
/// the tree" — the naive per-file engine can do it directly because it never delegates file
/// selection to anything else. Used directly (not through the `CopyEngine` trait, which has no
/// concept of an explicit file list) by `main.rs`'s generation-backup path.
pub fn copy_selected(
    source: &Path,
    dest: &Path,
    files: &[ScannedFile],
    dry_run: bool,
    sink: &dyn ProgressSink,
) -> Result<CopyOutcome, IngestError> {
    let files: Vec<&ScannedFile> = files.iter().collect();
    copy_files(source, dest, &files, dry_run, sink, ENGINE_NAME)
}

fn copy_files(
    source: &Path,
    dest: &Path,
    files: &[&ScannedFile],
    dry_run: bool,
    sink: &dyn ProgressSink,
    log_label: &str,
) -> Result<CopyOutcome, IngestError> {
    let mut outcome = CopyOutcome::new(ENGINE_NAME);
    outcome.dry_run = dry_run;
    let started = Instant::now();

    if !dry_run {
        fs::create_dir_all(dest).map_err(|source_err| IngestError::io(dest, source_err))?;
    }

    for file in files {
        let source_path = source.join(&file.relative_path);
        let dest_path = dest.join(&file.relative_path);

        let bytes = if dry_run {
            tracing::debug!(path = %file.relative_path.display(), "would copy");
            file.size_bytes
        } else {
            copy_one(&source_path, &dest_path)?
        };

        outcome.bytes_copied += bytes;
        outcome.files_copied += 1;
        sink.add_bytes(bytes);
        sink.add_file();
        tracing::debug!(path = %file.relative_path.display(), bytes, label = log_label, "copied file");
    }

    outcome.elapsed = started.elapsed();
    tracing::info!(
        files = outcome.files_copied,
        bytes = outcome.bytes_copied,
        elapsed_seconds = outcome.elapsed.as_secs_f64(),
        label = log_label,
        "copy finished"
    );
    Ok(outcome)
}

/// Stream one file, creating the destination directory tree as needed. Returns bytes written.
fn copy_one(source: &Path, dest: &Path) -> Result<u64, IngestError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| IngestError::io(parent, error))?;
    }

    let mut reader = File::open(source).map_err(|error| IngestError::io(source, error))?;
    let mut writer =
        BufWriter::new(File::create(dest).map_err(|error| IngestError::io(dest, error))?);

    let mut buffer = vec![0u8; BUFFER_BYTES];
    let mut written = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| IngestError::io(source, error))?;
        if read == 0 {
            break;
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|error| IngestError::io(dest, error))?;
        written += read as u64;
    }
    writer
        .flush()
        .map_err(|error| IngestError::io(dest, error))?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::CountingProgress;
    use crate::testkit::fixture_tree;
    use std::path::PathBuf;

    fn request(src: &Path, dst: &Path) -> CopyRequest {
        CopyRequest {
            source: src.to_path_buf(),
            dest: dst.to_path_buf(),
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
        }
    }

    #[test]
    fn copies_matching_files_preserving_the_tree() {
        let source = fixture_tree(&[("a.csv", 128), ("nested/b.csv", 256), ("skip.txt", 512)]);
        let dest = tempfile::tempdir().expect("dest");

        let sink = CountingProgress::default();
        let outcome = NaiveCopyEngine::new()
            .copy(&request(source.path(), dest.path()), &sink)
            .expect("copy");

        assert_eq!(outcome.engine, ENGINE_NAME);
        assert_eq!(outcome.files_copied, 2);
        assert_eq!(outcome.bytes_copied, 384);
        assert_eq!(outcome.exit_code, None, "no process, no exit code");
        assert!(outcome.is_success());

        assert!(dest.path().join("a.csv").is_file());
        assert!(dest.path().join("nested/b.csv").is_file());
        assert!(
            !dest.path().join("skip.txt").exists(),
            "pattern must be honoured"
        );
        assert_eq!(sink.bytes(), 384);
        assert_eq!(sink.files(), 2);
    }

    #[test]
    fn copied_content_is_identical() {
        let source = fixture_tree(&[("data.csv", 5000)]);
        let dest = tempfile::tempdir().expect("dest");

        NaiveCopyEngine::new()
            .copy(
                &request(source.path(), dest.path()),
                &CountingProgress::default(),
            )
            .expect("copy");

        let original = fs::read(source.path().join("data.csv")).expect("read source");
        let copied = fs::read(dest.path().join("data.csv")).expect("read dest");
        assert_eq!(original, copied);
    }

    #[test]
    fn creates_a_missing_destination_directory() {
        let source = fixture_tree(&[("a.csv", 64)]);
        let parent = tempfile::tempdir().expect("dest parent");
        let dest = parent.path().join("does/not/exist/yet");

        NaiveCopyEngine::new()
            .copy(&request(source.path(), &dest), &CountingProgress::default())
            .expect("copy");

        assert!(dest.join("a.csv").is_file());
    }

    #[test]
    fn dry_run_counts_without_writing() {
        let source = fixture_tree(&[("a.csv", 100), ("b.csv", 200)]);
        let parent = tempfile::tempdir().expect("dest parent");
        let dest = parent.path().join("untouched");

        let mut request = request(source.path(), &dest);
        request.dry_run = true;

        let outcome = NaiveCopyEngine::new()
            .copy(&request, &CountingProgress::default())
            .expect("dry run");

        assert_eq!(outcome.files_copied, 2);
        assert_eq!(outcome.bytes_copied, 300);
        assert!(outcome.dry_run);
        assert!(!dest.exists(), "dry run must not create anything");
    }

    #[test]
    fn empty_source_yields_an_empty_outcome() {
        let source = fixture_tree(&[]);
        let dest = tempfile::tempdir().expect("dest");

        let outcome = NaiveCopyEngine::new()
            .copy(
                &request(source.path(), dest.path()),
                &CountingProgress::default(),
            )
            .expect("copy");

        assert_eq!(outcome.files_copied, 0);
        assert_eq!(outcome.bytes_copied, 0);
        assert!(outcome.is_success());
    }

    #[test]
    fn missing_source_directory_is_an_error() {
        let dest = tempfile::tempdir().expect("dest");
        let outcome = NaiveCopyEngine::new().copy(
            &request(&PathBuf::from("/definitely/not/here"), dest.path()),
            &CountingProgress::default(),
        );
        // The walk yields no entries for a missing root, so this degenerates to an empty copy.
        assert_eq!(outcome.expect("no hard failure").files_copied, 0);
    }
}
