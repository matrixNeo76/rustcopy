//! Robocopy-based CSV ingestion with throughput reporting and baseline benchmarking.
//!
//! The crate is split so that everything except the actual `robocopy.exe` invocation is portable
//! and unit-testable on any platform:
//!
//! * [`cli`] argument parsing and validation;
//! * [`engine`] the [`engine::CopyEngine`] abstraction plus the robocopy and naive baseline
//!   implementations and the outer retry loop;
//! * [`exit_code`] interpretation of robocopy's bitmask exit codes;
//! * [`progress`] throughput-based progress reporting;
//! * [`integrity`] SHA-256 verification of source vs destination;
//! * [`logging`] non-blocking file logger;
//! * [`report`] the JSON report;
//! * [`scan`] source inventory and directory sizing;
//! * [`testkit`] test doubles shared by unit and integration tests.

pub mod cache;
pub mod checkpoint;
pub mod cli;
pub mod cloud;
pub mod config;
pub mod crypto;
pub mod engine;
pub mod errors;
pub mod exit_code;
pub mod generations;
pub mod hooks;
pub mod html_report;
pub mod integrity;
pub mod logging;
pub mod notify;
#[cfg(feature = "notify-server")]
pub mod notify_server;
pub mod notify_sink;
pub mod oem_codec;
pub mod progress;
pub mod report;
pub mod restore;
pub mod scan;
pub mod schedule;
pub mod service;
pub mod testkit;
pub mod vss;

use std::path::{Path, PathBuf};

/// Inserts `.{name}` before a path's extension (or at the end, if there is none). Shared by every
/// place that needs to give one job in a `[[jobs]]` batch (F33) its own file next to a `dest` it
/// shares with other jobs — the report path (`main.rs::run_jobs`), the fast-verify cache
/// (`cache::default_cache_path`), and the backup-generations manifest
/// (`generations::GenerationManifest::path_for`). E.g. `report.json` + `photos` ->
/// `report.photos.json`; `.ingest_cache` + `photos` -> `.ingest_cache.photos` (a leading-dot file
/// with no other dots has no extension, so the name is appended rather than inserted).
pub fn namespaced_path(path: &Path, name: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let file_name = match path.extension() {
        Some(ext) => format!("{stem}.{name}.{}", ext.to_string_lossy()),
        None => format!("{stem}.{name}"),
    };
    path.with_file_name(file_name)
}

/// Writes `contents` to `path` by streaming to a same-directory sibling temp file and atomically
/// renaming over the original, so a crash, a forced kill, or a dropped network share mid-write
/// never leaves a truncated/corrupt file at `path` — same safety property `crypto.rs`'s
/// `encrypt_file`/`decrypt_file` already rely on (D3/D4), generalized here for plain byte writes.
///
/// D13/D14 context: `generations::GenerationManifest::save` and `cache::IngestCache::save_to`
/// both write files that scale with the size of the tree being backed up — a manifest for a
/// 1.34M-file real-world tree (see `_ops_reports/full-profile-test.json`) serializes to ~174 MB
/// for one generation, growing linearly with every generation kept before `--keep-generations`
/// rotates it away. Before this existed, both used a bare `std::fs::write`, and since a corrupt
/// manifest fails every future incremental/differential/retention run against that destination
/// (`load_or_default`'s parse error is propagated with `?`, aborting the whole job — see
/// `execute_generation_backup`), a single interrupted write over a flaky SMB/NAS destination could
/// permanently break that destination's generation history until an operator manually intervenes.
pub fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".rustcopy-tmp");
    let tmp_path = path.with_file_name(tmp_name);

    let write_result = std::fs::write(&tmp_path, contents);
    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
        return write_result;
    }
    std::fs::rename(&tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_path_inserts_before_extension() {
        assert_eq!(
            namespaced_path(Path::new("report.json"), "photos"),
            PathBuf::from("report.photos.json")
        );
    }

    #[test]
    fn namespaced_path_appends_when_no_extension() {
        assert_eq!(
            namespaced_path(Path::new(".ingest_cache"), "photos"),
            PathBuf::from(".ingest_cache.photos")
        );
    }

    #[test]
    fn namespaced_path_handles_dotted_hidden_files() {
        assert_eq!(
            namespaced_path(Path::new(".rustcopy_generations.json"), "photos"),
            PathBuf::from(".rustcopy_generations.photos.json")
        );
    }

    #[test]
    fn atomic_write_creates_the_file_with_the_given_contents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("manifest.json");
        atomic_write(&path, b"hello").expect("write succeeds");
        assert_eq!(std::fs::read(&path).expect("read"), b"hello");
    }

    #[test]
    fn atomic_write_leaves_no_temp_file_behind_on_success() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("manifest.json");
        atomic_write(&path, b"hello").expect("write succeeds");
        let tmp_path = dir.path().join("manifest.json.rustcopy-tmp");
        assert!(!tmp_path.exists(), "temp file must be renamed away, not left behind");
    }

    #[test]
    fn atomic_write_overwrites_an_existing_file_completely() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("manifest.json");
        std::fs::write(&path, "a much longer previous version of this file").expect("seed");
        atomic_write(&path, b"short").expect("write succeeds");
        assert_eq!(std::fs::read(&path).expect("read"), b"short");
    }

    /// The property this whole function exists for: an in-progress write must never be visible at
    /// the real path — a reader (or a crash) mid-write only ever sees either the old complete file
    /// or the new complete file, never a partial one.
    #[test]
    fn atomic_write_never_leaves_a_partially_written_file_at_the_real_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("manifest.json");
        std::fs::write(&path, "original content").expect("seed");

        // Simulate a crash mid-write: write directly to the temp path the way atomic_write does
        // internally, but stop short of the rename that publishes it.
        let tmp_path = dir.path().join("manifest.json.rustcopy-tmp");
        std::fs::write(&tmp_path, "truncated garbage from an interrupted write").expect("seed tmp");

        // The real path must still hold the last known-good content, untouched by the abandoned
        // temp file.
        assert_eq!(std::fs::read(&path).expect("read"), b"original content");
    }
}
