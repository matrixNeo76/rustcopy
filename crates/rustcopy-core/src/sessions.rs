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
        /// Set when this run resumes an interrupted one: the folder holding the checkpoint, where the
        /// reports of the resumed run are written.
        #[serde(default)]
        resume_dir: Option<String>,
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
    /// Where this lavoro's own files live: the task's folder for a task run, else its session folder.
    /// A checkpoint left by an interrupted run is looked for here.
    pub folder: PathBuf,
    /// The folder a resumed run took its checkpoint from, when this lavoro is a resume.
    pub resume_dir: Option<PathBuf>,
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
            resume_dir: None,
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
            resume_dir: None,
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
            resume_dir: None,
        })?;
        Ok(Session { id, dir, config })
    }

    /// The interrupted runs of a lavoro that can be resumed: checkpoints in its own folder, newest
    /// first (`gui_api::list_checkpoints`). A checkpoint a later resume already carried through to a
    /// clean end is left out: the file stays on disk (the CLI never removes it), but offering to resume
    /// finished work would only invite a pointless second run.
    pub fn checkpoints_of(&self, id: &str) -> Vec<gui_api::CheckpointSummary> {
        let all = self.list(usize::MAX, None);
        let Some(session) = all.iter().find(|s| s.id == id) else {
            return Vec::new();
        };
        let mut found = gui_api::list_checkpoints(&session.folder).unwrap_or_default();
        found.retain(|checkpoint| {
            !all.iter().any(|later| {
                later.state == SessionState::Clean
                    && later.started_at >= checkpoint.timestamp
                    && later.resume_dir.as_deref() == Some(session.folder.as_path())
            })
        });
        found
    }

    /// Begins a lavoro that **resumes** an interrupted one from `checkpoint`, which must be one of
    /// that lavoro's own (`checkpoints_of`): anything else is refused, so a path typed or pasted
    /// elsewhere cannot start a run. The new lavoro keeps the old one's folders and task, and its
    /// `config` is the checkpoint (what `runner::resume_arguments` takes).
    pub fn begin_resume(&self, id: &str, checkpoint: &Path) -> Result<Session, IngestError> {
        let old = self
            .list(usize::MAX, None)
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| IngestError::CopyPlanInvalid(format!("Lavoro {id} non trovato.")))?;
        let wanted = std::path::absolute(checkpoint)
            .map_err(|e| IngestError::io(checkpoint, e))?
            .to_string_lossy()
            .to_lowercase();
        let own = self
            .checkpoints_of(id)
            .into_iter()
            .find(|c| {
                std::path::absolute(&c.path)
                    .map(|p| p.to_string_lossy().to_lowercase() == wanted)
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                IngestError::CopyPlanInvalid(
                    "Questo punto di ripresa non appartiene a questa copia.".to_string(),
                )
            })?;
        let checkpoint = PathBuf::from(own.path);
        let folder = checkpoint
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| old.folder.clone());

        let new_id = self.new_id();
        let dir = self.base.join(SESSIONS_DIR).join(&new_id);
        std::fs::create_dir_all(&dir).map_err(|e| IngestError::io(&dir, e))?;
        self.append(&Line::Started {
            id: new_id.clone(),
            at: Utc::now(),
            sources: old.sources.clone(),
            dest: old.dest.clone(),
            verify: old.verify,
            task: old.task.as_ref().map(|p| p.to_string_lossy().into_owned()),
            resume_dir: Some(folder.to_string_lossy().into_owned()),
        })?;
        Ok(Session {
            id: new_id,
            dir,
            config: checkpoint,
        })
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
            None => match started
                .as_ref()
                .and_then(|s| s.resume_dir.as_deref().map(|d| (d, s.started_at)))
            {
                // A resumed run writes into the folder its checkpoint came from, which also holds
                // the reports of the run it continues: only the ones written since it began count.
                Some((folder, since)) => fresh(find_reports(folder), since),
                None => find_reports(&dir),
            },
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
                    resume_dir,
                } => {
                    order.push(id.clone());
                    let session_dir = self.base.join(SESSIONS_DIR).join(&id);
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
                            folder: task
                                .as_deref()
                                .map(Path::new)
                                .and_then(Path::parent)
                                .map_or(session_dir, Path::to_path_buf),
                            task: task.map(PathBuf::from),
                            resume_dir: resume_dir.map(PathBuf::from),
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
    let paths: Vec<PathBuf> = jobs
        .into_iter()
        .filter_map(|job| job.report_path)
        .map(PathBuf::from)
        .collect();
    let mut found = fresh(paths, since);
    found.sort();
    found.dedup();
    found
}

/// The paths among `paths` that exist and were modified after `since` (two seconds of slack for clock
/// granularity).
fn fresh(paths: Vec<PathBuf>, since: DateTime<Utc>) -> Vec<PathBuf> {
    let floor = std::time::SystemTime::from(since) - std::time::Duration::from_secs(2);
    paths
        .into_iter()
        .filter(|path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified >= floor)
        })
        .collect()
}

/// Folders used by recent lavori, newest first, without repeats.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentFolders {
    pub sources: Vec<String>,
    pub dests: Vec<String>,
}

/// The folders the recent `sessions` (newest first, as [`SessionLog::list`] returns them) copied from
/// and to, at most `limit` of each. Nothing is stored for this: the log already is the memory, so
/// there is no second list that could disagree with it. Repeats are judged case-insensitively, as
/// Windows paths are, and blank entries are dropped.
pub fn recent_folders(sessions: &[SessionSummary], limit: usize) -> RecentFolders {
    fn push(list: &mut Vec<String>, value: &str, limit: usize) {
        let value = value.trim();
        if value.is_empty() || list.len() >= limit {
            return;
        }
        if !list.iter().any(|known| known.eq_ignore_ascii_case(value)) {
            list.push(value.to_string());
        }
    }
    let mut recent = RecentFolders::default();
    for session in sessions {
        for source in &session.sources {
            push(&mut recent.sources, source, limit);
        }
        push(&mut recent.dests, &session.dest, limit);
    }
    recent
}

/// A saved task as the list of attività shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskEntry {
    pub name: String,
    pub path: PathBuf,
    pub sources: Vec<String>,
    pub dest: String,
    pub verify: Option<HashAlgorithm>,
    /// How many jobs the file describes (`[[jobs]]`, or 1 for a single-job file).
    pub jobs: usize,
    /// `true` for a task this console saved, `false` for a configuration file the person added.
    pub saved: bool,
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
                jobs: pairs.len(),
                saved: true,
            });
        }
    }
    tasks.sort_by_key(|t| t.name.to_lowercase());
    tasks
}

/// One configuration file as an entry of the list, or `None` when it cannot be read as one with at
/// least one folder pair.
fn entry_for_file(file: &Path, saved: bool) -> Option<TaskEntry> {
    let (pairs, verify) = read_pairs(file).ok()?;
    Some(TaskEntry {
        name: file
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: file.to_path_buf(),
        sources: pairs
            .iter()
            .map(|(s, _)| s.to_string_lossy().into_owned())
            .collect(),
        dest: pairs[0].1.to_string_lossy().into_owned(),
        verify,
        jobs: pairs.len(),
        saved,
    })
}

const KNOWN_CONFIGS_FILE: &str = "configs.txt";

impl SessionLog {
    fn known_path(&self) -> PathBuf {
        self.base.join(KNOWN_CONFIGS_FILE)
    }

    fn read_known(&self) -> Vec<PathBuf> {
        std::fs::read_to_string(self.known_path())
            .map(|text| {
                // A hand-edited list may start with a byte-order mark (Notepad and PowerShell 5 add one).
                text.trim_start_matches('\u{feff}')
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn write_known(&self, paths: &[PathBuf]) -> Result<(), IngestError> {
        std::fs::create_dir_all(&self.base).map_err(|e| IngestError::io(&self.base, e))?;
        let text: String = paths
            .iter()
            .map(|p| format!("{}\n", p.to_string_lossy()))
            .collect();
        crate::atomic_write(&self.known_path(), text.as_bytes())
            .map_err(|e| IngestError::io(self.known_path(), e))
    }

    /// Adds a configuration file the person chose to the list, after checking that it can be read as
    /// one with at least one folder pair. The file is **not copied or changed**: only its path is
    /// remembered. Adding the same path twice (case-insensitively) keeps one entry.
    pub fn add_config(&self, file: &Path) -> Result<(), IngestError> {
        let file = std::path::absolute(file).map_err(|e| IngestError::io(file, e))?;
        read_pairs(&file)?;
        let mut known = self.read_known();
        let text = file.to_string_lossy().to_lowercase();
        if !known
            .iter()
            .any(|p| p.to_string_lossy().to_lowercase() == text)
        {
            known.push(file);
            self.write_known(&known)?;
        }
        Ok(())
    }

    /// Takes a configuration file off the list. The file itself is never touched.
    pub fn remove_config(&self, file: &Path) -> Result<(), IngestError> {
        let text = file.to_string_lossy().to_lowercase();
        let known: Vec<PathBuf> = self
            .read_known()
            .into_iter()
            .filter(|p| p.to_string_lossy().to_lowercase() != text)
            .collect();
        self.write_known(&known)
    }

    /// The list of attività: the tasks saved under `tasks_dir` and the configuration files the person
    /// added, by name. A listed file that no longer exists or is no longer readable is skipped, not
    /// removed: it may be on a drive that is simply not connected right now.
    pub fn list_entries(&self, tasks_dir: &Path) -> Vec<TaskEntry> {
        let mut entries = list_tasks(tasks_dir);
        for file in self.read_known() {
            if entries.iter().any(|e| e.path == file) {
                continue;
            }
            if let Some(entry) = entry_for_file(&file, false) {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|t| t.name.to_lowercase());
        entries
    }
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

    fn write_config(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write");
        path
    }

    #[test]
    fn an_added_configuration_is_listed_with_its_job_count_and_never_copied() {
        let base = tempfile::tempdir().expect("tempdir");
        let files = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let config = write_config(
            files.path(),
            "notte.toml",
            "[[jobs]]\nname='a'\nsource='C:\\a'\ndest='D:\\a'\n[[jobs]]\nname='b'\nsource='C:\\b'\ndest='D:\\b'\n",
        );
        log.add_config(&config).expect("add");
        log.add_config(&config)
            .expect("adding twice is not an error");

        let listed = log.list_entries(tasks.path());
        assert_eq!(listed.len(), 1, "one entry however many times it was added");
        assert_eq!(listed[0].jobs, 2);
        assert!(!listed[0].saved, "an added file is not a saved task");
        assert!(config.exists(), "the file is left where it is");
    }

    #[test]
    fn a_list_edited_by_hand_with_a_byte_order_mark_still_reads() {
        let base = tempfile::tempdir().expect("tempdir");
        let files = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let config = write_config(files.path(), "a.toml", "source='C:\\a'\ndest='D:\\a'\n");
        let list = format!("\u{feff}{}\n", config.display());
        std::fs::write(base.path().join(KNOWN_CONFIGS_FILE), list).expect("write");
        assert_eq!(
            SessionLog::at(base.path()).list_entries(tasks.path()).len(),
            1
        );
    }

    #[test]
    fn a_file_that_is_not_a_configuration_is_refused_when_added() {
        let base = tempfile::tempdir().expect("tempdir");
        let files = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let bad = write_config(files.path(), "x.toml", "threads = 2\n");
        assert!(log.add_config(&bad).is_err());
        assert!(log.add_config(&files.path().join("missing.toml")).is_err());
    }

    #[test]
    fn removing_a_configuration_takes_it_off_the_list_and_leaves_the_file() {
        let base = tempfile::tempdir().expect("tempdir");
        let files = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let config = write_config(files.path(), "a.toml", "source='C:\\a'\ndest='D:\\a'\n");
        log.add_config(&config).expect("add");
        let absolute = std::path::absolute(&config).expect("abs");
        log.remove_config(&absolute).expect("remove");
        assert!(log.list_entries(tasks.path()).is_empty());
        assert!(config.exists());
    }

    #[test]
    fn a_listed_file_that_disappeared_is_skipped_not_forgotten() {
        let base = tempfile::tempdir().expect("tempdir");
        let files = tempfile::tempdir().expect("tempdir");
        let tasks = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let config = write_config(files.path(), "a.toml", "source='C:\\a'\ndest='D:\\a'\n");
        log.add_config(&config).expect("add");
        std::fs::remove_file(&config).expect("remove file");
        assert!(
            log.list_entries(tasks.path()).is_empty(),
            "nothing to show right now"
        );
        std::fs::write(&config, "source='C:\\a'\ndest='D:\\a'\n").expect("back again");
        assert_eq!(
            log.list_entries(tasks.path()).len(),
            1,
            "it comes back when the drive does"
        );
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
    fn write_checkpoint_in(dir: &Path) -> PathBuf {
        use clap::Parser;
        let mut args = crate::cli::Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            "D:/src",
            "--dest",
            "E:/dst",
        ])
        .expect("parse");
        args.source = Some(PathBuf::from("D:/src"));
        args.dest = Some(PathBuf::from("E:/dst"));
        let path = dir.join("run.checkpoint.json");
        crate::checkpoint::Checkpoint::new(&args, "interrupted by Ctrl+C")
            .write_to(&path)
            .expect("write checkpoint");
        path
    }

    #[test]
    fn a_checkpoint_left_beside_a_task_can_be_resumed_and_a_foreign_one_cannot() {
        let base = tempfile::tempdir().expect("tempdir");
        let files = tempfile::tempdir().expect("tempdir");
        let elsewhere = tempfile::tempdir().expect("tempdir");
        let task = write_config(
            files.path(),
            "t.toml",
            "source='C:/a'
dest='D:/a'
",
        );
        let log = SessionLog::at(base.path());
        let first = log.begin_task(&task).expect("begin");
        let own = write_checkpoint_in(files.path());
        let foreign = write_checkpoint_in(elsewhere.path());

        assert_eq!(log.checkpoints_of(&first.id).len(), 1);
        let refused = log.begin_resume(&first.id, &foreign);
        assert!(
            refused.is_err(),
            "a checkpoint of another folder is refused"
        );

        let resumed = log.begin_resume(&first.id, &own).expect("own checkpoint");
        assert_eq!(
            resumed.config, own,
            "the run is started from the checkpoint"
        );
        let summary = log.list(1, Some(&resumed.id));
        assert_eq!(summary[0].task.as_deref(), Some(task.as_path()));
        assert_eq!(summary[0].resume_dir.as_deref(), Some(files.path()));
    }

    #[test]
    fn a_resumed_run_counts_only_the_reports_written_since_it_began() {
        let base = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let first = log
            .begin(
                &[PathBuf::from("C:/a")],
                Path::new("D:/out"),
                &[pair("C:/a", "D:/out/a")],
            )
            .expect("begin");
        let old_report = first.dir.join("robocopy_ingest_report_old.json");
        std::fs::write(&old_report, "{}").expect("write");
        let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&old_report)
            .and_then(|f| f.set_modified(old_time))
            .expect("age the report");
        let checkpoint = write_checkpoint_in(&first.dir);

        let resumed = log.begin_resume(&first.id, &checkpoint).expect("resume");
        let new_report = first.dir.join("robocopy_ingest_report_new.json");
        std::fs::write(&new_report, "{}").expect("write");
        log.finish(&resumed.id, 0).expect("finish");

        let summary = log.list(1, Some(&resumed.id));
        assert_eq!(summary[0].reports, vec![new_report]);
    }
    #[test]
    fn a_checkpoint_already_resumed_to_a_clean_end_is_no_longer_offered() {
        let base = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        let first = log
            .begin(
                &[PathBuf::from("C:/a")],
                Path::new("D:/out"),
                &[pair("C:/a", "D:/out/a")],
            )
            .expect("begin");
        let checkpoint = write_checkpoint_in(&first.dir);
        assert_eq!(log.checkpoints_of(&first.id).len(), 1);

        let resumed = log.begin_resume(&first.id, &checkpoint).expect("resume");
        assert_eq!(
            log.checkpoints_of(&first.id).len(),
            1,
            "still offered while the resume has not finished cleanly"
        );
        std::fs::write(
            first.dir.join("robocopy_ingest_report_done.json"),
            CLEAN_REPORT,
        )
        .expect("write");
        assert_eq!(
            log.finish(&resumed.id, 0).expect("finish"),
            SessionState::Clean
        );
        assert!(
            log.checkpoints_of(&first.id).is_empty(),
            "a resume that ended clean settles the checkpoint"
        );
    }
    #[test]
    fn recent_folders_are_newest_first_without_repeats_or_blanks() {
        let base = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::at(base.path());
        for (src, dst) in [
            (r"C:\foto", r"D:\backup"),
            (r"C:\docs", r"D:\backup"),
            (r"c:\FOTO", r"E:\altro"),
        ] {
            log.begin(&[PathBuf::from(src)], Path::new(dst), &[pair(src, dst)])
                .expect("begin");
        }
        let recent = recent_folders(&log.list(10, None), 8);
        assert_eq!(recent.sources, vec![r"c:\FOTO", r"C:\docs"]);
        assert_eq!(recent.dests, vec![r"E:\altro", r"D:\backup"]);
        assert_eq!(recent_folders(&log.list(10, None), 1).dests.len(), 1);
    }
}
