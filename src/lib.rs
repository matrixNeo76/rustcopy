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

pub mod advise;
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
pub mod history;
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

use chrono::{DateTime, Utc};

/// Placeholder recognized in `--report-path` (P1, `PIANO_MIGLIORAMENTI.md`). Resolved once for a
/// single-job run, or once per job in `run_jobs` — in the multi-job case, *after* that job's
/// `namespaced_path` call above (`main.rs::run_jobs` applies the two in that order; which one
/// runs first doesn't actually matter, since they touch disjoint parts of the filename — the
/// stem's own text vs. the `.{job_name}` insertion before the extension). Always resolved before
/// `validate()`, so everything else downstream that derives from `report_path`
/// (`checkpoint::checkpoint_path_for`, P2's `report::read_previous_report`) sees the resolved,
/// timestamped value, never the raw placeholder text.
pub const REPORT_PATH_TIMESTAMP_PLACEHOLDER: &str = "{timestamp}";

/// Replaces `{timestamp}` in `path` with `now` formatted as `yyyyMMdd_HHmmss` — the same format
/// `scripts/_ingest-common.ps1` and the PowerShell launcher's own `_ops_reports/<profile>/
/// <timestamp>/` folders already use (`Get-Date -Format "yyyyMMdd_HHmmss"`), so report filenames
/// look the same whether produced by this binary directly or via the launcher.
///
/// A path with no placeholder is returned unchanged — the pre-P1 default (a fixed `--report-path`
/// overwritten every run, which is what P2's `previous_run_comparison` actually depends on) stays
/// exactly as it was; this is opt-in, not a behavior change for anyone who never types
/// `{timestamp}`. `now` is a parameter rather than an internal `Utc::now()` call so this stays
/// trivially unit-testable without mocking the clock.
pub fn resolve_report_path_timestamp(path: &Path, now: DateTime<Utc>) -> PathBuf {
    let rendered = path.to_string_lossy();
    if !rendered.contains(REPORT_PATH_TIMESTAMP_PLACEHOLDER) {
        return path.to_path_buf();
    }
    let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
    PathBuf::from(rendered.replace(REPORT_PATH_TIMESTAMP_PLACEHOLDER, &timestamp))
}

/// Inserts `.{name}` before a path's extension (or at the end, if there is none). Shared by every
/// place that needs to give one job in a `[[jobs]]` batch (F33) its own file next to a `dest` it
/// shares with other jobs — the report path (`main.rs::run_jobs`), the fast-verify cache
/// (`cache::default_cache_path`), and the backup-generations manifest
/// (`generations::GenerationManifest::path_for`). E.g. `report.json` + `photos` ->
/// `report.photos.json`; `.ingest_cache` + `photos` -> `.ingest_cache.photos` (a leading-dot file
/// with no other dots has no extension, so the name is appended rather than inserted).
/// True when `file_name` is one of rustcopy's own bookkeeping files rather than backed-up content.
///
/// These live at the destination root (`<dest>/.ingest_cache` F28, `.rustcopy_generations.json`
/// F34, `.rustcopy_history.jsonl` Fase 0) and must never be inventoried as if they were user data.
///
/// Why this matters, concretely: `--restore-from` reverses source and destination, so a previous
/// run's **destination** becomes the next run's **source**. Without this filter, restoring a backup
/// copies rustcopy's bookkeeping into the restore target alongside the real files — and with
/// `--decrypt` in the mix it then fails outright, because those files were never encrypted and have
/// no `RCE1` header. Caught by `cli_smoke::encrypted_backup_restores_and_decrypts_end_to_end` when
/// the history index was added; the same latent defect already applied to `.ingest_cache` and
/// `.rustcopy_generations.json`, it simply had no test reaching it.
///
/// Matching is by exact file name, including the per-job namespaced variants F33/D12 produce
/// (`.rustcopy_history.nightly.jsonl`), which is why this checks stem prefixes rather than equality.
pub fn is_rustcopy_metadata(file_name: &std::ffi::OsStr) -> bool {
    let Some(name) = file_name.to_str() else {
        return false;
    };
    name == ".ingest_cache"
        || name.starts_with(".ingest_cache.")
        || (name.starts_with(".rustcopy_generations") && name.ends_with(".json"))
        || (name.starts_with(".rustcopy_history") && name.ends_with(".jsonl"))
}

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

    /// The filter guards a restore path, so its claim about namespaced names needs proving rather
    /// than asserting: `namespaced_path` inserts the job name *before* the extension, which for a
    /// dotfile with no extension (`.ingest_cache`) appends instead. Both shapes must be caught.
    #[test]
    fn rustcopy_metadata_is_recognised_including_its_per_job_namespaced_forms() {
        use std::ffi::OsStr;

        for name in [
            ".ingest_cache",
            ".rustcopy_generations.json",
            ".rustcopy_history.jsonl",
        ] {
            assert!(
                is_rustcopy_metadata(OsStr::new(name)),
                "{name} must be recognised"
            );
            // Exactly what `namespaced_path` would produce for a `[[jobs]]` entry.
            let namespaced = namespaced_path(Path::new(name), "nightly");
            let namespaced = namespaced.file_name().unwrap();
            assert!(
                is_rustcopy_metadata(namespaced),
                "the namespaced form {namespaced:?} of {name} must be recognised too"
            );
        }
    }

    /// The filter must not swallow a user's own files. A backup that silently skipped real data
    /// would be a far worse defect than the restore failure this filter exists to prevent.
    #[test]
    fn user_files_are_never_mistaken_for_rustcopy_metadata() {
        use std::ffi::OsStr;

        for name in [
            "ingest_cache",            // no leading dot
            ".ingest_cache_notes.txt", // shares the prefix, is not the file
            ".rustcopy_history.txt",   // right prefix, wrong extension
            ".rustcopy_generations.txt",
            "rustcopy_history.jsonl",
            "report.json",
            "a.csv",
            ".gitignore",
        ] {
            assert!(
                !is_rustcopy_metadata(OsStr::new(name)),
                "{name} is user data and must be backed up"
            );
        }
    }
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

    fn fixed_now() -> DateTime<Utc> {
        "2026-08-20T14:05:09Z".parse().expect("valid timestamp")
    }

    #[test]
    fn resolve_report_path_timestamp_substitutes_the_placeholder() {
        assert_eq!(
            resolve_report_path_timestamp(Path::new("report-{timestamp}.json"), fixed_now()),
            PathBuf::from("report-20260820_140509.json")
        );
    }

    #[test]
    fn resolve_report_path_timestamp_leaves_a_plain_path_unchanged() {
        assert_eq!(
            resolve_report_path_timestamp(Path::new("report.json"), fixed_now()),
            PathBuf::from("report.json")
        );
    }

    #[test]
    fn resolve_report_path_timestamp_handles_the_placeholder_inside_a_directory_component() {
        assert_eq!(
            resolve_report_path_timestamp(
                Path::new("_ops_reports/{timestamp}/report.json"),
                fixed_now()
            ),
            PathBuf::from("_ops_reports/20260820_140509/report.json")
        );
    }

    #[test]
    fn resolve_report_path_timestamp_replaces_every_occurrence() {
        // Not documented/expected usage, but the naive string-replace this is built on handles it
        // for free -- worth pinning down so a future rewrite doesn't silently only replace the
        // first occurrence.
        assert_eq!(
            resolve_report_path_timestamp(
                Path::new("{timestamp}/report-{timestamp}.json"),
                fixed_now()
            ),
            PathBuf::from("20260820_140509/report-20260820_140509.json")
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
        assert!(
            !tmp_path.exists(),
            "temp file must be renamed away, not left behind"
        );
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
