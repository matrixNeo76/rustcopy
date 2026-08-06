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
}
