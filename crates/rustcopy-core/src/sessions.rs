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
use crate::integrity::HashAlgorithm;
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
        /// The verification chosen for this copy, if any (an older line has none).
        #[serde(default)]
        verify: Option<HashAlgorithm>,
        /// The saved task this run executes in place (its configuration file), if any.
        #[serde(default)]
        task: Option<String>,
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
    /// The verification this copy was started with, if any.
    pub verify: Option<HashAlgorithm>,
    pub state: SessionState,
    pub exit_code: Option<i32>,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub elapsed_seconds: f64,
    /// Report files the run left, in the session's folder.
    pub reports: Vec<PathBuf>,
    /// The configuration file this session was saved as, if it was.
    pub saved_as: Option<PathBuf>,
    /// The saved task this run executed in place, if it was one (not a copy chosen in the window).
    pub task: Option<PathBuf>,
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
        self.begin_with(sources, dest, items, None)
    }

    /// [`SessionLog::begin`] with a verification: the copied files are read back and compared with
    /// `verify`. The only option a console copy can add, and typed so that nothing else can ride along.
    pub fn begin_with(
        &self,
        sources: &[PathBuf],
        dest: &Path,
        items: &[(PathBuf, PathBuf)],
        verify: Option<HashAlgorithm>,
    ) -> Result<Session, IngestError> {
        let id = self.new_id();
        let dir = self.base.join(SESSIONS_DIR).join(&id);
        std::fs::create_dir_all(&dir).map_err(|e| IngestError::io(&dir, e))?;
        let config = dir.join("config.toml");
        let text = runner::shell_drop_config_text(items, None, verify)?;
        crate::atomic_write(&config, text.as_bytes()).map_err(|e| IngestError::io(&config, e))?;
        self.append(&Line::Started {
            id: id.clone(),
            at: Utc::now(),
            sources: sources
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            dest: dest.to_string_lossy().into_owned(),
            verify,
            task: None,
        })?;
        Ok(Session { id, dir, config })
    }

    /// Begins a session for a configuration written elsewhere (the Explorer extension's drop file):
    /// the file is copied into the session's folder and its folders are read back, so the list shows
    /// the same thing whichever way the copy was started. A file with no source/destination pair is
    /// refused.
    pub fn begin_from_config(&self, config: &Path) -> Result<Session, IngestError> {
        let (pairs, verify) = read_pairs(config)?;
        let sources: Vec<PathBuf> = pairs.iter().map(|(source, _)| source.clone()).collect();
        // A drop puts each folder *under* the destination the person chose: that parent is the
        // destination to show and to repeat with.
        let dest = lexical_parent(&pairs[0].1);

        let id = self.new_id();
        let dir = self.base.join(SESSIONS_DIR).join(&id);
        std::fs::create_dir_all(&dir).map_err(|e| IngestError::io(&dir, e))?;
        let copied = dir.join("config.toml");
        std::fs::copy(config, &copied).map_err(|e| IngestError::io(&copied, e))?;
        self.append(&Line::Started {
            id: id.clone(),
            at: Utc::now(),
            sources: sources
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            dest: dest.to_string_lossy().into_owned(),
            verify,
            task: None,
        })?;
        Ok(Session {
            id,
            dir,
            config: copied,
        })
    }

    /// Begins a session that runs a **saved task in place**: the configuration is not copied, because
    /// the CLI runs in the configuration's own folder (relative paths mean "relative to the file")
    /// and the task's report and run history live there. The session's folder only holds its id.
    pub fn begin_task(&self, config: &Path) -> Result<Session, IngestError> {
        let config = std::path::absolute(config).map_err(|e| IngestError::io(config, e))?;
        let (pairs, verify) = read_pairs(&config)?;
        let sources: Vec<String> = pairs
            .iter()
            .map(|(source, _)| source.to_string_lossy().into_owned())
            .collect();
        let dest = pairs[0].1.to_string_lossy().into_owned();
        let id = self.new_id();
        let dir = self.base.join(SESSIONS_DIR).join(&id);
        std::fs::create_dir_all(&dir).map_err(|e| IngestError::io(&dir, e))?;
        self.append(&Line::Started {
            id: id.clone(),
            at: Utc::now(),
            sources,
            dest,
            verify,
            task: Some(config.to_string_lossy().into_owned()),
        })?;
        Ok(Session { id, dir, config })
    }

    fn new_id(&self) -> String {
        format!(
            "{}-{:03}",
            chrono::Local::now().format("%Y%m%d-%H%M%S"),
            SEQUENCE.fetch_add(1, Ordering::Relaxed) % 1000
        )
    }

    /// Logs how a session ended. The reports it left are found in its folder and read by the core, so
    /// the clean / needs-a-look reading is `exit_code_is_success`, never derived from the exit code.
    pub fn finish(&self, id: &str, exit_code: i32) -> Result<SessionState, IngestError> {
        let dir = self.base.join(SESSIONS_DIR).join(id);
        let started = self.list(usize::MAX, None).into_iter().find(|s| s.id == id);
        let reports = match started
            .as_ref()
            .and_then(|s| s.task.as_deref().map(|t| (t, s.started_at)))
        {
            // A task runs in its own folder: its reports are where its configuration says, and only
            // a file written since this run began counts (a report left by an earlier run must never
            // make a run that wrote nothing look clean).
            Some((task, since)) => task_reports(task, since),
            None => find_reports(&dir),
        };
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
                    verify,
                    task,
                } => {
                    order.push(id.clone());
                    by_id.insert(
                        id.clone(),
                        SessionSummary {
                            id,
                            started_at: at,
                            sources,
                            dest,
                            verify,
                            state: SessionState::Interrupted,
                            exit_code: None,
                            files_copied: 0,
                            bytes_copied: 0,
                            elapsed_seconds: 0.0,
                            reports: Vec::new(),
                            saved_as: None,
                            task: task.map(PathBuf::from),
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
        let text = runner::shell_drop_config_text(&items, Some(name), session.verify)?;

        // Each task gets a folder of its own: the configuration runs with that folder as its working
        // directory, so its report and its run history land there and cannot overwrite another
        // task's (one shared folder would give every task the same default report file).
        let task_dir = dir.join(name);
        std::fs::create_dir_all(&task_dir).map_err(|e| IngestError::io(&task_dir, e))?;
        let target = task_dir.join(format!("{name}.toml"));
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

/// The folder pairs of a configuration and the verification it asks for.
type ConfigPairs = (Vec<(PathBuf, PathBuf)>, Option<HashAlgorithm>);

/// The source/destination pairs of a configuration, and the verification it asks for.
fn read_pairs(config: &Path) -> Result<ConfigPairs, IngestError> {
    let parsed = crate::config::IngestConfig::load_from(config)?;
    let jobs: Vec<&crate::config::JobConfig> = match parsed.jobs.as_ref() {
        Some(jobs) if !jobs.is_empty() => jobs.iter().collect(),
        _ => vec![&parsed.defaults],
    };
    let pairs: Vec<(PathBuf, PathBuf)> = jobs
        .iter()
        .filter_map(|job| Some((job.source.clone()?, job.dest.clone()?)))
        .collect();
    if pairs.is_empty() {
        return Err(IngestError::CopyPlanInvalid(
            "La configurazione non contiene nessuna coppia cartella di origine / destinazione."
                .to_string(),
        ));
    }
    let verify = jobs.first().and_then(|job| {
        (job.verify_integrity == Some(true)).then(|| job.hash_algo.unwrap_or_default())
    });
    Ok((pairs, verify))
}

/// The reports a saved task wrote **since `since`**: the report paths its configuration resolves to
/// (`gui_api::list_jobs`) that exist and were modified after the run began (with two seconds of slack
/// for clock granularity). A report from an earlier run is not evidence about this one.
fn task_reports(config: &Path, since: DateTime<Utc>) -> Vec<PathBuf> {
    let Ok(jobs) = gui_api::list_jobs(config) else {
        return Vec::new();
    };
    let floor = std::time::SystemTime::from(since) - std::time::Duration::from_secs(2);
    let mut found: Vec<PathBuf> = jobs
        .into_iter()
        .filter_map(|job| job.report_path)
        .map(PathBuf::from)
        .filter(|path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified >= floor)
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// A saved task as the list of attività shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskEntry {
    pub name: String,
    pub path: PathBuf,
    pub sources: Vec<String>,
    pub dest: String,
    pub verify: Option<HashAlgorithm>,
}

/// The saved tasks under `dir`: every `*.toml` in each sub-folder (`dir/<name>/<name>.toml`), sorted
/// by name. A file that cannot be read as a configuration with at least one folder pair is skipped,
/// not fatal: the folder may hold something the person put there.
pub fn list_tasks(dir: &Path) -> Vec<TaskEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut tasks: Vec<TaskEntry> = Vec::new();
    for folder in entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
    {
        let Ok(files) = std::fs::read_dir(&folder) else {
            continue;
        };
        for file in files.filter_map(Result::ok).map(|e| e.path()) {
            let is_toml = file
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"));
            if !is_toml {
                continue;
            }
            let Ok((pairs, verify)) = read_pairs(&file) else {
                continue;
            };
            let name = file
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            tasks.push(TaskEntry {
                name,
                path: file,
                sources: pairs
                    .iter()
                    .map(|(s, _)| s.to_string_lossy().into_owned())
                    .collect(),
                dest: pairs[0].1.to_string_lossy().into_owned(),
                verify,
            });
        }
    }
    tasks.sort_by_key(|t| t.name.to_lowercase());
    tasks
}

/// The folder that contains `path`, by string logic with both separators: `Path::parent` follows the
/// *host's* separator rules, and these are Windows paths that must read the same on the Linux CI
/// runner (the same reason `runner::plan_copy` is lexical). A path with no parent is returned as is.
fn lexical_parent(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let trimmed = text.trim_end_matches(['\\', '/']);
    match trimmed.rfind(['\\', '/']) {
        Some(at) if at > 0 => PathBuf::from(&trimmed[..at]),
        _ => path.to_path_buf(),
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
    fn a_drop_configuration_becomes_a_session_with_its_folders() {
        let base = tempfile::tempdir().expect("tempdir");
        let drops = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let drop = drops.path().join("shell-drop-1.toml");
        runner::write_shell_drop_config(&[pair(r"C:\Dati\Foto", r"D:\Archivio\Foto")], &drop)
            .expect("write");

        let session = log.begin_from_config(&drop).expect("begin");
        assert!(session.config.exists());
        let listed = log.list(10, Some(&session.id));
        assert_eq!(listed[0].sources, vec![r"C:\Dati\Foto".to_string()]);
        assert_eq!(
            listed[0].dest, r"D:\Archivio",
            "the destination the person chose, not the folder under it"
        );
    }

    #[test]
    fn the_lexical_parent_reads_windows_paths_on_any_host() {
        assert_eq!(
            lexical_parent(Path::new(r"D:\Archivio\Foto")),
            PathBuf::from(r"D:\Archivio")
        );
        assert_eq!(
            lexical_parent(Path::new(r"D:\Archivio\Foto\")),
            PathBuf::from(r"D:\Archivio")
        );
        assert_eq!(
            lexical_parent(Path::new("D:/Archivio/Foto")),
            PathBuf::from("D:/Archivio")
        );
        assert_eq!(
            lexical_parent(Path::new(r"\\nas\share\x")),
            PathBuf::from(r"\\nas\share")
        );
        assert_eq!(lexical_parent(Path::new("Foto")), PathBuf::from("Foto"));
    }

    #[test]
    fn a_configuration_without_folders_is_refused() {
        let base = tempfile::tempdir().expect("tempdir");
        let empty = base.path().join("empty.toml");
        std::fs::write(&empty, "threads = 4\n").expect("write");
        let log = SessionLog::at(base.path());
        assert!(log.begin_from_config(&empty).is_err());
    }

    #[test]
    fn a_verification_is_written_remembered_and_repeated_into_a_saved_task() {
        let base = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let session = log
            .begin_with(
                &[PathBuf::from(r"C:\Dati\Foto")],
                Path::new(r"D:\out"),
                &[pair(r"C:\Dati\Foto", r"D:\out\Foto")],
                Some(HashAlgorithm::Xxh3),
            )
            .expect("begin");

        let config = std::fs::read_to_string(&session.config).expect("config");
        assert!(config.contains("verify_integrity = true"), "{config}");
        assert!(config.contains("xxh3"), "{config}");
        assert_eq!(log.list(10, None)[0].verify, Some(HashAlgorithm::Xxh3));

        let saved = log
            .save_as_task(&session.id, tasks.path(), "foto")
            .expect("save");
        let text = std::fs::read_to_string(saved).expect("read");
        assert!(
            text.contains("xxh3"),
            "a saved task keeps the verification it was run with: {text}"
        );
        for forbidden in ["mirror", "force_purge", "force-purge", "encrypt"] {
            assert!(
                !text.contains(forbidden),
                "verification must not bring {forbidden} with it"
            );
        }
    }

    #[test]
    fn no_verification_leaves_the_configuration_exactly_as_a_drop_writes_it() {
        let base = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let items = [pair(r"C:\a", r"D:\out\a")];
        let session = log
            .begin(&[PathBuf::from(r"C:\a")], Path::new(r"D:\out"), &items)
            .expect("begin");
        let written = std::fs::read_to_string(&session.config).expect("config");
        assert_eq!(
            written,
            runner::shell_drop_config_text(&items, None, None).expect("text")
        );
        assert!(!written.contains("verify"));
        assert_eq!(log.list(10, None)[0].verify, None);
    }

    #[test]
    fn a_drop_configuration_with_verification_is_recognised() {
        let base = tempfile::tempdir().expect("tempdir");
        let drops = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let drop = drops.path().join("shell-drop-2.toml");
        let text = runner::shell_drop_config_text(
            &[pair(r"C:\a", r"D:\out\a")],
            None,
            Some(HashAlgorithm::Blake3),
        )
        .expect("text");
        std::fs::write(&drop, text).expect("write");
        let session = log.begin_from_config(&drop).expect("begin");
        assert_eq!(
            log.list(10, Some(&session.id))[0].verify,
            Some(HashAlgorithm::Blake3)
        );
    }

    const CLEAN_REPORT: &str = r#"{"schema_version":2,"timestamp":"2026-08-31T00:00:00Z","tool_version":"6.0.0","host_platform":"windows","host_metadata":{"hostname":"HOST","os_name":"windows","logical_cpus":8},"source":"D:/src","dest":"E:/dst","total_files":1,"total_bytes":2,"robocopy_transfer":{"engine":"robocopy","elapsed_seconds":0.058,"throughput_mbps":0.001,"bytes_copied":64,"files_copied":3,"exit_code":1,"exit_code_meaning":"files copied","retry_attempts_used":0,"dry_run":false},"phase_timing":{"inventory_seconds":0.0049447,"transfer_seconds":0.0607128,"verification_seconds":0.0072939,"total_seconds":0.0737639},"configuration":{"threads":48,"retries":3,"retry_wait_seconds":5,"pattern":"*","verify_integrity":false,"compare_baseline":false,"dry_run":false},"log_lines_dropped":0,"encrypted":false,"decrypted":false}"#;

    fn saved_task(log: &SessionLog, tasks: &Path, name: &str) -> PathBuf {
        let session = log
            .begin(
                &[PathBuf::from(r"C:\Dati\Foto")],
                Path::new(r"D:\out"),
                &[pair(r"C:\Dati\Foto", r"D:\out\Foto")],
            )
            .expect("begin");
        log.save_as_task(&session.id, tasks, name).expect("save")
    }

    #[test]
    fn each_saved_task_lives_in_a_folder_of_its_own() {
        let base = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let a = saved_task(&log, tasks.path(), "foto");
        let b = saved_task(&log, tasks.path(), "video");
        assert_ne!(
            a.parent(),
            b.parent(),
            "two tasks must not share a working folder"
        );
        assert_eq!(
            a.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str()),
            Some("foto")
        );
        let listed = list_tasks(tasks.path());
        assert_eq!(
            listed.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["foto", "video"]
        );
        assert_eq!(listed[0].sources, vec![r"C:\Dati\Foto".to_string()]);
    }

    #[test]
    fn a_stray_file_in_the_tasks_folder_is_skipped() {
        let tasks = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tasks.path().join("x")).expect("dir");
        std::fs::write(tasks.path().join("x").join("note.toml"), "threads = 2\n").expect("write");
        std::fs::write(tasks.path().join("loose.toml"), "threads = 2\n").expect("write");
        assert!(list_tasks(tasks.path()).is_empty());
        assert!(list_tasks(&tasks.path().join("missing")).is_empty());
    }

    #[test]
    fn a_task_runs_in_place_and_only_a_fresh_report_counts() {
        let base = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let config = saved_task(&log, tasks.path(), "foto");

        // An earlier run left a clean report. A run that then writes nothing must not inherit it.
        let report = config
            .parent()
            .expect("folder")
            .join("robocopy_ingest_report.json");
        std::fs::write(&report, CLEAN_REPORT).expect("report");
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&report)
            .expect("open")
            .set_modified(old)
            .expect("mtime");

        let stale = log.begin_task(&config).expect("begin");
        assert_eq!(
            stale.config,
            std::path::absolute(&config).expect("abs"),
            "the file is not copied"
        );
        assert_eq!(
            log.finish(&stale.id, 1).expect("finish"),
            SessionState::NeedsLook
        );

        // The next run writes a fresh report: now it counts.
        std::fs::write(&report, CLEAN_REPORT).expect("fresh report");
        let fresh = log.begin_task(&config).expect("begin");
        std::fs::write(&report, CLEAN_REPORT).expect("rewrite after start");
        assert_eq!(
            log.finish(&fresh.id, 1).expect("finish"),
            SessionState::Clean
        );

        let listed = log.list(10, None);
        assert_eq!(
            listed[0].task.as_deref(),
            Some(std::path::absolute(&config).expect("abs").as_path())
        );
        assert_eq!(listed[0].files_copied, 3);
    }

    #[test]
    fn saving_an_unknown_session_is_refused() {
        let base = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        assert!(log.save_as_task("nope", tasks.path(), "x").is_err());
    }
}
