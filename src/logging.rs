//! Asynchronous file logger.
//!
//! `tracing` events are formatted on the calling thread and then handed to an unbounded
//! `tokio::sync::mpsc` channel. A dedicated tokio task owns the log file and performs every write,
//! so the copy loop and the progress bar never block on disk I/O — a real concern when the log
//! lives on the same spindle as a 50 GB ingestion.
//!
//! Logs go to the file only: the terminal is reserved for the progress bar and the final summary.

use std::io;
use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

use crate::errors::IngestError;

/// Default verbosity: DEBUG for this crate (per-file detail), WARN for dependencies.
pub const DEFAULT_FILTER: &str = "robocopy_ingest=debug,warn";

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
}

impl io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Use try_send so logging never blocks the caller or causes OOM on 1TB+ transfers.
        let _ = self.sender.try_send(LogMessage::Line(buf.to_vec()));
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
}

impl LogHandle {
    pub fn path(&self) -> &Path {
        &self.path
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
pub fn init(path: &Path) -> Result<LogHandle, IngestError> {
    let (subscriber, handle) = build(path)?;
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        tracing::debug!("a tracing subscriber was already installed; reusing it");
    }
    Ok(handle)
}

/// Same as [`init`] but scoped to the current thread, so several loggers can coexist in one
/// process. Used by the tests, where a single global subscriber would be shared across cases.
pub fn init_scoped(
    path: &Path,
) -> Result<(tracing::subscriber::DefaultGuard, LogHandle), IngestError> {
    let (subscriber, handle) = build(path)?;
    Ok((tracing::subscriber::set_default(subscriber), handle))
}

fn build(path: &Path) -> Result<(impl tracing::Subscriber + Send + Sync, LogHandle), IngestError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| IngestError::io(parent, error))?;
        }
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| IngestError::io(path, error))?;
    let mut file = tokio::fs::File::from_std(file);

    let (sender, mut receiver) = mpsc::channel::<LogMessage>(10_000);

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

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(ChannelWriter {
            sender: sender.clone(),
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

        let (_guard, handle) = init_scoped(&path).expect("logger starts");
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

        let (_guard, handle) = init_scoped(&path).expect("logger starts");
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
        let (_guard, handle) = init_scoped(&path).expect("logger starts");
        handle.shutdown().await;
        assert!(path.is_file());
    }

    #[tokio::test]
    async fn appends_to_an_existing_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");
        std::fs::write(&path, "previous run\n").expect("seed log");

        let (_guard, handle) = init_scoped(&path).expect("logger starts");
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
    async fn logging_after_the_writer_task_stops_does_not_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ingest.log");

        let (guard, handle) = init_scoped(&path).expect("logger starts");
        handle.shutdown().await;
        tracing::info!("dropped on the floor");
        drop(guard);
    }
}
