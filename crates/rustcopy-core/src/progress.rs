//! Throughput-based progress reporting.
//!
//! The progress bar is driven by *bytes*, not by file count, so the displayed rate, ETA and
//! percentage all reflect real throughput. Two independent sources feed it:
//!
//! 1. the copy engine, which reports the size of every file it completes
//!    (robocopy's per-file output is parsed thanks to `/BYTES`);
//! 2. a poller that samples the destination directory size.
//!
//! The displayed position is the maximum of the two, which keeps the bar monotonic even when one
//! source lags behind. Limitations are documented in the README.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressStyle};

/// Sink for progress events emitted by a [`crate::engine::CopyEngine`].
pub trait ProgressSink: Send + Sync {
    /// Report that `bytes` more bytes have been transferred (for one completed file).
    fn add_bytes(&self, bytes: u64);
    /// Report that one more file has been completed.
    fn add_file(&self);
    /// Report an absolute observed byte count (used by the destination-size poller).
    fn observe_total_bytes(&self, bytes: u64);
    /// Attach a free-form status message.
    fn set_status(&self, message: &str);
    /// Zero the byte/file counters. Called between retry attempts so a failed attempt's partial
    /// progress isn't added to the next attempt's, which would otherwise inflate the reported
    /// `bytes_copied` on the failure path (each retry re-copies overlapping files).
    fn reset(&self);
}

/// Progress sink that discards everything. Used by tests and by `--dry-run`.
#[derive(Debug, Default, Clone)]
pub struct NoopProgress;

impl ProgressSink for NoopProgress {
    fn add_bytes(&self, _bytes: u64) {}
    fn add_file(&self) {}
    fn observe_total_bytes(&self, _bytes: u64) {}
    fn set_status(&self, _message: &str) {}
    fn reset(&self) {}
}

/// Sink that only accumulates counters, without any terminal output. Handy in tests.
#[derive(Debug, Default)]
pub struct CountingProgress {
    bytes: AtomicU64,
    files: AtomicU64,
}

impl CountingProgress {
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    pub fn files(&self) -> u64 {
        self.files.load(Ordering::Relaxed)
    }
}

impl ProgressSink for CountingProgress {
    fn add_bytes(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    fn add_file(&self) {
        self.files.fetch_add(1, Ordering::Relaxed);
    }

    fn observe_total_bytes(&self, bytes: u64) {
        self.bytes.fetch_max(bytes, Ordering::Relaxed);
    }

    fn set_status(&self, _message: &str) {}

    fn reset(&self) {
        self.bytes.store(0, Ordering::Relaxed);
        self.files.store(0, Ordering::Relaxed);
    }
}

/// Terminal progress bar reporting MB/s, ETA and percentage of total bytes.
pub struct ThroughputProgress {
    bar: ProgressBar,
    reported_bytes: AtomicU64,
    observed_bytes: AtomicU64,
    files: AtomicU64,
    total_bytes: u64,
    started: Instant,
}

impl ThroughputProgress {
    pub fn new(total_bytes: u64, label: &str) -> Arc<Self> {
        let bar = ProgressBar::new(total_bytes.max(1));
        bar.set_style(Self::style());
        bar.set_prefix(label.to_string());
        bar.enable_steady_tick(Duration::from_millis(200));
        Arc::new(Self {
            bar,
            reported_bytes: AtomicU64::new(0),
            observed_bytes: AtomicU64::new(0),
            files: AtomicU64::new(0),
            total_bytes,
            started: Instant::now(),
        })
    }

    /// Hidden bar: no terminal output but the same accounting. Used for `--dry-run`.
    pub fn hidden(total_bytes: u64) -> Arc<Self> {
        let progress = Self::new(total_bytes, "hidden");
        progress
            .bar
            .set_draw_target(indicatif::ProgressDrawTarget::hidden());
        progress
    }

    fn style() -> ProgressStyle {
        ProgressStyle::with_template(
            "{prefix:<9} [{elapsed_precise}] [{bar:32.cyan/blue}] {percent:>3}% \
             {bytes}/{total_bytes} @ {binary_bytes_per_sec} ETA {eta} {msg}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("##-")
    }

    /// Bytes accounted for so far (best of the two sources).
    pub fn current_bytes(&self) -> u64 {
        self.reported_bytes
            .load(Ordering::Relaxed)
            .max(self.observed_bytes.load(Ordering::Relaxed))
    }

    pub fn files(&self) -> u64 {
        self.files.load(Ordering::Relaxed)
    }

    /// Average throughput in MB/s (10^6 bytes) since the bar was created.
    pub fn average_mbps(&self) -> f64 {
        throughput_mbps(self.current_bytes(), self.started.elapsed())
    }

    fn refresh(&self) {
        let position = self.current_bytes().min(self.total_bytes.max(1));
        self.bar.set_position(position);
    }

    /// Finish the bar and leave a one-line summary on screen.
    pub fn finish(&self, message: impl Into<String>) {
        self.refresh();
        self.bar.finish_with_message(message.into());
    }
}

impl ProgressSink for ThroughputProgress {
    fn add_bytes(&self, bytes: u64) {
        self.reported_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.refresh();
    }

    fn add_file(&self) {
        self.files.fetch_add(1, Ordering::Relaxed);
    }

    fn observe_total_bytes(&self, bytes: u64) {
        self.observed_bytes.fetch_max(bytes, Ordering::Relaxed);
        self.refresh();
    }

    fn set_status(&self, message: &str) {
        self.bar.set_message(message.to_string());
    }

    fn reset(&self) {
        self.reported_bytes.store(0, Ordering::Relaxed);
        self.observed_bytes.store(0, Ordering::Relaxed);
        self.files.store(0, Ordering::Relaxed);
        self.refresh();
    }
}

/// Throughput in MB/s using MB = 10^6 bytes, the unit storage vendors quote.
pub fn throughput_mbps(bytes: u64, elapsed: Duration) -> f64 {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return 0.0;
    }
    (bytes as f64 / 1_000_000.0) / seconds
}

/// Speedup of `baseline` over `candidate`; `None` when it cannot be computed.
pub fn speedup_factor(candidate_seconds: f64, baseline_seconds: f64) -> Option<f64> {
    // Both sides need the finiteness check, not just the baseline: `NaN <= 0.0` is false, so a
    // NaN candidate slipped through the asymmetric guard and came back out as `Some(NaN)`,
    // and an infinite one produced a `Some(0.0)` speedup. Neither is a number a report should
    // carry.
    if !candidate_seconds.is_finite()
        || candidate_seconds <= 0.0
        || !baseline_seconds.is_finite()
        || baseline_seconds <= 0.0
    {
        return None;
    }
    Some(baseline_seconds / candidate_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throughput_is_computed_in_decimal_megabytes() {
        let mbps = throughput_mbps(100_000_000, Duration::from_secs(2));
        assert!((mbps - 50.0).abs() < 1e-9, "got {mbps}");
    }

    #[test]
    fn throughput_of_zero_duration_is_zero() {
        assert_eq!(throughput_mbps(1_000, Duration::ZERO), 0.0);
    }

    #[test]
    fn speedup_factor_handles_degenerate_input() {
        assert_eq!(speedup_factor(2.0, 8.0), Some(4.0));
        assert_eq!(speedup_factor(0.0, 8.0), None);
        assert_eq!(speedup_factor(2.0, 0.0), None);
        assert_eq!(speedup_factor(2.0, f64::NAN), None);
    }

    #[test]
    fn counting_sink_accumulates() {
        let sink = CountingProgress::default();
        sink.add_bytes(10);
        sink.add_bytes(5);
        sink.add_file();
        assert_eq!(sink.bytes(), 15);
        assert_eq!(sink.files(), 1);
    }

    #[test]
    fn observed_total_never_regresses() {
        let sink = CountingProgress::default();
        sink.observe_total_bytes(100);
        sink.observe_total_bytes(40);
        assert_eq!(sink.bytes(), 100);
    }

    #[test]
    fn hidden_bar_tracks_max_of_both_sources() {
        let progress = ThroughputProgress::hidden(1_000);
        progress.add_bytes(300);
        progress.observe_total_bytes(500);
        assert_eq!(progress.current_bytes(), 500);
        progress.add_bytes(400);
        assert_eq!(progress.current_bytes(), 700);
        progress.add_file();
        assert_eq!(progress.files(), 1);
        progress.finish("done");
    }

    #[test]
    fn progress_template_is_valid() {
        // Guards against a typo in the indicatif template silently degrading the bar.
        assert!(ProgressStyle::with_template(
            "{prefix:<9} [{elapsed_precise}] [{bar:32.cyan/blue}] {percent:>3}% \
             {bytes}/{total_bytes} @ {binary_bytes_per_sec} ETA {eta} {msg}",
        )
        .is_ok());
    }
    /// The guard was asymmetric: it checked the baseline for finiteness and not the candidate. A
    /// NaN candidate then passed, because `NaN <= 0.0` is false, and the function returned
    /// `Some(NaN)`; an infinite one returned `Some(0.0)`, a speedup that reads as "no gain".
    #[test]
    fn speedup_rejects_non_finite_input_on_both_sides() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                speedup_factor(bad, 10.0),
                None,
                "a non-finite candidate ({bad}) has no meaningful speedup"
            );
            assert_eq!(
                speedup_factor(10.0, bad),
                None,
                "a non-finite baseline ({bad}) has no meaningful speedup"
            );
        }
        // The ordinary case still works.
        assert_eq!(speedup_factor(2.0, 10.0), Some(5.0));
    }
}
