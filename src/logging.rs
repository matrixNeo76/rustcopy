//! Asynchronous file logger.
//!
//! `tracing` events are formatted on the calling thread and then handed to a *bounded*
//! `tokio::sync::mpsc` channel (capacity `CHANNEL_CAPACITY`) via `try_send`, so a burst of log
//! lines can never grow without limit and OOM a 1TB+ transfer. A dedicated tokio task owns the
//! log file and performs every write, so the copy loop and the progress bar never block on disk
//! I/O. Because `try_send` is non-blocking, lines are dropped rather than buffered without bound
//! when the channel is full; [`LogHandle::dropped_lines`] reports how many, so a run under heavy
//! logging pressure can flag this in its report instead of silently losing audit trail entries.
//!
//! Logs go to the file only: the terminal is reserved for the progress bar and the final summary.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

use crate::errors::IngestError;

/// Bound on the in-flight log message queue.
const CHANNEL_CAPACITY: usize = 10_000;

/// Default verbosity: INFO for this crate, WARN for dependencies. D18 (closes the default-level
/// half of D9, which shipped as DEBUG): DEBUG emits one line per file copied. Measured on a
/// 1.34M-file real-world run (`_ops_reports/full-profile-test.log`): 356 MB at DEBUG, ~4.6 MB at
/// INFO — a 76x difference confirmed on a 300-file sample too (67 KB vs 877 bytes). Per-file detail
/// is still available via `--log-level debug` when actually diagnosing a specific run.
pub const DEFAULT_FILTER: &str = "robocopy_ingest=info,warn";

/// F27 (closes D9) / D18 (this threshold is now also enforced *during* the current run, not just
/// against the previous one — see `rotate_if_needed` and the writer task in `build`; D9's own
/// fix explicitly documented this as "not live rotation", which D18 closes). ~20MB matches the
/// field-observed ~19MB for 59,963 files at the old default DEBUG level (see `ANALYSIS.md` D9).
pub const DEFAULT_MAX_LOG_BYTES: u64 = 20 * 1024 * 1024;
/// F27: default number of rotated backups kept (`<path>.1` .. `<path>.N`, oldest dropped first).
pub const DEFAULT_MAX_LOG_BACKUPS: u32 = 3;

/// F27: what verbosity to log at and how aggressively to rotate. `filter` is only the *fallback*
/// used when the `RUST_LOG` env var isn't set — `RUST_LOG` still wins, exactly as before this
/// struct existed.
#[derive(Debug, Clone)]
pub struct LogConfig {
    pub filter: String,
    /// Rotate the previous log aside once it reaches this size. `0` disables rotation.
    pub max_bytes: u64,
    /// How many rotated backups to keep. `0` disables rotation (nothing to rotate into).
    pub max_backups: u32,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filter: DEFAULT_FILTER.to_string(),
            max_bytes: DEFAULT_MAX_LOG_BYTES,
            max_backups: DEFAULT_MAX_LOG_BACKUPS,
        }
    }
}

enum LogMessage {
    Line(Vec<u8>),
    Flush(oneshot::Sender<()>),
    /// Stop the writer task.
    Stop(oneshot::Sender<()>),
}

/// `io::Write` shim that forwards formatted log lines to the writer task.
#[derive(Clone)]
struct ChannelWriter {
    sender: mpsc::Sender<LogMessage>,
    dropped: Arc<AtomicU64>,
}

impl io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Use try_send so logging never blocks the caller or causes OOM on 1TB+ transfers.
        // A full channel means lines are dropped; track how many so the report can surface it.
        if self
            .sender
            .try_send(LogMessage::Line(buf.to_vec()))
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for ChannelWriter {
    type Writer = ChannelWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Handle used to flush and drain the logger before the process exits.
pub struct LogHandle {
    sender: mpsc::Sender<LogMessage>,
    task: tokio::task::JoinHandle<()>,
    path: PathBuf,
    dropped: Arc<AtomicU64>,
}

impl LogHandle {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of log lines dropped because the bounded channel was full.
    pub fn dropped_lines(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Wait until every buffered line has reached the file.
    pub async fn flush(&self) {
        let (ack_tx, ack_rx) = oneshot::channel();
        if self.sender.send(LogMessage::Flush(ack_tx)).await.is_ok() {
            let _ = ack_rx.await;
        }
    }

    /// Flush, stop the writer task and wait for it to finish.
    pub async fn shutdown(self) {
        let (ack_tx, ack_rx) = oneshot::channel();
        if self.sender.send(LogMessage::Stop(ack_tx)).await.is_ok() {
            let _ = ack_rx.await;
        }
        let _ = self.task.await;
    }
}

/// Install the global tracing subscriber writing asynchronously to `path`.
///
/// Must be called from within a tokio runtime. Returns the handle used to flush on shutdown.
pub fn init(path: &Path, config: &LogConfig) -> Result<LogHandle, IngestError> {
    let (subscriber, handle) = build(path, config)?;
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        tracing::debug!("a tracing subscriber was already installed; reusing it");
    }
    Ok(handle)
}

/// Same as [`init`] but scoped to the current thread, so several loggers can coexist in one
/// process. Used by the tests, where a single global subscriber would be shared across cases.
pub fn init_scoped(
    path: &Path,
    config: &LogConfig,
) -> Result<(tracing::subscriber::DefaultGuard, LogHandle), IngestError> {
    let (subscriber, handle) = build(path, config)?;
    Ok((tracing::subscriber::set_default(subscriber), handle))
}

/// F27: rotate the existing log aside (`<path>` -> `<path>.1`, `<path>.1` -> `<path>.2`, ...,
/// oldest beyond `max_backups` dropped) if it's already at or past `max_bytes`. Best-effort: a
/// failed rotation (e.g. a backup file locked by another process) must never stop logging from
/// starting, so errors here are swallowed rather than propagated.
fn rotate_if_needed(path: &Path, max_bytes: u64, max_backups: u32) {
    if max_bytes == 0 || max_backups == 0 {
        return;
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return; // nothing to rotate yet
    };
    if metadata.len() < max_bytes {
        return;
    }

    let _ = std::fs::remove_file(backup_path(path, max_backups));
    for n in (1..max_backups).rev() {
        let from = backup_path(path, n);
        if from.exists() {
            let _ = std::fs::rename(&from, backup_path(path, n + 1));
        }
    }
    let _ = std::fs::rename(path, backup_path(path, 1));
}

fn backup_path(path: &Path, n: u32) -> PathBuf {
    let mut os = path.as_os_str().to_owned();
    os.push(format!(".{n}"));
    PathBuf::from(os)
}

fn build(
    path: &Path,
    config: &LogConfig,
) -> Result<(impl tracing::Subscriber + Send + Sync, LogHandle), IngestError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| IngestError::io(parent, error))?;
        }
    }

    rotate_if_needed(path, config.max_bytes, config.max_backups);

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| IngestError::io(path, error))?;
    // D18: starting size after the startup rotation above, so mid-run rotation (below) triggers
    // at the same `max_bytes` total-file-size threshold `rotate_if_needed` itself uses, not a
    // separate "bytes written by this process" count that could let a resumed/appended file grow
    // well past the configured limit before the first in-run check fires.
    let mut bytes_written = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut file = tokio::fs::File::from_std(file);

    let (sender, mut receiver) = mpsc::channel::<LogMessage>(CHANNEL_CAPACITY);
    let dropped = Arc::new(AtomicU64::new(0));
    let rotation_path = path.to_path_buf();
    let max_bytes = config.max_bytes;
    let max_backups = config.max_backups;

    let task = tokio::spawn(async move {
        while let Some(message) = receiver.recv().await {
            match message {
                LogMessage::Line(bytes) => {
                    if file.write_all(&bytes).await.is_err() {
                        break;
                    }
                    bytes_written += bytes.len() as u64;

                    // D18 (closes D9's own "not live rotation" caveat): `rotate_if_needed` at
                    // startup only ever rotates the *previous* run's log — a single long run that
                    // crosses `max_bytes` mid-flight grew unbounded (this is exactly what
                    // produced the 356 MB / 1.34M-line log in `_ops_reports/full-profile-test.log`).
                    // Rotating here reuses the same rename-chain logic, not a separate code path.
                    if max_bytes > 0 && max_backups > 0 && bytes_written >= max_bytes {
                        let _ = file.flush().await;
                        // Windows note: Rust opens files with FILE_SHARE_DELETE, so renaming an
                        // open handle's path succeeds even without closing it first -- but
                        // `into_std().await` (waits for in-flight ops, then hands back a plain
                        // std::fs::File whose Drop closes the OS handle synchronously) removes
                        // any doubt rather than relying on that share-mode detail.
                        drop(file.into_std().await);
                        rotate_if_needed(&rotation_path, max_bytes, max_backups);
                        match std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&rotation_path)
                        {
                            Ok(fresh) => {
                                // Not unconditionally 0: `rotate_if_needed` is best-effort and
                                // swallows its own errors (e.g. a backup file locked by another
                                // process), so a failed rotation leaves the *same* oversized file
                                // still at `rotation_path` -- reopening it in append mode returns a
                                // handle whose size is still >= max_bytes, not 0. Seeding from the
                                // real size here (same pattern as the initial open above) means a
                                // persistent rotation failure re-checks every `max_bytes` written
                                // instead of silently growing the file to multiples of the cap
                                // before the next check ever fires.
                                bytes_written = fresh.metadata().map(|m| m.len()).unwrap_or(0);
                                file = tokio::fs::File::from_std(fresh);
                            }
                            // Can't reopen (e.g. the directory vanished) -- stop logging rather
                            // than panic the writer task; the run itself must not be affected.
                            // `return` (not `break`): `file` was already consumed by
                            // `into_std()` above and never reassigned on this path, so the
                            // shared post-loop flush below has nothing valid left to flush.
                            Err(_) => return,
                        }
                    }
                }
                LogMessage::Flush(ack) => {
                    let _ = file.flush().await;
                    let _ = ack.send(());
                }
                LogMessage::Stop(ack) => {
                    let _ = file.flush().await;
                    let _ = ack.send(());
                    break;
                }
            }
        }
        let _ = file.flush().await;
    });

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(config.filter.clone()));
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(ChannelWriter {
            sender: sender.clone(),
            dropped: Arc::clone(&dropped),
        })
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .finish();

    Ok((
        subscriber,
        LogHandle {
            sender,
            task,
            path: path.to_path_buf(),
            dropped,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_are_flushed_to_the_log_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("logs/ingest.log");

        let (_guard, handle) = init_scoped(&path, &LogConfig::default()).expect("logger starts");
        tracing::info!("ingestion started");
        handle.flush().await;

        let contents = std::fs::read_to_string(&path).expect("read log");
        assert!(contents.contains("ingestion started"), "got: {contents}");

        handle.shutdown().await;
    }

    /// D18: the default level is INFO, not DEBUG — DEBUG's per-file line (`robocopy transferred
    /// file`, one per file copied) is what produced a 356 MB log on a 1.34M-file real run
    /// (`_ops_reports/full-profile-test.log`). Suppressed by default now; still available via
    /// `--log-level debug` (`Args::log_config()` builds the filter string this test constructs
    /// directly, since `LogConfig::default()` always uses `RUST_LOG`-independent `DEFAULT_FILTER`).
    #[tokio::test]
    async fn debug_detail_is_suppressed_by_default_but_available_when_requested() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");

        let (_guard, handle) = init_scoped(&path, &LogConfig::default()).expect("logger starts");
        tracing::info!("ingestion started");
        tracing::debug!(file = "part-0001.csv", "per-file detail");
        handle.flush().await;
        handle.shutdown().await;

        let contents = std::fs::read_to_string(&path).expect("read log");
        assert!(contents.contains("ingestion started"));
        assert!(
            !contents.contains("per-file detail"),
            "DEBUG must be suppressed by the new INFO default, got: {contents}"
        );

        let path2 = dir.path().join("ingest_debug.log");
        let config = LogConfig {
            filter: "robocopy_ingest=debug,warn".to_string(),
            ..LogConfig::default()
        };
        let (_guard2, handle2) = init_scoped(&path2, &config).expect("logger starts");
        tracing::debug!(file = "part-0001.csv", "per-file detail");
        handle2.flush().await;
        handle2.shutdown().await;

        let contents2 = std::fs::read_to_string(&path2).expect("read log");
        assert!(
            contents2.contains("per-file detail"),
            "explicit --log-level debug must still record per-file detail, got: {contents2}"
        );
    }

    #[tokio::test]
    async fn log_lines_carry_a_timestamp_and_a_level() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");

        let (_guard, handle) = init_scoped(&path, &LogConfig::default()).expect("logger starts");
        tracing::warn!("retrying after incomplete copy");
        handle.flush().await;

        let contents = std::fs::read_to_string(&path).expect("read log");
        let line = contents
            .lines()
            .find(|l| l.contains("retrying after incomplete copy"))
            .expect("line present");
        assert!(line.contains("WARN"), "level missing in {line:?}");
        // Default tracing-subscriber timestamps are RFC 3339-ish and start with the year.
        assert!(line.starts_with("20"), "timestamp missing in {line:?}");

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn parent_directories_are_created() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("deep/nested/tree/ingest.log");
        let (_guard, handle) = init_scoped(&path, &LogConfig::default()).expect("logger starts");
        handle.shutdown().await;
        assert!(path.is_file());
    }

    #[tokio::test]
    async fn appends_to_an_existing_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        std::fs::write(&path, "previous run\n").expect("seed log");

        let (_guard, handle) = init_scoped(&path, &LogConfig::default()).expect("logger starts");
        tracing::error!("second run");
        handle.flush().await;

        let contents = std::fs::read_to_string(&path).expect("read log");
        assert!(
            contents.starts_with("previous run"),
            "existing content preserved"
        );
        assert!(contents.contains("second run"));
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn dropped_lines_starts_at_zero() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        let (_guard, handle) = init_scoped(&path, &LogConfig::default()).expect("logger starts");
        assert_eq!(handle.dropped_lines(), 0);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn logging_after_the_writer_task_stops_does_not_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");

        let (guard, handle) = init_scoped(&path, &LogConfig::default()).expect("logger starts");
        handle.shutdown().await;
        tracing::info!("dropped on the floor");
        drop(guard);
    }

    /// F27: a `warn`-only filter (what `--quiet` and `--log-level warn` both produce) drops
    /// per-file DEBUG noise but keeps WARN/ERROR — the actual point of the flag.
    #[tokio::test]
    async fn quiet_style_filter_drops_debug_but_keeps_warnings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        let config = LogConfig {
            filter: "robocopy_ingest=warn,warn".to_string(),
            ..LogConfig::default()
        };

        let (_guard, handle) = init_scoped(&path, &config).expect("logger starts");
        tracing::debug!(file = "part-0001.csv", "per-file detail");
        tracing::warn!("retrying after incomplete copy");
        handle.flush().await;

        let contents = std::fs::read_to_string(&path).expect("read log");
        assert!(
            !contents.contains("per-file detail"),
            "DEBUG must be suppressed under a warn-only filter, got: {contents}"
        );
        assert!(contents.contains("retrying after incomplete copy"));

        handle.shutdown().await;
    }

    /// F27 (closes D9): the previous run's log is rotated aside once it reaches `max_bytes`,
    /// instead of growing without bound across repeated runs against the same `--log-path`.
    #[tokio::test]
    async fn oversized_log_is_rotated_aside_before_a_new_run_starts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        std::fs::write(&path, "x".repeat(100)).expect("seed an oversized log");

        let config = LogConfig {
            // D18: 90 sits strictly between the ~79-byte formatted "fresh run" line and the
            // 100-byte seeded content -- big enough that the "fresh run" write below doesn't
            // itself cross max_bytes and trigger a *second*, mid-run rotation on top of the
            // startup one this test is actually checking, but still small enough that the
            // seeded oversized content does trigger that startup rotation in the first place.
            max_bytes: 90,
            max_backups: 2,
            ..LogConfig::default()
        };
        let (_guard, handle) = init_scoped(&path, &config).expect("logger starts");
        tracing::info!("fresh run");
        handle.flush().await;
        handle.shutdown().await;

        let rotated =
            std::fs::read_to_string(backup_path(&path, 1)).expect("rotated backup exists");
        assert_eq!(
            rotated,
            "x".repeat(100),
            "the old oversized content must be preserved, not lost"
        );

        let fresh = std::fs::read_to_string(&path).expect("fresh log exists");
        assert!(
            !fresh.contains(&"x".repeat(100)),
            "old content must not linger in the fresh file"
        );
        assert!(
            fresh.contains("fresh run"),
            "the new run must log into a fresh file"
        );
    }

    /// F27: rotation keeps only `max_backups` files, oldest dropped, not accumulated forever.
    #[tokio::test]
    async fn rotation_respects_max_backups() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        let config = LogConfig {
            max_bytes: 10,
            max_backups: 2,
            ..LogConfig::default()
        };

        for generation in 0..4 {
            std::fs::write(&path, format!("generation-{generation}-{}", "x".repeat(10)))
                .expect("seed oversized log");
            let (_guard, handle) = init_scoped(&path, &config).expect("logger starts");
            handle.shutdown().await;
        }

        assert!(backup_path(&path, 1).is_file());
        assert!(backup_path(&path, 2).is_file());
        assert!(
            !backup_path(&path, 3).exists(),
            "only max_backups=2 rotated files must be kept"
        );
        // 4 iterations rotate generation-0..generation-3 in turn; with max_backups=2 only the two
        // most recent (2 and 3) survive — 0 and 1 must have been dropped as the oldest.
        let newest_backup = std::fs::read_to_string(backup_path(&path, 1)).expect("read slot 1");
        assert!(
            newest_backup.contains("generation-3"),
            "got: {newest_backup}"
        );
        let older_backup = std::fs::read_to_string(backup_path(&path, 2)).expect("read slot 2");
        assert!(older_backup.contains("generation-2"), "got: {older_backup}");
    }

    #[test]
    fn rotation_is_skipped_when_disabled_via_zero_max_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        std::fs::write(&path, "x".repeat(1000)).expect("seed");
        rotate_if_needed(&path, 0, 3);
        assert!(
            !backup_path(&path, 1).exists(),
            "max_bytes=0 must disable rotation"
        );
    }

    /// D18 (closes D9's "not live rotation" caveat): a *single* run that writes enough lines to
    /// cross `max_bytes` mid-flight must rotate right then, not just at the next process's
    /// startup — this is the exact gap that let one real run grow a 356 MB log
    /// (`_ops_reports/full-profile-test.log`) with rotation configured but never triggered.
    #[tokio::test]
    async fn a_single_run_rotates_mid_flight_once_it_crosses_max_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        let config = LogConfig {
            filter: "robocopy_ingest=info,warn".to_string(),
            max_bytes: 200,
            max_backups: 2,
        };

        let (_guard, handle) = init_scoped(&path, &config).expect("logger starts");
        // Each line is long enough that a handful of them cross the 200-byte threshold within
        // this one process, with no second `init_scoped` call involved.
        for i in 0..40 {
            tracing::info!(
                iteration = i,
                "a moderately long line to accumulate bytes quickly"
            );
        }
        handle.flush().await;
        handle.shutdown().await;

        assert!(
            backup_path(&path, 1).is_file(),
            "at least one mid-run rotation must have happened"
        );
        let current_len = std::fs::metadata(&path).expect("current log exists").len();
        assert!(
            current_len < 400,
            "the live/current log must not have kept accumulating past the threshold \
             unrotated, got {current_len} bytes"
        );
        // The most recent line must not be lost. With only max_backups=2 slots and ~40 lines
        // crossing the 200-byte threshold many times over, the earliest iterations are
        // legitimately pruned by the same "oldest backup dropped" policy
        // `rotation_respects_max_backups` already covers -- this test's job is proving rotation
        // triggers *within* a run, not that every historical line survives forever. Whether the
        // very last write landed in the live file or in backup slot 1 depends on exactly which
        // write crossed the threshold, so check both rather than assuming one deterministically.
        let mut recent = std::fs::read_to_string(&path).expect("read current log");
        if let Ok(backup1) = std::fs::read_to_string(backup_path(&path, 1)) {
            recent.push_str(&backup1);
        }
        assert!(
            recent.contains("iteration=39"),
            "the most recent line must not be lost, got: {recent}"
        );
    }

    /// D18: a fresh run (no pre-existing log, so `bytes_written` starts at 0) whose total output
    /// stays under `max_bytes` must never rotate at all.
    #[tokio::test]
    async fn a_run_under_max_bytes_never_rotates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        let config = LogConfig {
            filter: "robocopy_ingest=info,warn".to_string(),
            max_bytes: 1024 * 1024,
            max_backups: 2,
        };

        let (_guard, handle) = init_scoped(&path, &config).expect("logger starts");
        tracing::info!("a single short line");
        handle.flush().await;
        handle.shutdown().await;

        assert!(
            !backup_path(&path, 1).exists(),
            "well under max_bytes must never trigger rotation"
        );
    }

    /// Found by CodeRabbit reviewing the PR that introduced mid-run rotation above: after a
    /// *failed* rotation attempt, `bytes_written` was reset to 0 unconditionally, even though
    /// `rotate_if_needed` is best-effort and swallows its own errors -- a failed rename leaves the
    /// same oversized file sitting at the reopened path, not an empty one. Reproduced by blocking
    /// the rename step with a pre-existing directory at the backup slot (fails the same way on
    /// both Windows and Linux), then unblocking it before a second, deliberately short write: a
    /// short line alone never crosses `max_bytes` from a wrongly-reset 0, so a still-buggy
    /// implementation would never re-attempt rotation and `backup_path(&path, 1)` would still be
    /// the blocking directory, not a rotated-log file, after this test's writes.
    #[tokio::test]
    async fn a_failed_rotation_still_reseeds_the_byte_counter_from_the_real_file_size() {
        let dir = tempfile::tempdir().expect("tempdir");

        // Measure one minimal formatted line's real byte size empirically instead of guessing it
        // (timestamp/level/target overhead is an internal `tracing_subscriber` formatting detail),
        // so the thresholds below stay correct even if that formatting ever changes.
        let probe_path = dir.path().join("probe.log");
        let probe_config = LogConfig {
            filter: "robocopy_ingest=info,warn".to_string(),
            max_bytes: 0,
            max_backups: 0,
        };
        let short_line_len = {
            let (_guard, handle) = init_scoped(&probe_path, &probe_config).expect("logger starts");
            tracing::info!("x");
            handle.flush().await;
            handle.shutdown().await;
            std::fs::metadata(&probe_path)
                .expect("probe log exists")
                .len()
        };

        let path = dir.path().join("ingest.log");
        // Comfortably between one short line's real size and the much longer padded line below.
        let max_bytes = short_line_len * 3;
        let config = LogConfig {
            filter: "robocopy_ingest=info,warn".to_string(),
            max_bytes,
            max_backups: 1,
        };

        // Block the first rotation attempt: an existing directory at the backup slot makes
        // `std::fs::rename(path, backup_path(path, 1))` fail, on both Windows and Linux.
        std::fs::create_dir(backup_path(&path, 1)).expect("create blocking dir");

        let (_guard, handle) = init_scoped(&path, &config).expect("logger starts");

        // Padded well past `max_bytes` on its own -- triggers a rotation attempt that fails
        // (blocked above), reopening the live file at its real, still-oversized size.
        let padding = "x".repeat(short_line_len as usize * 4);
        tracing::info!(pad = %padding, "long line");
        handle.flush().await;

        // Simulate the transient lock clearing before the next write.
        std::fs::remove_dir(backup_path(&path, 1)).expect("unblock rotation");

        // A single short line -- on its own (i.e. starting from a wrongly-reset bytes_written=0)
        // this stays under max_bytes. Only re-triggers (and this time succeeds at) rotation if
        // bytes_written was correctly reseeded from the real, still-oversized file after the
        // failed first attempt.
        tracing::info!("x");
        handle.flush().await;
        handle.shutdown().await;

        assert!(
            backup_path(&path, 1).is_file(),
            "a failed rotation must not reset the byte counter to 0 -- the very next short write \
             should still be enough to re-trigger (and this time succeed at) rotation, instead of \
             waiting for another full max_bytes worth of new output counted from zero"
        );
    }
}
