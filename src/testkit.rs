//! Test doubles and fixtures.
//!
//! Kept in the library (rather than behind `#[cfg(test)]`) so both the unit tests and the
//! integration tests under `tests/` can drive the engine layer without a Windows machine.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tempfile::TempDir;

use crate::engine::robocopy::CommandRunner;
use crate::engine::{CopyEngine, CopyOutcome, CopyRequest};
use crate::errors::IngestError;
use crate::progress::ProgressSink;

/// [`crate::engine::Sleeper`] that records requested delays instead of waiting.
#[derive(Debug, Default)]
pub struct RecordingSleeper {
    waits: Mutex<Vec<Duration>>,
}

impl RecordingSleeper {
    pub fn waits(&self) -> Vec<Duration> {
        self.waits.lock().expect("sleeper lock").clone()
    }
}

impl crate::engine::Sleeper for RecordingSleeper {
    fn sleep(&self, duration: Duration) {
        self.waits.lock().expect("sleeper lock").push(duration);
    }
}

/// Shared log of the `(program, args)` pairs a [`ScriptedRunner`] was asked to execute.
pub type Invocations = std::sync::Arc<Mutex<Vec<(String, Vec<String>)>>>;

/// [`CommandRunner`] that replays canned stdout plus an exit code, one script entry per call.
pub struct ScriptedRunner {
    script: Mutex<std::collections::VecDeque<(Vec<String>, i32)>>,
    recorded: Invocations,
    fail: bool,
}

impl ScriptedRunner {
    /// `script` is a queue of `(stdout lines, exit code)` pairs.
    pub fn new(script: Vec<(Vec<String>, i32)>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            recorded: std::sync::Arc::new(Mutex::new(Vec::new())),
            fail: false,
        }
    }

    /// Runner that always fails to launch the program.
    pub fn always_failing() -> Self {
        Self {
            script: Mutex::new(Default::default()),
            recorded: std::sync::Arc::new(Mutex::new(Vec::new())),
            fail: true,
        }
    }

    /// Handle on the `(program, args)` pairs the runner was asked to execute.
    pub fn recorded(&self) -> Invocations {
        std::sync::Arc::clone(&self.recorded)
    }
}

impl CommandRunner for ScriptedRunner {
    fn run(
        &self,
        program: &str,
        args: &[String],
        on_line: &mut dyn FnMut(&str),
    ) -> Result<i32, IngestError> {
        self.recorded
            .lock()
            .expect("recorded lock")
            .push((program.to_string(), args.to_vec()));

        if self.fail {
            return Err(IngestError::RobocopyUnavailable);
        }

        let (lines, code) = self
            .script
            .lock()
            .expect("script lock")
            .pop_front()
            .unwrap_or_else(|| (Vec::new(), 0));

        for line in &lines {
            on_line(line);
        }
        Ok(code)
    }
}

type OutcomeResult = Result<CopyOutcome, IngestError>;

/// [`CopyEngine`] that replays a scripted sequence of outcomes, counting invocations.
pub struct ScriptedEngine {
    exit_codes: Mutex<std::collections::VecDeque<i32>>,
    calls: AtomicUsize,
    elapsed: Duration,
    bytes_per_call: u64,
    error_factory: Option<Box<dyn Fn() -> IngestError + Send + Sync>>,
}

impl ScriptedEngine {
    /// One exit code per expected invocation; the last one repeats if called again.
    pub fn from_exit_codes<I: IntoIterator<Item = i32>>(codes: I) -> Self {
        Self {
            exit_codes: Mutex::new(codes.into_iter().collect()),
            calls: AtomicUsize::new(0),
            elapsed: Duration::from_millis(1),
            bytes_per_call: 0,
            error_factory: None,
        }
    }

    /// Engine whose every invocation fails hard.
    pub fn failing_with<F>(factory: F) -> Self
    where
        F: Fn() -> IngestError + Send + Sync + 'static,
    {
        Self {
            exit_codes: Mutex::new(Default::default()),
            calls: AtomicUsize::new(0),
            elapsed: Duration::from_millis(1),
            bytes_per_call: 0,
            error_factory: Some(Box::new(factory)),
        }
    }

    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = elapsed;
        self
    }

    pub fn with_bytes_per_call(mut self, bytes: u64) -> Self {
        self.bytes_per_call = bytes;
        self
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl CopyEngine for ScriptedEngine {
    fn name(&self) -> &'static str {
        "scripted"
    }

    fn copy(&self, request: &CopyRequest, sink: &dyn ProgressSink) -> OutcomeResult {
        self.calls.fetch_add(1, Ordering::Relaxed);

        if let Some(factory) = &self.error_factory {
            return Err(factory());
        }

        let code = {
            let mut queue = self.exit_codes.lock().expect("exit code lock");
            if queue.len() > 1 {
                queue.pop_front().unwrap_or(0)
            } else {
                queue.front().copied().unwrap_or(0)
            }
        };

        if self.bytes_per_call > 0 {
            sink.add_bytes(self.bytes_per_call);
            sink.add_file();
        }

        Ok(CopyOutcome {
            engine: "scripted",
            bytes_copied: self.bytes_per_call,
            files_copied: u64::from(self.bytes_per_call > 0),
            elapsed: self.elapsed,
            exit_code: Some(code),
            retry_attempts_used: 0,
            dry_run: request.dry_run,
        })
    }
}

/// Create a temporary tree with the given `(relative path, size in bytes)` files.
///
/// File contents are deterministic pseudo-random bytes derived from the path, so two files of the
/// same size still differ — which is what makes checksum tests meaningful.
pub fn fixture_tree(files: &[(&str, usize)]) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    for (relative, size) in files {
        write_fixture_file(&dir.path().join(relative), *size);
    }
    dir
}

/// Write a single deterministic fixture file, creating parent directories as needed.
pub fn write_fixture_file(path: &std::path::Path, size: usize) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture parent");
    }
    std::fs::write(path, fixture_bytes(&path.to_string_lossy(), size)).expect("write fixture");
}

fn fixture_bytes(seed: &str, size: usize) -> Vec<u8> {
    let mut state = seed.bytes().fold(0x1234_5678u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(u32::from(byte))
    }) | 1;
    (0..size)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state & 0xFF) as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_files_have_the_requested_size_and_differ_by_path() {
        let dir = fixture_tree(&[("a.csv", 1024), ("nested/b.csv", 1024)]);
        let a = std::fs::read(dir.path().join("a.csv")).expect("read a");
        let b = std::fs::read(dir.path().join("nested/b.csv")).expect("read b");
        assert_eq!(a.len(), 1024);
        assert_eq!(b.len(), 1024);
        assert_ne!(a, b, "same-size fixtures must have different content");
    }

    #[test]
    fn scripted_engine_repeats_its_last_exit_code() {
        let dir = TempDir::new().expect("create temp dir");
        let engine = ScriptedEngine::from_exit_codes([8]);
        let request = CopyRequest {
            source: dir.path().join("src"),
            dest: dir.path().join("dst"),
            pattern: "*.csv".to_string(),
            threads: 4,
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
        };
        let sink = crate::progress::NoopProgress;
        for _ in 0..3 {
            let outcome = engine.copy(&request, &sink).expect("scripted outcome");
            assert_eq!(outcome.exit_code, Some(8));
        }
        assert_eq!(engine.calls(), 3);
    }
}
