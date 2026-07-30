//! Copy engine abstraction.
//!
//! [`CopyEngine`] hides *how* bytes get moved so that the orchestration, retry logic, progress
//! reporting and reporting layers are platform independent and unit-testable on Linux:
//!
//! * [`robocopy::RobocopyEngine`] shells out to `robocopy.exe` (Windows only at run time, but it
//!   compiles everywhere and its flag-building/output-parsing logic is tested via an injected
//!   [`robocopy::CommandRunner`]);
//! * [`naive::NaiveCopyEngine`] is the cross-platform baseline, a plain recursive file-by-file
//!   copy equivalent to `Get-ChildItem -Recurse | Copy-Item`.

pub mod naive;
pub mod robocopy;

use std::path::PathBuf;
use std::time::Duration;

use crate::errors::IngestError;
use crate::exit_code::RobocopyStatus;
use crate::progress::ProgressSink;

/// Everything an engine needs to perform one copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyRequest {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub pattern: String,
    /// Mapped to robocopy `/MT:N`; the naive engine ignores it by design.
    pub threads: u16,
    /// Mapped to robocopy `/R:N` (per-file retries).
    pub file_retries: u32,
    /// Mapped to robocopy `/W:N` (wait between per-file retries).
    pub retry_wait_seconds: u64,
    /// Mapped to robocopy `/L`; the naive engine only walks and counts.
    pub dry_run: bool,
    /// F4.3: mirror mode — syncs destination to source, deleting extra files (`/MIR`).
    pub mirror: bool,
    /// F4.1: file patterns to exclude (`/XF pattern1 /XF pattern2 …`).
    pub exclude_files: Vec<String>,
    /// F4.1: directory patterns to exclude (`/XD dir1 /XD dir2 …`).
    pub exclude_dirs: Vec<String>,
    /// F4.2: minimum file age in days (`/MINAGE:N`). `None` means no lower bound.
    pub min_age_days: Option<u32>,
    /// F4.2: maximum file age in days (`/MAXAGE:N`). `None` means no upper bound.
    pub max_age_days: Option<u32>,
    /// F4.5: inter-packet gap in ms to throttle bandwidth (`/IPG:N`). `None` = no throttle.
    pub inter_packet_gap_ms: Option<u32>,
    /// F2.6: when `true`, skip the upfront source walk (inventory) and let robocopy proceed
    /// directly. No integrity check can be performed in this mode.
    pub prescan: bool,
    /// F6.1: prepend Windows long path prefix `\\?\` for paths > 260 chars.
    pub long_paths: bool,
    /// F6.2: preserve directory timestamps (`/DCOPY:DAT`).
    pub preserve_timestamps: bool,
    /// F6.2: preserve NTFS ACL security permissions (`/COPYALL`).
    pub preserve_acl: bool,
}

/// Result of a single engine invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyOutcome {
    pub engine: &'static str,
    pub bytes_copied: u64,
    pub files_copied: u64,
    /// Cumulative wall time of every attempt performed by [`run_with_retries`].
    pub elapsed: Duration,
    /// Process exit code; `None` for engines that are not backed by a process.
    pub exit_code: Option<i32>,
    /// Number of *extra* attempts beyond the first one.
    pub retry_attempts_used: u32,
    pub dry_run: bool,
}

impl CopyOutcome {
    pub fn new(engine: &'static str) -> Self {
        Self {
            engine,
            bytes_copied: 0,
            files_copied: 0,
            elapsed: Duration::ZERO,
            exit_code: None,
            retry_attempts_used: 0,
            dry_run: false,
        }
    }

    /// Interpretation of the exit code, when the engine produced one.
    pub fn status(&self) -> Option<RobocopyStatus> {
        self.exit_code.map(RobocopyStatus::new)
    }

    /// Engines without an exit code succeed by returning `Ok`.
    pub fn is_success(&self) -> bool {
        self.status()
            .map(RobocopyStatus::is_success)
            .unwrap_or(true)
    }

    fn should_retry(&self) -> bool {
        self.status()
            .map(RobocopyStatus::should_retry)
            .unwrap_or(false)
    }

    fn describe_status(&self) -> String {
        self.status()
            .map(RobocopyStatus::describe)
            .unwrap_or_else(|| "no exit code reported".to_string())
    }
}

/// A copy strategy.
pub trait CopyEngine: Send + Sync {
    /// Stable identifier used in logs and in the JSON report.
    fn name(&self) -> &'static str;

    /// Perform the copy, reporting incremental progress to `sink`.
    fn copy(
        &self,
        request: &CopyRequest,
        sink: &dyn ProgressSink,
    ) -> Result<CopyOutcome, IngestError>;
}

/// Outer retry configuration, applied *on top of* robocopy's own `/R` and `/W`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Extra attempts after the first one. `0` means "run once, never retry".
    pub max_retries: u32,
    pub base_wait: Duration,
    pub cap: Duration,
}

impl RetryPolicy {
    pub const DEFAULT_CAP: Duration = Duration::from_secs(300);

    pub fn new(max_retries: u32, base_wait_seconds: u64) -> Self {
        Self {
            max_retries,
            base_wait: Duration::from_secs(base_wait_seconds),
            cap: Self::DEFAULT_CAP,
        }
    }

    /// Exponential backoff: `base * 2^attempt`, clamped to [`RetryPolicy::cap`].
    pub fn backoff(&self, attempt: u32) -> Duration {
        let factor = 1u32 << attempt.min(16);
        self.base_wait.saturating_mul(factor).min(self.cap)
    }

    pub fn total_attempts(&self) -> u32 {
        self.max_retries.saturating_add(1)
    }
}

/// Indirection over sleeping so retry logic can be unit-tested without real delays.
pub trait Sleeper: Send + Sync {
    fn sleep(&self, duration: Duration);
}

/// Blocks the current thread. The engines run inside `spawn_blocking`, so this is safe.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, duration: Duration) {
        if !duration.is_zero() {
            std::thread::sleep(duration);
        }
    }
}

/// Run `engine`, re-invoking it while the failure looks transient.
///
/// A robocopy exit code with bit 3 set (>= 8) means some files exhausted their per-file retries,
/// which is exactly the case worth retrying from the outside; bit 4 (16) is a fatal configuration
/// error and is never retried. See [`crate::exit_code`].
pub fn run_with_retries(
    engine: &dyn CopyEngine,
    request: &CopyRequest,
    sink: &dyn ProgressSink,
    policy: &RetryPolicy,
    sleeper: &dyn Sleeper,
) -> Result<CopyOutcome, IngestError> {
    let mut accumulated = Duration::ZERO;

    for attempt in 0..policy.total_attempts() {
        let is_last = attempt + 1 == policy.total_attempts();
        match engine.copy(request, sink) {
            Ok(mut outcome) => {
                accumulated += outcome.elapsed;
                outcome.elapsed = accumulated;
                outcome.retry_attempts_used = attempt;

                if outcome.is_success() {
                    if attempt > 0 {
                        tracing::info!(
                            engine = engine.name(),
                            attempt = attempt + 1,
                            "copy succeeded after retrying"
                        );
                    }
                    return Ok(outcome);
                }

                let description = outcome.describe_status();
                if !outcome.should_retry() || is_last {
                    tracing::error!(
                        engine = engine.name(),
                        exit_code = outcome.exit_code,
                        attempts = attempt + 1,
                        "copy failed permanently: {description}"
                    );
                    return Err(IngestError::CopyFailed {
                        code: outcome.exit_code.unwrap_or(-1),
                        description,
                        attempts: attempt + 1,
                    });
                }

                let wait = policy.backoff(attempt);
                tracing::warn!(
                    engine = engine.name(),
                    exit_code = outcome.exit_code,
                    attempt = attempt + 1,
                    backoff_seconds = wait.as_secs(),
                    "copy incomplete ({description}), retrying"
                );
                sink.set_status("retrying after incomplete copy");
                sleeper.sleep(wait);
            }
            Err(error) => {
                if is_last || !error.is_transient() {
                    tracing::error!(
                        engine = engine.name(),
                        attempts = attempt + 1,
                        "copy aborted: {error}"
                    );
                    return Err(error);
                }
                let wait = policy.backoff(attempt);
                tracing::warn!(
                    engine = engine.name(),
                    attempt = attempt + 1,
                    backoff_seconds = wait.as_secs(),
                    "copy attempt errored ({error}), retrying"
                );
                sleeper.sleep(wait);
            }
        }
    }

    // total_attempts() >= 1, so the loop always returns.
    unreachable!("retry loop must return")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::CountingProgress;
    use crate::testkit::{RecordingSleeper, ScriptedEngine};

    fn request() -> CopyRequest {
        CopyRequest {
            source: PathBuf::from("/src"),
            dest: PathBuf::from("/dst"),
            pattern: "*.csv".to_string(),
            threads: 8,
            file_retries: 3,
            retry_wait_seconds: 5,
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
        }
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        let policy = RetryPolicy::new(3, 5);
        assert_eq!(policy.backoff(0), Duration::from_secs(5));
        assert_eq!(policy.backoff(1), Duration::from_secs(10));
        assert_eq!(policy.backoff(2), Duration::from_secs(20));
        assert_eq!(policy.backoff(20), RetryPolicy::DEFAULT_CAP);
        assert_eq!(RetryPolicy::new(3, 0).backoff(5), Duration::ZERO);
        assert_eq!(policy.total_attempts(), 4);
    }

    #[test]
    fn success_on_first_attempt_does_not_sleep() {
        let engine = ScriptedEngine::from_exit_codes([1]);
        let sleeper = RecordingSleeper::default();
        let sink = CountingProgress::default();

        let outcome = run_with_retries(
            &engine,
            &request(),
            &sink,
            &RetryPolicy::new(3, 5),
            &sleeper,
        )
        .expect("should succeed");

        assert_eq!(outcome.retry_attempts_used, 0);
        assert_eq!(engine.calls(), 1);
        assert!(sleeper.waits().is_empty());
    }

    #[test]
    fn transient_failure_is_retried_with_exponential_backoff() {
        let engine = ScriptedEngine::from_exit_codes([8, 9, 1]);
        let sleeper = RecordingSleeper::default();
        let sink = CountingProgress::default();

        let outcome = run_with_retries(
            &engine,
            &request(),
            &sink,
            &RetryPolicy::new(3, 2),
            &sleeper,
        )
        .expect("third attempt succeeds");

        assert_eq!(outcome.retry_attempts_used, 2);
        assert_eq!(outcome.exit_code, Some(1));
        assert_eq!(engine.calls(), 3);
        assert_eq!(
            sleeper.waits(),
            vec![Duration::from_secs(2), Duration::from_secs(4)]
        );
    }

    #[test]
    fn elapsed_time_accumulates_across_attempts() {
        let engine = ScriptedEngine::from_exit_codes([8, 1]).with_elapsed(Duration::from_secs(3));
        let sleeper = RecordingSleeper::default();
        let sink = CountingProgress::default();

        let outcome = run_with_retries(
            &engine,
            &request(),
            &sink,
            &RetryPolicy::new(2, 0),
            &sleeper,
        )
        .expect("succeeds");

        assert_eq!(outcome.elapsed, Duration::from_secs(6));
    }

    #[test]
    fn fatal_exit_code_is_not_retried() {
        let engine = ScriptedEngine::from_exit_codes([16, 1]);
        let sleeper = RecordingSleeper::default();
        let sink = CountingProgress::default();

        let error = run_with_retries(
            &engine,
            &request(),
            &sink,
            &RetryPolicy::new(3, 1),
            &sleeper,
        )
        .expect_err("fatal code must abort");

        assert!(matches!(
            error,
            IngestError::CopyFailed {
                code: 16,
                attempts: 1,
                ..
            }
        ));
        assert_eq!(engine.calls(), 1, "must not retry a fatal error");
        assert!(sleeper.waits().is_empty());
    }

    #[test]
    fn retry_budget_is_exhausted_then_reported() {
        let engine = ScriptedEngine::from_exit_codes([8, 8, 8, 8, 8]);
        let sleeper = RecordingSleeper::default();
        let sink = CountingProgress::default();

        let error = run_with_retries(
            &engine,
            &request(),
            &sink,
            &RetryPolicy::new(2, 0),
            &sleeper,
        )
        .expect_err("budget exhausted");

        match error {
            IngestError::CopyFailed { code, attempts, .. } => {
                assert_eq!(code, 8);
                assert_eq!(attempts, 3, "1 initial attempt + 2 retries");
            }
            other => panic!("unexpected error: {other}"),
        }
        assert_eq!(engine.calls(), 3);
        assert_eq!(sleeper.waits().len(), 2);
    }

    #[test]
    fn zero_retries_runs_exactly_once() {
        let engine = ScriptedEngine::from_exit_codes([8, 1]);
        let sleeper = RecordingSleeper::default();
        let sink = CountingProgress::default();

        let error = run_with_retries(
            &engine,
            &request(),
            &sink,
            &RetryPolicy::new(0, 5),
            &sleeper,
        )
        .expect_err("no retry budget");

        assert!(matches!(error, IngestError::CopyFailed { attempts: 1, .. }));
        assert_eq!(engine.calls(), 1);
    }

    #[test]
    fn transient_io_errors_are_retried_then_surfaced() {
        let engine = ScriptedEngine::failing_with(|| {
            IngestError::io(
                "/src/a.csv",
                std::io::Error::from(std::io::ErrorKind::TimedOut),
            )
        });
        let sleeper = RecordingSleeper::default();
        let sink = CountingProgress::default();

        let error = run_with_retries(
            &engine,
            &request(),
            &sink,
            &RetryPolicy::new(2, 0),
            &sleeper,
        )
        .expect_err("always fails");

        assert!(matches!(error, IngestError::Io { .. }));
        assert_eq!(engine.calls(), 3, "an I/O error is worth retrying");
    }

    #[test]
    fn non_transient_errors_abort_immediately() {
        let engine = ScriptedEngine::failing_with(|| IngestError::RobocopyUnavailable);
        let sleeper = RecordingSleeper::default();
        let sink = CountingProgress::default();

        let error = run_with_retries(
            &engine,
            &request(),
            &sink,
            &RetryPolicy::new(3, 60),
            &sleeper,
        )
        .expect_err("cannot succeed");

        assert!(matches!(error, IngestError::RobocopyUnavailable));
        assert_eq!(engine.calls(), 1, "no point retrying a missing binary");
        assert!(sleeper.waits().is_empty(), "must not waste backoff time");
    }

    #[test]
    fn engine_without_exit_code_is_successful() {
        let mut outcome = CopyOutcome::new("naive");
        assert!(outcome.is_success());
        assert!(!outcome.should_retry());
        outcome.exit_code = Some(8);
        assert!(!outcome.is_success());
        assert!(outcome.should_retry());
    }

    #[test]
    fn informational_exit_codes_are_accepted() {
        for code in [0, 1, 2, 3, 4, 5, 6, 7] {
            let engine = ScriptedEngine::from_exit_codes([code]);
            let sink = CountingProgress::default();
            let outcome = run_with_retries(
                &engine,
                &request(),
                &sink,
                &RetryPolicy::new(1, 0),
                &RecordingSleeper::default(),
            )
            .unwrap_or_else(|e| panic!("code {code} should succeed: {e}"));
            assert_eq!(outcome.exit_code, Some(code));
        }
    }
}
