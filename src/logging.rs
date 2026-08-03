//! Asynchronous file logger.
//!
//! `tracing` events are formatted on the calling thread and then handed to a *bounded*
//! `tokio::sync::mpsc` channel (capacity [`CHANNEL_CAPACITY`]) via `try_send`, so a burst of log
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

/// Default verbosity: DEBUG for this crate (per-file detail), WARN for dependencies.
pub const DEFAULT_FILTER: &str = "robocopy_ingest=debug,warn";

/// F27 (closes D9): default rotation threshold. ~20MB matches the field-observed ~19MB for
/// 59,963 files at the default DEBUG level (see `ANALYSIS.md` D9) — past this, the *previous*
/// run's log is rotated aside rather than left to grow unbounded across repeated runs against the
/// same `--log-path`.
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
        if self.sender.try_send(LogMessage::Line(buf.to_vec())).is_err() {
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
    let mut file = tokio::fs::File::from_std(file);

    let (sender, mut receiver) = mpsc::channel::<LogMessage>(CHANNEL_CAPACITY);
    let dropped = Arc::new(AtomicU64::new(0));

    let task = tokio::spawn(async move {
        while let Some(message) = receiver.recv().await {
            match message {
                LogMessage::Line(bytes) => {
                    if file.write_all(&bytes).await.is_err() {
                        break;
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

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.filter.clone()));
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
        tracing::debug!(file = "part-0001.csv", "per-file detail");
        handle.flush().await;

        let contents = std::fs::read_to_string(&path).expect("read log");
        assert!(contents.contains("ingestion started"), "got: {contents}");
        assert!(
            contents.contains("per-file detail"),
            "DEBUG must be recorded by default"
        );

        handle.shutdown().await;
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
            max_bytes: 50,
            max_backups: 2,
            ..LogConfig::default()
        };
        let (_guard, handle) = init_scoped(&path, &config).expect("logger starts");
        tracing::info!("fresh run");
        handle.flush().await;
        handle.shutdown().await;

        let rotated = std::fs::read_to_string(backup_path(&path, 1)).expect("rotated backup exists");
        assert_eq!(rotated, "x".repeat(100), "the old oversized content must be preserved, not lost");

        let fresh = std::fs::read_to_string(&path).expect("fresh log exists");
        assert!(!fresh.contains(&"x".repeat(100)), "old content must not linger in the fresh file");
        assert!(fresh.contains("fresh run"), "the new run must log into a fresh file");
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
        assert!(newest_backup.contains("generation-3"), "got: {newest_backup}");
        let older_backup = std::fs::read_to_string(backup_path(&path, 2)).expect("read slot 2");
        assert!(older_backup.contains("generation-2"), "got: {older_backup}");
    }

    #[test]
    fn rotation_is_skipped_when_disabled_via_zero_max_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        std::fs::write(&path, "x".repeat(1000)).expect("seed");
        rotate_if_needed(&path, 0, 3);
        assert!(!backup_path(&path, 1).exists(), "max_bytes=0 must disable rotation");
    }
}
