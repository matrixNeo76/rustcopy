//! The console's list of "lavori": every copy started from the console, with how it went.
//!
//! A *session* is one copy a person started (one or more folders to one destination). It lives in
//! its own folder under the console's data directory, holds the throw-away configuration the CLI is
//! run with, and the reports that run leaves. An append-only log (`sessions.jsonl`) records three
//! kinds of line -- `started`, `finished`, `saved` -- and the list is folded from them, so a crash
//! can lose at most the last line and never corrupts earlier ones (the same NDJSON discipline as
//! `history.rs` and `generations.rs`).
//!
//! What this module decides, so that no interface has to (PIANO_GUI_SLINT.md, CATALOGO G01):
//! how a session ends (clean, dry run, needs a look, interrupted), what a session aggregates over
//! several reports, and what "save as task" writes. Saving writes the same plain configuration an
//! Explorer drop produces plus a name; it cannot carry mirror, purge or verification, and it refuses
//! to overwrite an existing file.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::IngestError;
use crate::{gui_api, runner};

const LOG_FILE: &str = "sessions.jsonl";
const SESSIONS_DIR: &str = "sessions";
const REPORT_PREFIX: &str = "robocopy_ingest_report";
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// How a session stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Started from this window and not finished yet.
    Running,
    /// Finished, every report clean (robocopy's own success reading, not `exit_code == 0`).
    Clean,
    /// Finished as a simulation: no file was really copied.
    DryRun,
    /// Finished, but something needs a look.
    NeedsLook,
    /// Started and never finished, and not running now: the window or the machine went away.
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Line {
    Started {
        id: String,
        at: DateTime<Utc>,
        sources: Vec<String>,
        dest: String,
    },
    Finished {
        id: String,
        at: DateTime<Utc>,
        exit_code: i32,
        state: SessionState,
        files_copied: u64,
        bytes_copied: u64,
        elapsed_seconds: f64,
        reports: Vec<String>,
    },
    Saved {
        id: String,
        at: DateTime<Utc>,
        saved_as: String,
    },
}

/// One session as the list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub sources: Vec<String>,
    pub dest: String,
    pub state: SessionState,
    pub exit_code: Option<i32>,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub elapsed_seconds: f64,
    /// Report files the run left, in the session's folder.
    pub reports: Vec<PathBuf>,
    /// The configuration file this session was saved as, if it was.
    pub saved_as: Option<PathBuf>,
}

/// A session just begun: where its configuration is, and its id.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub dir: PathBuf,
    pub config: PathBuf,
}

/// The sessions log and the folders beside it.
pub struct SessionLog {
    base: PathBuf,
}

/// Where the console keeps its data: `RUSTCOPY_DATA_DIR`, else `%LOCALAPPDATA%\rustcopy\console`,
/// else a folder in the temp directory.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("RUSTCOPY_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        return PathBuf::from(local).join("rustcopy").join("console");
    }
    std::env::temp_dir().join("rustcopy-console")
}

impl SessionLog {
    /// The log in the console's own data directory.
    pub fn default_location() -> Self {
        Self { base: data_dir() }
    }

    /// A log rooted at `base` (tests, and anything that wants a different place).
    pub fn at(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    fn log_path(&self) -> PathBuf {
        self.base.join(LOG_FILE)
    }

    fn append(&self, line: &Line) -> Result<(), IngestError> {
        std::fs::create_dir_all(&self.base).map_err(|e| IngestError::io(&self.base, e))?;
        let mut text = serde_json::to_string(line).map_err(|e| {
            IngestError::io(
                self.log_path(),
                std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            )
        })?;
        text.push('\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path())
            .map_err(|e| IngestError::io(self.log_path(), e))?;
        // One write call per line: a line is either entirely there or (after a crash) a torn tail
        // that `list` skips.
        file.write_all(text.as_bytes())
            .map_err(|e| IngestError::io(self.log_path(), e))
    }

    /// Begins a session for `items` (the pairs `runner::plan_copy` returned): creates its folder,
    /// writes the configuration there and logs `started`. `sources`/`dest` are what the person chose,
    /// kept to repeat the copy later.
    pub fn begin(
        &self,
        sources: &[PathBuf],
        dest: &Path,
        items: &[(PathBuf, PathBuf)],
    ) -> Result<Session, IngestError> {
        let id = format!(
            "{}-{:03}",
            chrono::Local::now().format("%Y%m%d-%H%M%S"),
            SEQUENCE.fetch_add(1, Ordering::Relaxed) % 1000
        );
        let dir = self.base.join(SESSIONS_DIR).join(&id);
        std::fs::create_dir_all(&dir).map_err(|e| IngestError::io(&dir, e))?;
        let config = dir.join("config.toml");
        runner::write_shell_drop_config(items, &config)?;
        self.append(&Line::Started {
            id: id.clone(),
            at: Utc::now(),
            sources: sources
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            dest: dest.to_string_lossy().into_owned(),
        })?;
        Ok(Session { id, dir, config })
    }

    /// Logs how a session ended. The reports it left are found in its folder and read by the core, so
    /// the clean / needs-a-look reading is `exit_code_is_success`, never derived from the exit code.
    pub fn finish(&self, id: &str, exit_code: i32) -> Result<SessionState, IngestError> {
        let dir = self.base.join(SESSIONS_DIR).join(id);
        let reports = find_reports(&dir);
        let mut files = 0u64;
        let mut bytes = 0u64;
        let mut seconds = 0.0f64;
        let mut all_clean = !reports.is_empty();
        let mut any_dry = false;
        for report in &reports {
            match gui_api::read_report(report) {
                Ok(view) => {
                    files += view.files_copied;
                    bytes += view.bytes_copied;
                    seconds += view.elapsed_seconds;
                    all_clean &= view.exit_code_is_success.unwrap_or(false);
                    any_dry |= view.dry_run;
                }
                Err(_) => all_clean = false,
            }
        }
        let state = if any_dry {
            SessionState::DryRun
        } else if all_clean {
            SessionState::Clean
        } else {
            SessionState::NeedsLook
        };
        self.append(&Line::Finished {
            id: id.to_string(),
            at: Utc::now(),
            exit_code,
            state,
            files_copied: files,
            bytes_copied: bytes,
            elapsed_seconds: seconds,
            reports: reports
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        })?;
        Ok(state)
    }

    /// The most recent `limit` sessions, newest first. `running` is the id of the session this
    /// window is running now: a `started` line with no `finished` is *running* if it is that one and
    /// *interrupted* otherwise. A torn or unreadable line is skipped, never fatal.
    pub fn list(&self, limit: usize, running: Option<&str>) -> Vec<SessionSummary> {
        let Ok(text) = std::fs::read_to_string(self.log_path()) else {
            return Vec::new();
        };
        let mut order: Vec<String> = Vec::new();
        let mut by_id: HashMap<String, SessionSummary> = HashMap::new();
        for raw in text.lines() {
            let Ok(line) = serde_json::from_str::<Line>(raw) else {
                continue;
            };
            match line {
                Line::Started {
                    id,
                    at,
                    sources,
                    dest,
                } => {
                    order.push(id.clone());
                    by_id.insert(
                        id.clone(),
                        SessionSummary {
                            id,
                            started_at: at,
                            sources,
                            dest,
                            state: SessionState::Interrupted,
                            exit_code: None,
                            files_copied: 0,
                            bytes_copied: 0,
                            elapsed_seconds: 0.0,
                            reports: Vec::new(),
                            saved_as: None,
                        },
                    );
                }
                Line::Finished {
                    id,
                    exit_code,
                    state,
                    files_copied,
                    bytes_copied,
                    elapsed_seconds,
                    reports,
                    ..
                } => {
                    if let Some(entry) = by_id.get_mut(&id) {
                        entry.state = state;
                        entry.exit_code = Some(exit_code);
                        entry.files_copied = files_copied;
                        entry.bytes_copied = bytes_copied;
                        entry.elapsed_seconds = elapsed_seconds;
                        entry.reports = reports.into_iter().map(PathBuf::from).collect();
                    }
                }
                Line::Saved { id, saved_as, .. } => {
                    if let Some(entry) = by_id.get_mut(&id) {
                        entry.saved_as = Some(PathBuf::from(saved_as));
                    }
                }
            }
        }
        let mut sessions: Vec<SessionSummary> = order
            .into_iter()
            .rev()
            .filter_map(|id| by_id.remove(&id))
            .take(limit)
            .collect();
        for session in &mut sessions {
            if session.exit_code.is_none() && Some(session.id.as_str()) == running {
                session.state = SessionState::Running;
            }
        }
        sessions
    }

    /// Saves a session as a task: writes the plain configuration for its folders to
    /// `dir/<name>.toml`, **refusing to overwrite** an existing file (`create_new`), and logs where.
    /// The name is checked as a job name is (it becomes part of file names). Returns the file written.
    pub fn save_as_task(&self, id: &str, dir: &Path, name: &str) -> Result<PathBuf, IngestError> {
        crate::validate_job_name(name)?;
        let session = self
            .list(usize::MAX, None)
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| IngestError::CopyPlanInvalid(format!("Lavoro {id} non trovato.")))?;
        let sources: Vec<PathBuf> = session.sources.iter().map(PathBuf::from).collect();
        let items = runner::plan_copy(&sources, Path::new(&session.dest))?;
        let text = runner::shell_drop_config_text(&items, Some(name))?;

        std::fs::create_dir_all(dir).map_err(|e| IngestError::io(dir, e))?;
        let target = dir.join(format!("{name}.toml"));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| IngestError::io(&target, e))?;
        file.write_all(text.as_bytes())
            .map_err(|e| IngestError::io(&target, e))?;
        self.append(&Line::Saved {
            id: id.to_string(),
            at: Utc::now(),
            saved_as: target.to_string_lossy().into_owned(),
        })?;
        Ok(target)
    }
}

/// The report files in a session's folder, sorted; checkpoints are not reports.
fn find_reports(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with(REPORT_PREFIX) && n.ends_with(".json") && !n.contains("checkpoint")
            })
        })
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(source: &str, dest: &str) -> (PathBuf, PathBuf) {
        (PathBuf::from(source), PathBuf::from(dest))
    }

    #[test]
    fn a_started_session_with_no_end_is_running_only_when_this_window_runs_it() {
        let base = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let session = log
            .begin(
                &[PathBuf::from(r"C:\a")],
                Path::new(r"D:\out"),
                &[pair(r"C:\a", r"D:\out\a")],
            )
            .expect("begin");
        assert!(
            session.config.exists(),
            "the configuration is written into the session folder"
        );

        let running = log.list(10, Some(&session.id));
        assert_eq!(running[0].state, SessionState::Running);
        let after_crash = log.list(10, None);
        assert_eq!(
            after_crash[0].state,
            SessionState::Interrupted,
            "nobody runs it: it was interrupted"
        );
    }

    #[test]
    fn the_list_is_newest_first_and_honours_the_limit() {
        let base = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        for name in ["one", "two", "three"] {
            log.begin(
                &[PathBuf::from(format!(r"C:\{name}"))],
                Path::new(r"D:\out"),
                &[pair(&format!(r"C:\{name}"), &format!(r"D:\out\{name}"))],
            )
            .expect("begin");
        }
        let two = log.list(2, None);
        assert_eq!(two.len(), 2);
        assert!(two[0].sources[0].ends_with("three"));
        assert!(two[1].sources[0].ends_with("two"));
    }

    #[test]
    fn a_torn_or_unknown_line_is_skipped_not_fatal() {
        let base = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        log.begin(
            &[PathBuf::from(r"C:\a")],
            Path::new(r"D:\out"),
            &[pair(r"C:\a", r"D:\out\a")],
        )
        .expect("begin");
        let mut text = std::fs::read_to_string(log.log_path()).expect("read");
        text.push_str("{\"event\":\"started\",\"id\":\"torn"); // an append cut off by a crash
        std::fs::write(log.log_path(), text).expect("write");
        assert_eq!(log.list(10, None).len(), 1);
    }

    #[test]
    fn finishing_without_any_report_is_never_called_clean() {
        let base = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let session = log
            .begin(
                &[PathBuf::from(r"C:\a")],
                Path::new(r"D:\out"),
                &[pair(r"C:\a", r"D:\out\a")],
            )
            .expect("begin");
        let state = log.finish(&session.id, 16).expect("finish");
        assert_eq!(
            state,
            SessionState::NeedsLook,
            "no report means nothing proves it went well"
        );
        let listed = log.list(10, None);
        assert_eq!(listed[0].exit_code, Some(16));
    }

    #[test]
    fn checkpoints_are_not_reports() {
        let base = tempfile::tempdir().expect("tempdir");
        for name in [
            "robocopy_ingest_report.json",
            "robocopy_ingest_report.docs.json",
            "robocopy_ingest_report.json.checkpoint.json",
            "config.toml",
        ] {
            std::fs::write(base.path().join(name), "{}").expect("write");
        }
        let names: Vec<String> = find_reports(base.path())
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            names,
            vec![
                "robocopy_ingest_report.docs.json",
                "robocopy_ingest_report.json"
            ]
        );
    }

    #[test]
    fn saving_as_a_task_writes_a_plain_config_and_never_overwrites() {
        let base = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let session = log
            .begin(
                &[PathBuf::from(r"C:\Dati\Foto")],
                Path::new(r"D:\out"),
                &[pair(r"C:\Dati\Foto", r"D:\out\Foto")],
            )
            .expect("begin");

        let saved = log
            .save_as_task(&session.id, tasks.path(), "foto-notte")
            .expect("first save");
        let text = std::fs::read_to_string(&saved).expect("read");
        assert!(
            text.contains("foto-notte"),
            "the chosen name is in the file"
        );
        for forbidden in ["mirror", "force_purge", "force-purge", "verify", "encrypt"] {
            assert!(
                !text.contains(forbidden),
                "a saved task must not carry {forbidden}"
            );
        }
        assert!(
            log.save_as_task(&session.id, tasks.path(), "foto-notte")
                .is_err(),
            "second save must refuse"
        );
        assert_eq!(
            log.list(10, None)[0].saved_as.as_deref(),
            Some(saved.as_path())
        );
    }

    #[test]
    fn a_task_name_is_validated_like_a_job_name() {
        let base = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let session = log
            .begin(
                &[PathBuf::from(r"C:\a")],
                Path::new(r"D:\out"),
                &[pair(r"C:\a", r"D:\out\a")],
            )
            .expect("begin");
        assert!(log
            .save_as_task(&session.id, tasks.path(), "bad:name")
            .is_err());
        assert!(log.save_as_task(&session.id, tasks.path(), "CON").is_err());
        assert!(
            std::fs::read_dir(tasks.path())
                .expect("dir")
                .next()
                .is_none(),
            "nothing written"
        );
    }

    #[test]
    fn saving_an_unknown_session_is_refused() {
        let base = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        assert!(log.save_as_task("nope", tasks.path(), "x").is_err());
    }
}
