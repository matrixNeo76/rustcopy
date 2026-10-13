//! F36: lightweight scheduling via Windows Task Scheduler (`schtasks.exe`), rather than an
//! internal scheduler process — same "shell out to a native tool" pattern already used for VSS
//! (`vssadmin.exe`, F30). A scheduled job is nothing more than a Task Scheduler entry that
//! re-invokes this same binary, with the same arguments used to install it (minus the scheduling
//! flags themselves), on a timer. `rustcopy` itself has no long-running scheduler; once installed,
//! Windows itself is what wakes the binary up.
//!
//! **Architectural decision (recorded in `ROADMAP.md`'s F36 row)**: this was chosen over an
//! internal scheduler living inside a persistent Windows service (F37) specifically to decouple
//! the two — F36 needs no service at all, and F37 remains its own task focused on the
//! notify-server (F41).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::errors::IngestError;

/// A parsed `--install-schedule <SPEC>` value. Deliberately a small, fixed set of trigger shapes
/// rather than a general cron grammar — covers the common Cobian-parity cases (daily, every N
/// hours, specific weekdays) without pulling in a cron-expression parser for a v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleSpec {
    /// `daily@HH:MM`
    Daily { time: String },
    /// `hourly@N` — every N hours, starting from whenever the task is created.
    Hourly { every_n_hours: u32 },
    /// `weekly@DAY1,DAY2,...@HH:MM` — `DAY` is a 3-letter weekday code (`MON`..`SUN`).
    Weekly { days: Vec<String>, time: String },
}

const WEEKDAY_CODES: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];

/// Parses `--install-schedule`'s value. See `ScheduleSpec` for the accepted shapes.
/// `schtasks.exe /SC HOURLY /MO N` accepts 1..=23. Verified empirically against the real binary,
/// not read from documentation: `/MO 23` is accepted, `/MO 24` and above are refused.
const MAX_HOURLY_INTERVAL: u32 = 23;

pub fn parse_schedule_spec(spec: &str) -> Result<ScheduleSpec, IngestError> {
    let invalid = || IngestError::InvalidScheduleSpec(spec.to_string());

    let (kind, rest) = spec.split_once('@').ok_or_else(invalid)?;
    match kind {
        "daily" => Ok(ScheduleSpec::Daily {
            time: parse_time(rest).ok_or_else(invalid)?,
        }),
        "hourly" => {
            let n: u32 = rest.parse().map_err(|_| invalid())?;
            // `schtasks.exe /SC HOURLY /MO N` accepts 1..=23 and nothing above -- verified against
            // the real binary, which answers `/MO 24` with "valore non valido per l'opzione /MO".
            // Rejecting here turns a failure the operator would only meet at install time, with
            // schtasks' own opaque message, into one that names the flag and the range.
            if !(1..=MAX_HOURLY_INTERVAL).contains(&n) {
                return Err(IngestError::InvalidScheduleSpec(format!(
                    "hourly@{n}: the interval must be between 1 and {MAX_HOURLY_INTERVAL} hours                      (schtasks.exe rejects anything above {MAX_HOURLY_INTERVAL}). For longer                      gaps use daily@HH:MM."
                )));
            }
            Ok(ScheduleSpec::Hourly { every_n_hours: n })
        }
        "weekly" => {
            let (days_raw, time_raw) = rest.split_once('@').ok_or_else(invalid)?;
            let days: Vec<String> = days_raw
                .split(',')
                .map(|d| d.trim().to_uppercase())
                .collect();
            if days.is_empty() || days.iter().any(|d| !WEEKDAY_CODES.contains(&d.as_str())) {
                return Err(invalid());
            }
            Ok(ScheduleSpec::Weekly {
                days,
                time: parse_time(time_raw).ok_or_else(invalid)?,
            })
        }
        _ => Err(invalid()),
    }
}

/// Parses and normalises an `HH:MM` 24h time, or `None` if it isn't one.
fn parse_time(time: &str) -> Option<String> {
    let (h_raw, m_raw) = time.split_once(':')?;
    let h: u32 = h_raw.parse().ok()?;
    let m: u32 = m_raw.parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some(format!("{h:02}:{m:02}"))
}

/// The `schtasks.exe /Create ...` arguments that register the task described by the XML file at
/// `xml_path` under `name`. `/F` forces overwriting an existing task of the same name, so
/// re-running `--install-schedule` with the same `--schedule-name` updates it instead of failing.
///
/// The task is described by **XML**, not by `/SC`/`/ST`/`/TR` flags, because those flags cannot
/// express the settings that decide whether a nightly backup actually happens. A task created from
/// flags gets Task Scheduler's defaults, read back from a real one (`schtasks /Query /XML`):
/// it does **not** start on battery and is **stopped** if the machine goes onto battery, it is
/// killed after 72 hours, and a run missed because the PC was off or asleep at the scheduled time
/// is simply lost. A laptop backing up at 02:00 is exactly the case those defaults break.
pub fn build_create_args(name: &str, xml_path: &str) -> Vec<String> {
    vec![
        "/Create".to_string(),
        "/TN".to_string(),
        name.to_string(),
        "/XML".to_string(),
        xml_path.to_string(),
        "/F".to_string(),
    ]
}

/// The moment the task's trigger starts counting from: always in the **future**. A start in the
/// past is not harmless once `StartWhenAvailable` is on (it is what makes a missed run catch up),
/// because the first occurrence would then count as already missed and fire at install time.
pub fn first_start(spec: &ScheduleSpec, now: chrono::NaiveDateTime) -> chrono::NaiveDateTime {
    use chrono::{Datelike, Duration, NaiveTime, Timelike};
    let at = |date: chrono::NaiveDate, time: &str| {
        let parsed = NaiveTime::parse_from_str(time, "%H:%M").unwrap_or(NaiveTime::MIN);
        date.and_time(parsed)
    };
    match spec {
        ScheduleSpec::Daily { time } => {
            let today = at(now.date(), time);
            if today > now {
                today
            } else {
                today + Duration::days(1)
            }
        }
        ScheduleSpec::Weekly { days, time } => {
            for offset in 0..=7 {
                let date = now.date() + Duration::days(offset);
                let code = WEEKDAY_CODES[date.weekday().num_days_from_monday() as usize];
                let candidate = at(date, time);
                if days.iter().any(|d| d == code) && candidate > now {
                    return candidate;
                }
            }
            // Unreachable for a parsed spec (it has at least one valid day), but a fixed fallback
            // is better than a panic in the middle of an install.
            at(now.date() + Duration::days(1), time)
        }
        ScheduleSpec::Hourly { .. } => {
            let next = now + Duration::minutes(1);
            next.with_second(0)
                .unwrap_or(next)
                .with_nanosecond(0)
                .unwrap_or(next)
        }
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn weekday_element(code: &str) -> &'static str {
    match code {
        "MON" => "Monday",
        "TUE" => "Tuesday",
        "WED" => "Wednesday",
        "THU" => "Thursday",
        "FRI" => "Friday",
        "SAT" => "Saturday",
        _ => "Sunday",
    }
}

/// The Task Scheduler XML for `spec`, running `exe` with `arguments` (one command-line string,
/// already quoted where needed), first firing at `start`.
///
/// What it sets, and why (each is a deliberate departure from what `/SC` flags give):
/// - `DisallowStartIfOnBatteries`/`StopIfGoingOnBatteries` **off**: a backup on a laptop must neither
///   refuse to start nor be killed halfway because the charger was unplugged;
/// - `StartWhenAvailable` **on**: the run missed because the PC was off or asleep happens as soon as
///   it can, instead of being lost until the next day;
/// - `ExecutionTimeLimit` `PT0S` (none): the default kills a task after 72 hours, which a large
///   first copy can exceed;
/// - `MultipleInstancesPolicy` `IgnoreNew`: a slow run is not started on top of itself.
///
/// The task still runs only while its owner is logged on (`InteractiveToken`): running "whether or
/// not logged on" needs the account password, which this tool never handles.
pub fn build_task_xml(
    spec: &ScheduleSpec,
    exe: &str,
    arguments: &str,
    start: chrono::NaiveDateTime,
) -> String {
    build_task_xml_with(spec, exe, arguments, start, &TaskOptions::default())
}

/// What differs between an ordinary schedule and an elevated one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskOptions {
    /// Run with the highest privileges the account has (`HighestAvailable`) instead of a limited token.
    pub elevated: bool,
    /// The folder the program starts in. Task Scheduler's default is the system folder, which would put
    /// every relative report and log path somewhere nobody looks.
    pub working_dir: Option<String>,
}

/// [`build_task_xml`] with the options an elevated schedule needs.
pub fn build_task_xml_with(
    spec: &ScheduleSpec,
    exe: &str,
    arguments: &str,
    start: chrono::NaiveDateTime,
    options: &TaskOptions,
) -> String {
    let run_level = if options.elevated {
        "HighestAvailable"
    } else {
        "LeastPrivilege"
    };
    let working_dir = options
        .working_dir
        .as_deref()
        .map(|dir| format!("<WorkingDirectory>{}</WorkingDirectory>", xml_escape(dir)))
        .unwrap_or_default();
    let boundary = start.format("%Y-%m-%dT%H:%M:%S");
    let trigger = match spec {
        ScheduleSpec::Daily { .. } => format!(
            "<CalendarTrigger><StartBoundary>{boundary}</StartBoundary><Enabled>true</Enabled>\
             <ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay></CalendarTrigger>"
        ),
        ScheduleSpec::Hourly { every_n_hours } => format!(
            "<TimeTrigger><StartBoundary>{boundary}</StartBoundary><Enabled>true</Enabled>\
             <Repetition><Interval>PT{every_n_hours}H</Interval></Repetition></TimeTrigger>"
        ),
        ScheduleSpec::Weekly { days, .. } => {
            let days: String = days
                .iter()
                .map(|d| format!("<{0}/>", weekday_element(d)))
                .collect();
            format!(
                "<CalendarTrigger><StartBoundary>{boundary}</StartBoundary><Enabled>true</Enabled>\
                 <ScheduleByWeek><DaysOfWeek>{days}</DaysOfWeek><WeeksInterval>1</WeeksInterval>\
                 </ScheduleByWeek></CalendarTrigger>"
            )
        }
    };
    let command = xml_escape(exe);
    let arguments = xml_escape(arguments);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n\
         <Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n\
         <RegistrationInfo><Description>rustcopy scheduled backup</Description></RegistrationInfo>\n\
         <Triggers>{trigger}</Triggers>\n\
         <Principals><Principal id=\"Author\"><LogonType>InteractiveToken</LogonType>\
         <RunLevel>{run_level}</RunLevel></Principal></Principals>\n\
         <Settings>\
         <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\
         <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\
         <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\
         <AllowHardTerminate>true</AllowHardTerminate>\
         <StartWhenAvailable>true</StartWhenAvailable>\
         <AllowStartOnDemand>true</AllowStartOnDemand>\
         <Enabled>true</Enabled>\
         <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\
         </Settings>\n\
         <Actions Context=\"Author\"><Exec><Command>{command}</Command>\
         <Arguments>{arguments}</Arguments>{working_dir}</Exec></Actions>\n\
         </Task>\n"
    )
}

/// The XML as the bytes `schtasks /XML` reads reliably: UTF-16 little endian with a byte-order mark.
pub fn task_xml_bytes(xml: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// The `schtasks.exe /Delete ...` arguments for removing `name`. `/F` suppresses the interactive
/// confirmation prompt schtasks would otherwise show.
pub fn build_delete_args(name: &str) -> Vec<String> {
    vec![
        "/Delete".to_string(),
        "/TN".to_string(),
        name.to_string(),
        "/F".to_string(),
    ]
}

/// The CLI flags that only make sense at install/uninstall time — never part of the recurring
/// invocation Task Scheduler should actually run.
const SCHEDULE_ONLY_FLAGS: [&str; 3] = [
    "--install-schedule",
    "--schedule-name",
    "--uninstall-schedule",
];

/// The scheduling flags that take no value.
const SCHEDULE_ONLY_SWITCHES: [&str; 1] = ["--schedule-elevated"];

/// Strips `--install-schedule`/`--schedule-name`/`--uninstall-schedule` (and their values) out of
/// a raw argv, in either `--flag value` or `--flag=value` form — what's left is exactly the
/// backup invocation that should run on a timer.
pub fn strip_schedule_flags(args: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if SCHEDULE_ONLY_SWITCHES.contains(&arg.as_str()) {
            i += 1; // a switch has no value token
            continue;
        }
        if SCHEDULE_ONLY_FLAGS.contains(&arg.as_str()) {
            i += 2; // skip the flag and its separate value token
            continue;
        }
        if SCHEDULE_ONLY_FLAGS
            .iter()
            .any(|flag| arg.starts_with(flag) && arg.as_bytes().get(flag.len()) == Some(&b'='))
        {
            i += 1; // `--flag=value` is a single token
            continue;
        }
        result.push(arg.clone());
        i += 1;
    }
    result
}

/// Quotes `s` for inclusion in a Windows command line if it contains whitespace (paths are the
/// common case). Escapes embedded double quotes so a quoted argument can't be broken out of.
fn quote_if_needed(s: &str) -> String {
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// Builds the full `/TR` command line: the given executable path followed by `filtered_args`
/// (already stripped of the scheduling-only flags via `strip_schedule_flags`).
pub fn build_task_run_command(exe_path: &Path, filtered_args: &[String]) -> String {
    let mut parts = vec![quote_if_needed(&exe_path.display().to_string())];
    parts.extend(filtered_args.iter().map(|a| quote_if_needed(a)));
    parts.join(" ")
}

/// Just the arguments of [`build_task_run_command`], for the task XML, which keeps the program and
/// its arguments apart.
pub fn build_task_arguments(filtered_args: &[String]) -> String {
    filtered_args
        .iter()
        .map(|a| quote_if_needed(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the console may ask the command line to install, after checking the safety level (GUI Slint
/// plan, phase 6). The console never writes a schedule itself: it prepares **this fixed argument list**,
/// runs the CLI with it (`--install-schedule` is the CLI's own, F36) and lets Windows' Task Scheduler do
/// the rest. Pure, so every refusal is a unit test.
///
/// Refused, each with a sentence the person can act on:
/// - a level below Standard ([`crate::safety::SafetyLevel::can_prepare_schedules`]) -- checked **here**,
///   in the core, not only by hiding a button;
/// - a malformed `spec` (the CLI's own grammar, [`parse_schedule_spec`]) or an unusable task `name`;
/// - a configuration that cannot be read, or that has a job which **mirrors** or **prunes generations**:
///   a schedule runs unattended, and the console never starts an unattended purge;
/// - a job with a relative source or destination: the Task Scheduler starts the program from a system
///   folder, so a relative path would resolve somewhere else on every run.
pub fn install_arguments(
    level: crate::safety::SafetyLevel,
    config: &Path,
    spec: &str,
    name: &str,
) -> Result<Vec<String>, IngestError> {
    if !level.can_prepare_schedules() {
        return Err(IngestError::Schedule(format!(
            "Il livello di sicurezza «{}» non permette di preparare pianificazioni: servono almeno «{}».",
            level.name(),
            crate::safety::SafetyLevel::Standard.name()
        )));
    }
    parse_schedule_spec(spec)?;
    validate_task_name(name)?;

    let drafts = crate::job_editor::read_drafts(config)?;
    for draft in &drafts {
        if draft.mirror || draft.keep_generations.is_some() {
            return Err(IngestError::Schedule(format!(
                "Il job «{}» fa un mirror o elimina vecchie generazioni: una pianificazione parte senza nessuno davanti e la console non avvia mai una pulizia non presidiata. Si pianifica soltanto dalla riga di comando, con una scelta esplicita.",
                draft.name
            )));
        }
        for path in [&draft.source, &draft.dest] {
            if !is_absolute_windows_path(path) {
                return Err(IngestError::Schedule(format!(
                    "Il job «{}» usa il percorso relativo «{path}»: Windows avvia una pianificazione da una cartella di sistema, quindi quel percorso punterebbe altrove. Usa percorsi completi.",
                    draft.name
                )));
            }
        }
    }

    let config = std::path::absolute(config).map_err(|e| IngestError::io(config, e))?;
    Ok(vec![
        "--config".to_string(),
        config.display().to_string(),
        "--install-schedule".to_string(),
        spec.to_string(),
        "--schedule-name".to_string(),
        name.to_string(),
    ])
}

/// What an **elevated** schedule refuses, over and above the ordinary rules (checked by the console's
/// request and again by the elevated process itself, which does not trust that the request was checked):
/// anything that deletes or runs a command. An elevated task is the only thing here that runs with
/// administrator rights, so what it may do is limited to copying (and shadow-copying) files.
///
/// - `mirror` and `keep_generations`: the same reason a schedule never starts an unattended purge;
/// - `pre_command`/`post_command`: arbitrary commands, which would run elevated;
/// - a relative source or destination (the same rule as any schedule).
pub fn validate_for_elevation(config: &Path) -> Result<(), IngestError> {
    let text = std::fs::read_to_string(config).map_err(|e| IngestError::io(config, e))?;
    validate_elevation_text(&text, config)
}

/// [`validate_for_elevation`] on text already read. The installer reads the file **once** and validates
/// and copies those very bytes: validating the path and then copying the path again would let the
/// person (or anything running as them) swap the file in between and have the unchecked version run
/// elevated.
pub fn validate_elevation_text(text: &str, origin: &Path) -> Result<(), IngestError> {
    let parsed: crate::config::IngestConfig = toml::from_str(text).map_err(|err| {
        IngestError::io(
            origin,
            std::io::Error::new(std::io::ErrorKind::InvalidData, err),
        )
    })?;
    let jobs = parsed.jobs.clone().unwrap_or_default();
    let mut every: Vec<(String, crate::config::JobConfig)> = Vec::new();
    if jobs.is_empty() {
        every.push(("job1".to_string(), parsed.defaults.clone()));
    } else {
        for (index, job) in jobs.iter().enumerate() {
            every.push((
                job.name
                    .clone()
                    .unwrap_or_else(|| format!("job{}", index + 1)),
                job.merged_over(&parsed.defaults),
            ));
        }
    }
    for (name, job) in &every {
        let refuse = |why: &str| {
            Err(IngestError::Schedule(format!(
                "Il job «{name}» {why}: una pianificazione elevata gira con diritti da amministratore e può soltanto copiare."
            )))
        };
        if job.mirror == Some(true) {
            return refuse("fa un mirror");
        }
        if job.keep_generations.is_some() {
            return refuse("elimina vecchie generazioni");
        }
        if job.pre_command.is_some() || job.post_command.is_some() {
            return refuse("esegue un comando prima o dopo la copia");
        }
        // The report and the log are written by the elevated process: a path the person chose, in a
        // folder they can change, is a place where a swapped link would make that process overwrite
        // some other file. An elevated schedule writes both inside its own protected folder.
        if job.report_path.is_some() || job.log_path.is_some() || job.html_report_path.is_some() {
            return refuse("sceglie dove scrivere il report o il registro");
        }
        for path in [&job.source, &job.dest] {
            let Some(path) = path else {
                return refuse("non ha un'origine e una destinazione");
            };
            let text = path.to_string_lossy();
            if !is_absolute_windows_path(&text) {
                return refuse(&format!("usa il percorso relativo «{text}»"));
            }
        }
    }
    Ok(())
}

/// The argument list the console hands to the command line **through the elevation prompt** to install an
/// elevated schedule. Fixed shape: only the validated configuration, spec and name vary.
pub fn elevated_install_arguments(
    level: crate::safety::SafetyLevel,
    config: &Path,
    spec: &str,
    name: &str,
) -> Result<Vec<String>, IngestError> {
    if !level.can_prepare_elevated_schedules() {
        return Err(IngestError::Schedule(format!(
            "Il livello di sicurezza «{}» non permette una pianificazione con privilegi elevati: serve «{}».",
            level.name(),
            crate::safety::SafetyLevel::Expert.name()
        )));
    }
    parse_schedule_spec(spec)?;
    validate_task_name(name)?;
    validate_for_elevation(config)?;
    let config = std::path::absolute(config).map_err(|e| IngestError::io(config, e))?;
    Ok(vec![
        "--config".to_string(),
        config.display().to_string(),
        "--install-schedule".to_string(),
        spec.to_string(),
        "--schedule-name".to_string(),
        name.to_string(),
        "--schedule-elevated".to_string(),
    ])
}

/// The argument list that removes an elevated schedule (and its copy) through the elevation prompt.
pub fn elevated_removal_arguments(
    level: crate::safety::SafetyLevel,
    name: &str,
) -> Result<Vec<String>, IngestError> {
    if !level.can_prepare_elevated_schedules() {
        return Err(IngestError::Schedule(format!(
            "Il livello di sicurezza «{}» non permette di togliere una pianificazione elevata: serve «{}».",
            level.name(),
            crate::safety::SafetyLevel::Expert.name()
        )));
    }
    validate_task_name(name)?;
    Ok(vec![
        "--uninstall-schedule".to_string(),
        name.to_string(),
        "--schedule-elevated".to_string(),
    ])
}

/// Whether a task's command line runs the copy of an elevated schedule.
pub fn is_elevated_command(command: &str) -> bool {
    command.to_lowercase().contains(&format!(
        "\\{}\\",
        crate::elevation::ELEVATED_SUBDIR.to_lowercase()
    ))
}

/// The command a schedule installed by [`install_elevated`] runs: the copy of the configuration, with
/// the shadow copy switched on and the report and log inside the protected folder `dir`.
pub fn elevated_arguments(copy: &Path, dir: &Path) -> Vec<String> {
    vec![
        "--config".to_string(),
        copy.display().to_string(),
        "--vss-snapshot".to_string(),
        "--report-path".to_string(),
        dir.join("robocopy_ingest_report.json")
            .display()
            .to_string(),
        "--log-path".to_string(),
        dir.join("robocopy_ingest.log").display().to_string(),
        // The elevated process writes with administrator rights into a folder the person chose:
        // robocopy would write through a junction left there, so the run refuses to start (and
        // refuses to succeed) when one is.
        "--refuse-dest-links".to_string(),
    ]
}

/// Installs an elevated schedule (the **elevated** command line). Refuses to do anything unless this
/// process really is elevated: copying a configuration into a protected folder and registering a task
/// that runs it with the highest rights are exactly the two steps that must not happen otherwise.
#[cfg(windows)]
pub fn install_elevated(name: &str, spec: &ScheduleSpec, config: &Path) -> Result<(), IngestError> {
    if !crate::elevation::is_elevated() {
        return Err(IngestError::Schedule(
            "una pianificazione con privilegi elevati si crea solo da un processo eseguito come amministratore (la console lo avvia dopo la richiesta di Windows)".to_string(),
        ));
    }
    validate_task_name(name)?;
    // Read once: what is validated is what is installed, byte for byte.
    let bytes = std::fs::read(config).map_err(|e| IngestError::io(config, e))?;
    let text = String::from_utf8(bytes.clone()).map_err(|_| {
        IngestError::Schedule("la configurazione non è testo UTF-8 valido".to_string())
    })?;
    validate_elevation_text(&text, config)?;
    let program_data = crate::elevation::program_data_dir()?;
    let dir = crate::elevation::elevated_dir_in(&program_data, name);
    crate::elevation::prepare_protected_dir(&program_data, &dir)?;
    let copy = dir.join("config.toml");
    std::fs::write(&copy, &bytes).map_err(|e| IngestError::io(&copy, e))?;
    // The program that runs elevated is a copy kept in the protected folder too. Pointing the task at the
    // installed program would hand administrator rights to whatever a limited user can write to that
    // folder (a build folder, a user profile, a badly protected install).
    let source_exe = std::env::current_exe()
        .map_err(|e| IngestError::io(Path::new("<current executable>"), e))?;
    let exe = dir.join("robocopy_ingest.exe");
    std::fs::copy(&source_exe, &exe).map_err(|e| IngestError::io(&exe, e))?;
    let options = TaskOptions {
        elevated: true,
        working_dir: Some(dir.display().to_string()),
    };
    let xml = build_task_xml_with(
        spec,
        &exe.display().to_string(),
        &build_task_arguments(&elevated_arguments(&copy, &dir)),
        first_start(spec, chrono::Local::now().naive_local()),
        &options,
    );
    // The XML is written inside the protected folder, not in a temp folder a limited process could swap
    // between the write and the read.
    let xml_file = dir.join("task.xml");
    std::fs::write(&xml_file, task_xml_bytes(&xml)).map_err(|e| IngestError::io(&xml_file, e))?;
    let result = run_schtasks(&build_create_args(name, &xml_file.display().to_string()));
    let _ = std::fs::remove_file(&xml_file);
    result
}

#[cfg(not(windows))]
pub fn install_elevated(
    _name: &str,
    _spec: &ScheduleSpec,
    _config: &Path,
) -> Result<(), IngestError> {
    Err(IngestError::Schedule(
        "le pianificazioni elevate richiedono Windows".to_string(),
    ))
}

/// Removes an elevated schedule and its copy of the configuration (the **elevated** command line). The
/// reports the runs left in the folder stay: they are the person's record, and `remove_dir` (never
/// `remove_dir_all`) refuses a folder that is not empty.
#[cfg(windows)]
pub fn uninstall_elevated(name: &str) -> Result<(), IngestError> {
    if !crate::elevation::is_elevated() {
        return Err(IngestError::Schedule(
            "una pianificazione con privilegi elevati si toglie solo da un processo eseguito come amministratore".to_string(),
        ));
    }
    validate_task_name(name)?;
    run_schtasks(&build_delete_args(name))?;
    let dir = crate::elevation::elevated_dir_in(&crate::elevation::program_data_dir()?, name);
    for file in ["config.toml", "task.xml", "robocopy_ingest.exe"] {
        let _ = std::fs::remove_file(dir.join(file));
    }
    let _ = std::fs::remove_dir(&dir);
    Ok(())
}

#[cfg(not(windows))]
pub fn uninstall_elevated(_name: &str) -> Result<(), IngestError> {
    Err(IngestError::Schedule(
        "le pianificazioni elevate richiedono Windows".to_string(),
    ))
}

/// The argument list that removes the schedule `name`, under the same level check. Fixed shape: only
/// `--uninstall-schedule` and the (validated) name.
pub fn removal_arguments(
    level: crate::safety::SafetyLevel,
    name: &str,
) -> Result<Vec<String>, IngestError> {
    if !level.can_prepare_schedules() {
        return Err(IngestError::Schedule(format!(
            "Il livello di sicurezza «{}» non permette di togliere pianificazioni: servono almeno «{}».",
            level.name(),
            crate::safety::SafetyLevel::Standard.name()
        )));
    }
    validate_task_name(name)?;
    Ok(vec!["--uninstall-schedule".to_string(), name.to_string()])
}

/// A task name `schtasks.exe` takes without surprises: letters, digits, space, `_`, `.` and `-`, up to 64
/// characters, not starting with a space or a dot, and without the `\` that would put the task in a folder.
fn validate_task_name(name: &str) -> Result<(), IngestError> {
    let ok = !name.is_empty()
        && name.chars().count() <= 64
        && !name.starts_with([' ', '.'])
        && !name.ends_with(' ')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '.' | '-'));
    if ok {
        Ok(())
    } else {
        Err(IngestError::Schedule(format!(
            "Il nome «{name}» non va bene per una pianificazione: usa lettere, cifre, spazio, trattino, punto o trattino basso (massimo 64 caratteri)."
        )))
    }
}

/// `C:\...`, `C:/...` or a UNC path. Plain string logic, so it gives the same answer on every host
/// (see `vss::remap_to_shadow`, D16).
fn is_absolute_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    drive || path.starts_with("\\\\")
}

/// Registers the task: writes its XML to a private temporary file, hands it to `schtasks /XML`,
/// and removes the file whatever the outcome.
#[cfg(windows)]
pub fn install(
    name: &str,
    spec: &ScheduleSpec,
    exe_path: &Path,
    filtered_args: &[String],
) -> Result<(), IngestError> {
    let xml = build_task_xml(
        spec,
        &exe_path.display().to_string(),
        &build_task_arguments(filtered_args),
        first_start(spec, chrono::Local::now().naive_local()),
    );
    let file = std::env::temp_dir().join(format!(
        "rustcopy-task-{}-{}.xml",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&file, task_xml_bytes(&xml)).map_err(|e| IngestError::io(&file, e))?;
    let result = run_schtasks(&build_create_args(name, &file.display().to_string()));
    let _ = std::fs::remove_file(&file);
    result
}

#[cfg(windows)]
pub fn uninstall(name: &str) -> Result<(), IngestError> {
    run_schtasks(&build_delete_args(name))
}

#[cfg(not(windows))]
pub fn install(
    _name: &str,
    _spec: &ScheduleSpec,
    _exe_path: &Path,
    _filtered_args: &[String],
) -> Result<(), IngestError> {
    Err(IngestError::Schedule(
        "--install-schedule requires Windows Task Scheduler (schtasks.exe), unavailable on this platform"
            .to_string(),
    ))
}

#[cfg(not(windows))]
pub fn uninstall(_name: &str) -> Result<(), IngestError> {
    Err(IngestError::Schedule(
        "--uninstall-schedule requires Windows Task Scheduler (schtasks.exe), unavailable on this platform"
            .to_string(),
    ))
}

#[cfg(windows)]
fn run_schtasks(args: &[String]) -> Result<(), IngestError> {
    let mut command = std::process::Command::new(crate::elevation::system_program("schtasks.exe"));
    command.args(args);
    // The caller may be the CLI (already has a console) or the GUI (a windowed process with none) —
    // without this, `schtasks.exe` is a console-subsystem binary and Windows allocates it a fresh
    // console regardless, flashing a black window in front of the console on every install/uninstall
    // even though `.output()` already captures everything it would print. Same fix, same reasoning
    // as `main.rs`'s child-process spawn (F54).
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|source| IngestError::SpawnFailed {
            program: "schtasks.exe".to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(IngestError::Schedule(format!(
            "schtasks.exe {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// The XML with the whitespace Windows adds between tags removed, so a task registered from our own
/// compact XML and one Windows rewrote with indentation read the same.
fn compact_xml(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut between_tags = false;
    for line in xml.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if between_tags && !line.starts_with('<') {
            out.push(' ');
        }
        out.push_str(line);
        between_tags = line.ends_with('>');
    }
    out
}

/// The text between `<tag>` and `</tag>`, first occurrence, in `xml`.
fn tag_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{tag}>"))? + start;
    Some(&xml[start..end])
}

/// `HH:MM` out of a `StartBoundary` such as `2026-10-13T22:00:00`.
fn boundary_time(boundary: &str) -> Option<String> {
    let time = boundary.split_once('T')?.1;
    Some(time.get(..5)?.to_string())
}

/// When a stored task's trigger fires, in plain Italian, for the shapes this module registers
/// (`daily`, `hourly`, `weekly`) and a name for the common others. Empty when the XML has no trigger.
///
/// Read from the stored XML rather than the CSV `schtasks` prints: its labels and its dates are
/// localized, the element names are not.
pub fn describe_trigger(xml: &str) -> String {
    let xml = compact_xml(xml);
    let Some(triggers) = tag_text(&xml, "Triggers") else {
        return String::new();
    };
    let at = tag_text(triggers, "StartBoundary")
        .and_then(boundary_time)
        .map(|t| format!(" alle {t}"))
        .unwrap_or_default();
    if let Some(days) = tag_text(triggers, "DaysOfWeek") {
        const NAMES: [(&str, &str); 7] = [
            ("Monday", "Lun"),
            ("Tuesday", "Mar"),
            ("Wednesday", "Mer"),
            ("Thursday", "Gio"),
            ("Friday", "Ven"),
            ("Saturday", "Sab"),
            ("Sunday", "Dom"),
        ];
        let picked: Vec<&str> = NAMES
            .iter()
            .filter(|(element, _)| days.contains(&format!("<{element}")))
            .map(|(_, short)| *short)
            .collect();
        return format!("{}{at}", picked.join(", "));
    }
    if triggers.contains("<ScheduleByDay>") {
        let every = tag_text(triggers, "DaysInterval")
            .and_then(|n| n.trim().parse::<u32>().ok())
            .unwrap_or(1);
        return if every <= 1 {
            format!("Ogni giorno{at}")
        } else {
            format!("Ogni {every} giorni{at}")
        };
    }
    if let Some(interval) = tag_text(triggers, "Interval") {
        let interval = interval.trim();
        if let Some(hours) = interval
            .strip_prefix("PT")
            .and_then(|r| r.strip_suffix('H'))
        {
            return format!("Ogni {hours} ore");
        }
        if let Some(minutes) = interval
            .strip_prefix("PT")
            .and_then(|r| r.strip_suffix('M'))
        {
            return format!("Ogni {minutes} minuti");
        }
    }
    if triggers.contains("<LogonTrigger>") {
        return "All'accesso".to_string();
    }
    if triggers.contains("<BootTrigger>") {
        return "All'avvio del computer".to_string();
    }
    "Pianificata".to_string()
}

/// Whether the task is switched off: `<Enabled>false</Enabled>` under `<Settings>` (the triggers carry
/// their own `<Enabled>`, which is not this).
pub fn task_is_disabled(xml: &str) -> bool {
    let xml = compact_xml(xml);
    tag_text(&xml, "Settings")
        .and_then(|settings| tag_text(settings, "Enabled"))
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("false"))
}

/// One scheduled task discovered by [`list_installed`] — F62, `--list-schedules`, closing the gap
/// `CLAUDE.md`'s F36 note had documented ("no --list-schedules — the operator can run
/// `schtasks /Query /TN <name>` directly"). Also the shape `gui_api::list_all_schedules` hands to
/// the console, replacing the plain boolean badge [`referencing_config`] gives with a real list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    /// The full `/TR` command line — showing what a schedule actually runs is the point, not just
    /// that one exists (same reasoning as `Settings.svelte` reading `pre_command`/`post_command`
    /// verbatim rather than summarizing them).
    pub command: String,
    /// "Prossima esecuzione" as `schtasks.exe` reports it, in whatever locale/format the machine
    /// uses — shown verbatim, never reparsed into a different representation.
    pub next_run: String,
    /// "Stato" — "Pronta"/"Disabilitata"/etc., same locale caveat as `next_run`.
    pub status: String,
    /// When the trigger fires, in plain Italian ("Ogni giorno alle 22:00"), read from the task's
    /// stored XML by [`describe_trigger`] rather than from the localized CSV. Empty when it could not
    /// be read.
    #[serde(default)]
    pub trigger: String,
    /// Whether the task is switched off (`<Settings><Enabled>false`), read from the stored XML: the
    /// localized `status` text is not something to compare against.
    #[serde(default)]
    pub disabled: bool,
}

/// Queries `schtasks.exe` once and returns its raw verbose CSV, or `None` when the query itself
/// fails (a locked-down policy, a permission gap) — shared by [`referencing_config`] and
/// [`list_installed`] so the one `Command` invocation and its `CREATE_NO_WINDOW` flag exist in
/// exactly one place.
#[cfg(windows)]
fn query_all_tasks_csv() -> Result<Option<String>, IngestError> {
    let mut command = std::process::Command::new("schtasks.exe");
    command.args(["/Query", "/FO", "CSV", "/V"]);
    // Called from the console's Esegui tab on every "Esamina" — without this a black console
    // window flashes in front of the GUI each time, same cause and fix as `run_schtasks` above.
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|source| IngestError::SpawnFailed {
            program: "schtasks.exe".to_string(),
            source,
        })?;
    if !output.status.success() {
        // A machine where the operator cannot query the scheduler at all should not turn this
        // advisory check into a hard failure blocking a job that has nothing to do with
        // scheduling.
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

/// Task names whose command line references `config_path` — read-only, for the console's F49-
/// adjacent "does a schedule already point at this file" badge (PIANO_GUI.md, Onda 1).
/// This never installs, removes, or otherwise acts on a schedule; F61's prohibitions on the
/// console apply here exactly as everywhere else — it can report a schedule, never touch one.
#[cfg(windows)]
pub fn referencing_config(config_path: &Path) -> Result<Vec<String>, IngestError> {
    let Some(csv) = query_all_tasks_csv()? else {
        return Ok(Vec::new());
    };
    Ok(tasks_referencing(&csv, config_path))
}

#[cfg(not(windows))]
pub fn referencing_config(_config_path: &Path) -> Result<Vec<String>, IngestError> {
    Ok(Vec::new())
}

/// The scheduled tasks whose command line references `config_path`, **with** what Task Scheduler says
/// about each (next run, status) — what the console shows beside a task that is scheduled. Same
/// query and same matching as [`referencing_config`]; read-only like it.
#[cfg(windows)]
pub fn details_for_config(config_path: &Path) -> Result<Vec<ScheduledTask>, IngestError> {
    let Some(csv) = query_all_tasks_csv()? else {
        return Ok(Vec::new());
    };
    let mut tasks = tasks_for_config(&csv, config_path);
    for task in &mut tasks {
        // One more query per task, and only for the few that run this config: the CSV does not carry
        // the trigger in a form that survives translation, the stored XML does.
        if let Some(xml) = query_task_xml(&task.name) {
            task.trigger = describe_trigger(&xml);
            task.disabled = task_is_disabled(&xml);
        }
    }
    Ok(tasks)
}

/// The stored XML of one scheduled task, or `None` when `schtasks.exe` cannot give it. The output is
/// declared UTF-16 but arrives through a pipe in the console code page; every tag read from it is
/// ASCII, so a lossy conversion loses nothing that matters.
#[cfg(windows)]
fn query_task_xml(name: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new(crate::elevation::system_program("schtasks.exe"))
        .args(["/Query", "/TN", name, "/XML"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(windows))]
pub fn details_for_config(_config_path: &Path) -> Result<Vec<ScheduledTask>, IngestError> {
    Ok(Vec::new())
}

/// Every scheduled task whose command line invokes `binary_path` — F62, unlike
/// [`referencing_config`] this does not filter by which config a task happens to reference, only
/// by which executable it runs. Read-only, same as every other function in this module that does
/// not install/uninstall.
#[cfg(windows)]
pub fn list_installed(binary_path: &Path) -> Result<Vec<ScheduledTask>, IngestError> {
    let Some(csv) = query_all_tasks_csv()? else {
        return Ok(Vec::new());
    };
    Ok(tasks_matching_binary(&csv, binary_path))
}

#[cfg(not(windows))]
pub fn list_installed(_binary_path: &Path) -> Result<Vec<ScheduledTask>, IngestError> {
    Ok(Vec::new())
}

/// The matching itself, pulled out of the public functions above so it can be tested against
/// captured real `schtasks` output without a real Task Scheduler on the machine running the test.
///
/// `#[cfg(windows)]`, like the public functions that are these two functions' only callers: on
/// another platform the stubs above never reach them, and a plain `--lib` clippy check (which does
/// not compile `#[cfg(test)]` code) would otherwise see a private function with no caller at all
/// and flag it as dead code — found exactly this way by `ubuntu-latest`'s CI job (D16).
///
/// Column *order* in `schtasks.exe`'s verbose CSV output is stable across locales even though the
/// header *labels* are localized (verified empirically on this machine, an Italian Windows
/// install) — matched by position rather than a header string that would only work in one
/// language.
#[cfg(windows)]
fn parse_scheduled_tasks(csv: &str) -> Vec<ScheduledTask> {
    csv.lines()
        .skip(1) // header row
        .filter_map(|line| {
            let fields = parse_csv_line(line);
            Some(ScheduledTask {
                name: fields.get(1)?.trim_start_matches('\\').to_string(), // "Nome attività" / "Task Name"
                next_run: fields.get(2)?.clone(), // "Prossima esecuzione" / "Next Run Time"
                status: fields.get(3)?.clone(),   // "Stato" / "Status"
                command: fields.get(8)?.clone(),  // "Attività da eseguire" / "Task To Run"
                trigger: String::new(),
                disabled: false,
            })
        })
        .collect()
}

#[cfg(windows)]
fn tasks_for_config(csv: &str, config_path: &Path) -> Vec<ScheduledTask> {
    // Paths on Windows are case-insensitive; a task installed with one casing must still match a
    // config path typed or picked with another.
    let needle = config_path.display().to_string().to_lowercase();
    parse_scheduled_tasks(csv)
        .into_iter()
        .filter(|task| task.command.to_lowercase().contains(&needle))
        .collect()
}

#[cfg(windows)]
fn tasks_referencing(csv: &str, config_path: &Path) -> Vec<String> {
    tasks_for_config(csv, config_path)
        .into_iter()
        .map(|task| task.name)
        .collect()
}

#[cfg(windows)]
fn tasks_matching_binary(csv: &str, binary_path: &Path) -> Vec<ScheduledTask> {
    let needle = binary_path.display().to_string().to_lowercase();
    parse_scheduled_tasks(csv)
        .into_iter()
        .filter(|task| task.command.to_lowercase().contains(&needle))
        .collect()
}

/// One line of `schtasks.exe /FO CSV` output: comma-separated, every field quoted, an embedded
/// quote doubled (`""`). Hand-rolled rather than a dependency for one read path — the console's
/// own `csv.js` makes the same call for the same reason.
///
/// Deliberately lenient rather than a strict RFC 4180 implementation: a command line that itself
/// contains a quoted sub-path (`""C:\Program Files\Git\git-bash.exe" --hide ...`, a real value
/// captured from this machine) does not round-trip through strict quote-doubling rules, and this
/// function's only job is to get the *field boundaries* right so a substring search over the
/// command field works — not to reproduce the original quoting exactly.
#[cfg(windows)]
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => fields.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the console shows in "Quando": read back from the XML this module itself writes, for each
    /// shape, so the description cannot drift from what was registered.
    mod trigger_text {
        use super::*;

        fn start() -> chrono::NaiveDateTime {
            chrono::NaiveDate::from_ymd_opt(2026, 10, 14)
                .expect("date")
                .and_hms_opt(22, 0, 0)
                .expect("time")
        }

        fn xml_for(spec: &str) -> String {
            build_task_xml(
                &parse_schedule_spec(spec).expect("spec"),
                "C:/x/robocopy_ingest.exe",
                "--config C:/x/a.toml",
                start(),
            )
        }

        #[test]
        fn each_shape_we_register_reads_back_in_plain_italian() {
            assert_eq!(
                describe_trigger(&xml_for("daily@22:00")),
                "Ogni giorno alle 22:00"
            );
            assert_eq!(describe_trigger(&xml_for("hourly@6")), "Ogni 6 ore");
            assert_eq!(
                describe_trigger(&xml_for("weekly@MON,WED,FRI@07:30")),
                "Lun, Mer, Ven alle 22:00"
            );
        }

        /// Windows rewrites a registered task with indentation and line breaks between the tags; the
        /// description must not depend on our compact form.
        #[test]
        fn windows_own_indented_rewrite_reads_the_same() {
            let rewritten = [
                "<?xml version=\"1.0\" encoding=\"UTF-16\"?>",
                "<Task>",
                "  <Triggers>",
                "    <CalendarTrigger>",
                "      <StartBoundary>2026-10-14T06:15:00</StartBoundary>",
                "      <Enabled>true</Enabled>",
                "      <ScheduleByDay>",
                "        <DaysInterval>2</DaysInterval>",
                "      </ScheduleByDay>",
                "    </CalendarTrigger>",
                "  </Triggers>",
                "</Task>",
            ]
            .join("\r\n");
            assert_eq!(describe_trigger(&rewritten), "Ogni 2 giorni alle 06:15");
        }

        #[test]
        fn other_triggers_have_a_name_and_none_is_empty() {
            assert_eq!(
                describe_trigger("<Task><Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers></Task>"),
                "All'accesso"
            );
            assert_eq!(
                describe_trigger("<Task><Triggers><BootTrigger/></Triggers></Task>"),
                "Pianificata",
                "an unknown shape is still said to be scheduled"
            );
            assert_eq!(describe_trigger("<Task></Task>"), "");
            assert_eq!(describe_trigger(""), "");
        }

        /// A task switched off is told apart from the trigger's own `<Enabled>`.
        #[test]
        fn a_disabled_task_is_read_from_its_settings_not_its_trigger() {
            let on = xml_for("daily@22:00");
            assert!(!task_is_disabled(&on));
            let off = on.replace(
                "<Enabled>true</Enabled><ExecutionTimeLimit>",
                "<Enabled>false</Enabled><ExecutionTimeLimit>",
            );
            assert_ne!(on, off, "the fixture must actually change the setting");
            assert!(task_is_disabled(&off));
            // A disabled trigger inside an enabled task is not a disabled task.
            let trigger_only =
                on.replacen("<Enabled>true</Enabled>", "<Enabled>false</Enabled>", 1);
            assert!(!task_is_disabled(&trigger_only));
        }
    }

    // `tasks_referencing`/`parse_csv_line` are `#[cfg(windows)]` (see their doc comments), so the
    // tests exercising them have to be too — they'd otherwise fail to compile on a non-Windows CI
    // runner, referencing functions that do not exist there.
    #[cfg(windows)]
    mod schedule_matching {
        use super::*;

        /// Captured directly from `schtasks.exe /Query /FO CSV /V` on this machine (an Italian
        /// Windows install). The command field is real and deliberately messy: a quoted sub-path
        /// immediately followed by unquoted flags, which does not round-trip through strict
        /// RFC 4180 quote-doubling rules — exactly why `parse_csv_line` is lenient rather than
        /// strict.
        const REAL_CAPTURED_ROW: &str = r#""WKAI01","\Git for Windows Updater","04/09/2026 13:59:53","Pronta","Solo interattivo","03/09/2026 13:59:54","1","N/D",""C:\Program Files\Git\git-bash.exe" --hide --no-needs-console --command=cmd\git.exe update-git-for-windows --quiet --gui","N/D","N/D","Abilitata","Disabilitata","Interrompi in modalità di alimentazione a batterie, Non avviare se il sistema è alimentato a batterie","auresystem","Disabilitata","72:00:00","Dati di pianificazione non disponibili in questo formato.","Ogni giorno ","13:59:53","18/11/2025","N/D","Ogni 1 giorni","N/D","Disabilitata","Disabilitata","Disabilitata","Disabilitata""#;

        fn csv_with(rows: &[&str]) -> String {
            let mut csv = "\"header row, ignored by tasks_referencing\"\n".to_string();
            for row in rows {
                csv.push_str(row);
                csv.push('\n');
            }
            csv
        }

        #[test]
        fn the_real_capture_splits_the_task_name_correctly() {
            let fields = parse_csv_line(REAL_CAPTURED_ROW);
            assert_eq!(fields[1], "\\Git for Windows Updater");
        }

        #[test]
        fn the_messy_quoted_command_field_still_yields_a_searchable_path() {
            let fields = parse_csv_line(REAL_CAPTURED_ROW);
            assert!(fields[8].contains(r"C:\Program Files\Git\git-bash.exe"));
        }

        #[test]
        fn a_task_is_reported_when_its_command_contains_the_config_path() {
            let csv = csv_with(&[
                r#""WKAI01","\rustcopy-nightly","N/D","Pronta","N/D","N/D","0","N/D","C:\rustcopy.exe --config C:\jobs\nightly.toml""#,
                REAL_CAPTURED_ROW,
            ]);
            let found = tasks_referencing(&csv, Path::new(r"C:\jobs\nightly.toml"));
            assert_eq!(found, vec!["rustcopy-nightly".to_string()]);
        }

        /// Windows paths are case-insensitive; a schedule installed with one casing must still be
        /// found against a config path typed or picked with another.
        #[test]
        fn matching_is_case_insensitive_like_windows_paths() {
            let csv = csv_with(&[
                r#""WKAI01","\job","N/D","N/D","N/D","N/D","N/D","N/D","C:\Rustcopy.exe --config C:\Jobs\Nightly.toml""#,
            ]);
            let found = tasks_referencing(&csv, Path::new(r"c:\jobs\nightly.toml"));
            assert_eq!(found, vec!["job".to_string()]);
        }

        #[test]
        fn the_details_carry_what_task_scheduler_says_about_the_next_run() {
            let csv = csv_with(&[
                r#""WKAI01","\rustcopy-nightly","04/09/2026 02:00:00","Pronta","N/D","N/D","0","N/D","C:\rustcopy.exe --config C:\jobs\nightly.toml""#,
            ]);
            let found = tasks_for_config(&csv, Path::new(r"C:\jobs\nightly.toml"));
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].next_run, "04/09/2026 02:00:00");
            assert_eq!(found[0].status, "Pronta");
        }

        #[test]
        fn no_task_references_an_unrelated_config() {
            let csv = csv_with(&[REAL_CAPTURED_ROW]);
            let found = tasks_referencing(&csv, Path::new(r"C:\jobs\nightly.toml"));
            assert!(found.is_empty());
        }

        // F62: `--list-schedules` filters by the binary's own path rather than a specific config,
        // so it must find a task even when the two jobs it schedules use different config files —
        // exactly the case `tasks_referencing` above would need two separate queries for.
        #[test]
        fn matching_by_binary_finds_every_task_regardless_of_which_config_it_targets() {
            let csv = csv_with(&[
                r#""WKAI01","\rustcopy-nightly","04/09/2026 02:00:00","Pronta","N/D","N/D","0","N/D","C:\rustcopy.exe --config C:\jobs\nightly.toml""#,
                r#""WKAI01","\rustcopy-weekly","05/09/2026 03:00:00","Pronta","N/D","N/D","0","N/D","C:\rustcopy.exe --config C:\jobs\weekly.toml""#,
                REAL_CAPTURED_ROW,
            ]);
            let found = tasks_matching_binary(&csv, Path::new(r"C:\rustcopy.exe"));
            assert_eq!(found.len(), 2);
            assert_eq!(found[0].name, "rustcopy-nightly");
            assert_eq!(found[0].next_run, "04/09/2026 02:00:00");
            assert_eq!(found[0].status, "Pronta");
            assert_eq!(found[1].name, "rustcopy-weekly");
        }

        #[test]
        fn matching_by_binary_is_case_insensitive_like_windows_paths() {
            let csv = csv_with(&[
                r#""WKAI01","\job","N/D","N/D","N/D","N/D","N/D","N/D","C:\Rustcopy.exe --config C:\Jobs\Nightly.toml""#,
            ]);
            let found = tasks_matching_binary(&csv, Path::new(r"c:\rustcopy.exe"));
            assert_eq!(found.len(), 1);
        }

        #[test]
        fn matching_by_binary_ignores_a_task_that_invokes_a_different_program() {
            let csv = csv_with(&[REAL_CAPTURED_ROW]);
            let found = tasks_matching_binary(&csv, Path::new(r"C:\rustcopy.exe"));
            assert!(found.is_empty());
        }

        #[test]
        fn parse_scheduled_tasks_extracts_all_four_fields() {
            let csv = csv_with(&[REAL_CAPTURED_ROW]);
            let tasks = parse_scheduled_tasks(&csv);
            assert_eq!(tasks.len(), 1);
            assert_eq!(tasks[0].name, "Git for Windows Updater");
            assert_eq!(tasks[0].next_run, "04/09/2026 13:59:53");
            assert_eq!(tasks[0].status, "Pronta");
            assert!(tasks[0]
                .command
                .contains(r"C:\Program Files\Git\git-bash.exe"));
        }
    }

    #[test]
    fn parses_a_daily_spec() {
        assert_eq!(
            parse_schedule_spec("daily@02:30").unwrap(),
            ScheduleSpec::Daily {
                time: "02:30".to_string()
            }
        );
    }

    #[test]
    fn parses_and_zero_pads_a_daily_spec() {
        assert_eq!(
            parse_schedule_spec("daily@2:5").unwrap(),
            ScheduleSpec::Daily {
                time: "02:05".to_string()
            }
        );
    }

    #[test]
    fn parses_an_hourly_spec() {
        assert_eq!(
            parse_schedule_spec("hourly@4").unwrap(),
            ScheduleSpec::Hourly { every_n_hours: 4 }
        );
    }

    #[test]
    fn rejects_hourly_zero() {
        assert!(parse_schedule_spec("hourly@0").is_err());
    }

    #[test]
    fn parses_a_weekly_spec_and_uppercases_days() {
        assert_eq!(
            parse_schedule_spec("weekly@mon,wed,fri@03:00").unwrap(),
            ScheduleSpec::Weekly {
                days: vec!["MON".to_string(), "WED".to_string(), "FRI".to_string()],
                time: "03:00".to_string(),
            }
        );
    }

    #[test]
    fn rejects_an_unknown_weekday_code() {
        assert!(parse_schedule_spec("weekly@funday@03:00").is_err());
    }

    #[test]
    fn rejects_an_out_of_range_time() {
        assert!(parse_schedule_spec("daily@24:00").is_err());
        assert!(parse_schedule_spec("daily@10:60").is_err());
    }

    #[test]
    fn rejects_an_unknown_kind() {
        assert!(parse_schedule_spec("monthly@1@02:00").is_err());
    }

    #[test]
    fn rejects_a_spec_without_an_at_separator() {
        assert!(parse_schedule_spec("daily").is_err());
    }

    fn at(date: &str, time: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M:%S")
            .expect("valid test instant")
    }

    #[test]
    fn create_args_register_the_xml_file_not_trigger_flags() {
        assert_eq!(
            build_create_args("rustcopy-photos", "C:\\Temp\\task.xml"),
            vec![
                "/Create",
                "/TN",
                "rustcopy-photos",
                "/XML",
                "C:\\Temp\\task.xml",
                "/F"
            ]
        );
    }

    #[test]
    fn the_first_start_is_always_in_the_future() {
        let now = at("2026-10-09", "10:00:00"); // a Friday
        let daily = |t: &str| ScheduleSpec::Daily { time: t.into() };
        assert_eq!(
            first_start(&daily("11:30"), now),
            at("2026-10-09", "11:30:00")
        );
        assert_eq!(
            first_start(&daily("09:00"), now),
            at("2026-10-10", "09:00:00"),
            "today's 09:00 has passed: tomorrow, never the past (StartWhenAvailable would fire it at once)"
        );
        assert_eq!(
            first_start(&daily("10:00"), now),
            at("2026-10-10", "10:00:00"),
            "exactly now is not the future"
        );
    }

    #[test]
    fn a_weekly_start_is_the_next_listed_day_at_that_time() {
        let now = at("2026-10-09", "10:00:00"); // Friday
        let weekly = |days: &[&str], t: &str| ScheduleSpec::Weekly {
            days: days.iter().map(|d| d.to_string()).collect(),
            time: t.into(),
        };
        assert_eq!(
            first_start(&weekly(&["MON", "FRI"], "23:00"), now),
            at("2026-10-09", "23:00:00"),
            "today is a listed day and the time is ahead"
        );
        assert_eq!(
            first_start(&weekly(&["MON", "FRI"], "09:00"), now),
            at("2026-10-12", "09:00:00"),
            "Friday 09:00 has passed: next Monday"
        );
        assert_eq!(
            first_start(&weekly(&["FRI"], "09:00"), now),
            at("2026-10-16", "09:00:00"),
            "the same weekday next week"
        );
    }

    #[test]
    fn an_hourly_start_is_the_next_whole_minute() {
        let now = at("2026-10-09", "10:00:42");
        assert_eq!(
            first_start(&ScheduleSpec::Hourly { every_n_hours: 4 }, now),
            at("2026-10-09", "10:01:00")
        );
    }

    fn daily_xml() -> String {
        build_task_xml(
            &ScheduleSpec::Daily {
                time: "02:00".into(),
            },
            r"C:\Program Files\rustcopy\robocopy_ingest.exe",
            r#"--config "C:\jobs\a & b.toml""#,
            at("2026-10-10", "02:00:00"),
        )
    }

    #[test]
    fn the_task_xml_lets_a_laptop_back_up_and_catches_up_a_missed_run() {
        let xml = daily_xml();
        for wanted in [
            "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
            "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>",
            "<StartWhenAvailable>true</StartWhenAvailable>",
            "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
            "<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>",
            "<StartBoundary>2026-10-10T02:00:00</StartBoundary>",
            "<ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>",
        ] {
            assert!(xml.contains(wanted), "missing {wanted} in {xml}");
        }
    }

    #[test]
    fn the_command_and_its_arguments_are_kept_apart_and_escaped() {
        let xml = daily_xml();
        assert!(xml.contains("<Command>C:\\Program Files\\rustcopy\\robocopy_ingest.exe</Command>"));
        assert!(
            xml.contains("<Arguments>--config &quot;C:\\jobs\\a &amp; b.toml&quot;</Arguments>"),
            "{xml}"
        );
    }

    #[test]
    fn hourly_and_weekly_triggers_have_their_own_shape() {
        let start = at("2026-10-10", "02:00:00");
        let hourly = build_task_xml(
            &ScheduleSpec::Hourly { every_n_hours: 6 },
            "x.exe",
            "",
            start,
        );
        assert!(hourly.contains("<TimeTrigger>") && hourly.contains("<Interval>PT6H</Interval>"));
        let weekly = build_task_xml(
            &ScheduleSpec::Weekly {
                days: vec!["MON".into(), "SAT".into()],
                time: "03:00".into(),
            },
            "x.exe",
            "",
            start,
        );
        assert!(weekly.contains("<DaysOfWeek><Monday/><Saturday/></DaysOfWeek>"));
    }

    #[test]
    fn the_xml_is_written_as_utf16_with_a_byte_order_mark() {
        let bytes = task_xml_bytes("<a>é</a>");
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "<a>é</a>");
    }

    #[test]
    fn build_delete_args_targets_the_named_task() {
        assert_eq!(
            build_delete_args("rustcopy-photos"),
            vec!["/Delete", "/TN", "rustcopy-photos", "/F"]
        );
    }

    #[test]
    fn strip_schedule_flags_removes_flag_and_value_pairs() {
        let args: Vec<String> = [
            "--source",
            "C:\\a",
            "--dest",
            "C:\\b",
            "--install-schedule",
            "daily@02:00",
            "--schedule-name",
            "myjob",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let stripped = strip_schedule_flags(&args);
        assert_eq!(stripped, vec!["--source", "C:\\a", "--dest", "C:\\b"]);
    }

    #[test]
    fn strip_schedule_flags_handles_equals_form() {
        let args: Vec<String> = ["--source", "C:\\a", "--install-schedule=daily@02:00"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let stripped = strip_schedule_flags(&args);
        assert_eq!(stripped, vec!["--source", "C:\\a"]);
    }

    #[test]
    fn build_task_run_command_quotes_paths_with_spaces() {
        let exe = Path::new("C:\\Program Files\\rustcopy.exe");
        let args = vec!["--source".to_string(), "C:\\my data".to_string()];
        let command = build_task_run_command(exe, &args);
        assert_eq!(
            command,
            "\"C:\\Program Files\\rustcopy.exe\" --source \"C:\\my data\""
        );
    }

    #[test]
    fn build_task_run_command_leaves_simple_args_unquoted() {
        let exe = Path::new("C:\\rustcopy.exe");
        let args = vec!["--source".to_string(), "C:\\data".to_string()];
        let command = build_task_run_command(exe, &args);
        assert_eq!(command, "C:\\rustcopy.exe --source C:\\data");
    }
    /// The boundary is not a guess: `schtasks.exe /SC HOURLY /MO 23` is accepted and `/MO 24` is
    /// refused with "valore non valido per l'opzione /MO", checked against the real binary. Before
    /// this, an out-of-range interval parsed fine here and failed only at install time, with
    /// schtasks' message rather than one naming the flag.
    #[test]
    fn hourly_accepts_the_schtasks_range_and_rejects_what_it_would_refuse() {
        for n in [1u32, 12, 23] {
            assert!(
                parse_schedule_spec(&format!("hourly@{n}")).is_ok(),
                "hourly@{n} is inside the range schtasks accepts"
            );
        }
        for n in [0u32, 24, 25, 100] {
            let error = parse_schedule_spec(&format!("hourly@{n}"))
                .expect_err("outside the range schtasks accepts");
            assert!(
                format!("{error}").contains("hourly") || format!("{error}").contains("spec"),
                "the message must point at the spec, got: {error}"
            );
        }
    }
    mod elevated {
        use super::*;
        use crate::safety::SafetyLevel;

        fn config(dir: &Path, body: &str) -> std::path::PathBuf {
            let path = dir.join("job.toml");
            std::fs::write(&path, body).expect("write");
            path
        }

        const PLAIN: &str = "source = 'C:/a'\ndest = 'D:/b'\n";

        fn start() -> chrono::NaiveDateTime {
            chrono::NaiveDateTime::parse_from_str("2026-10-12 02:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("instant")
        }

        #[test]
        fn an_elevated_task_asks_for_the_highest_rights_and_starts_in_its_own_folder() {
            let spec = ScheduleSpec::Daily {
                time: "02:00".into(),
            };
            let xml = build_task_xml_with(
                &spec,
                "C:\\r\\robocopy_ingest.exe",
                "--config x --vss-snapshot",
                start(),
                &TaskOptions {
                    elevated: true,
                    working_dir: Some("C:\\ProgramData\\rustcopy\\elevated\\notte".into()),
                },
            );
            assert!(
                xml.contains("<RunLevel>HighestAvailable</RunLevel>"),
                "{xml}"
            );
            assert!(
                xml.contains("<WorkingDirectory>C:\\ProgramData\\rustcopy\\elevated\\notte</WorkingDirectory>"),
                "{xml}"
            );
            // Still only while the owner is logged on: no password is involved.
            assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        }

        #[test]
        fn an_ordinary_task_is_unchanged() {
            let spec = ScheduleSpec::Daily {
                time: "02:00".into(),
            };
            let xml = build_task_xml(&spec, "x.exe", "", start());
            assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
            assert!(!xml.contains("WorkingDirectory"));
        }

        #[test]
        fn the_elevated_switch_never_reaches_a_scheduled_command() {
            let args: Vec<String> = [
                "--config",
                "a.toml",
                "--install-schedule",
                "daily@02:00",
                "--schedule-name",
                "x",
                "--schedule-elevated",
                "--verify-integrity",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            assert_eq!(
                strip_schedule_flags(&args),
                vec!["--config", "a.toml", "--verify-integrity"]
            );
        }

        #[test]
        fn only_the_highest_level_may_prepare_or_remove_an_elevated_schedule() {
            let dir = tempfile::tempdir().expect("tempdir");
            let file = config(dir.path(), PLAIN);
            for level in [SafetyLevel::Prudent, SafetyLevel::Standard] {
                assert!(elevated_install_arguments(level, &file, "daily@02:00", "notte").is_err());
                assert!(elevated_removal_arguments(level, "notte").is_err());
            }
            assert!(
                elevated_install_arguments(SafetyLevel::Expert, &file, "daily@02:00", "notte")
                    .is_ok()
            );
            assert!(elevated_removal_arguments(SafetyLevel::Expert, "notte").is_ok());
        }

        /// The F61 prohibitions as a test: the argument list that goes through the elevation prompt has a
        /// fixed shape and no destructive flag.
        #[test]
        fn the_elevated_install_arguments_have_a_fixed_shape_and_no_destructive_flag() {
            let dir = tempfile::tempdir().expect("tempdir");
            let file = config(dir.path(), PLAIN);
            let args =
                elevated_install_arguments(SafetyLevel::Expert, &file, "daily@02:00", "notte")
                    .expect("allowed");
            assert_eq!(args.len(), 7);
            assert_eq!(args[0], "--config");
            assert_eq!(args[2], "--install-schedule");
            assert_eq!(args.last().map(String::as_str), Some("--schedule-elevated"));
            let joined = args.join(" ");
            for forbidden in [
                "--force-purge",
                "--mirror",
                "--keep-generations",
                "--pre-command",
                "--post-command",
            ] {
                assert!(!joined.contains(forbidden), "{forbidden} in {joined}");
            }
            let removal =
                elevated_removal_arguments(SafetyLevel::Expert, "notte").expect("allowed");
            assert_eq!(
                removal,
                vec!["--uninstall-schedule", "notte", "--schedule-elevated"]
            );
        }

        #[test]
        fn an_elevated_schedule_refuses_whatever_deletes_or_runs_a_command() {
            let dir = tempfile::tempdir().expect("tempdir");
            let cases = [
                ("mirror = true\n", "mirror"),
                (
                    "keep_generations = 3\nbackup_type = 'full'\n",
                    "generazioni",
                ),
                ("pre_command = 'net stop x'\n", "comando"),
                ("post_command = 'net start x'\n", "comando"),
                ("report_path = 'C:/Users/x/r.json'\n", "report"),
                ("log_path = 'C:/Users/x/l.log'\n", "report"),
                ("html_report_path = 'C:/Users/x/r.html'\n", "report"),
            ];
            for (extra, word) in cases {
                let file = config(dir.path(), &format!("{PLAIN}{extra}"));
                let error = validate_for_elevation(&file).expect_err(extra).to_string();
                assert!(error.contains(word), "{extra}: {error}");
            }
            let relative = config(dir.path(), "source = 'a'\ndest = 'D:/b'\n");
            assert!(
                validate_for_elevation(&relative).is_err(),
                "a relative path"
            );
            let fine = config(dir.path(), PLAIN);
            assert!(validate_for_elevation(&fine).is_ok());
        }

        /// What is validated is the text that was read, not the file: swapping the file afterwards cannot
        /// make a configuration that was not checked run elevated.
        #[test]
        fn validation_of_text_does_not_look_at_the_file_again() {
            let dir = tempfile::tempdir().expect("tempdir");
            let origin = config(dir.path(), "mirror = true\nsource='C:/a'\ndest='D:/b'\n");
            // The path now holds a harmless file, but the text that was read is the dangerous one.
            std::fs::write(&origin, PLAIN).expect("swap");
            assert!(validate_elevation_text(
                "mirror = true\nsource='C:/a'\ndest='D:/b'\n",
                &origin
            )
            .is_err());
            assert!(validate_elevation_text(PLAIN, &origin).is_ok());
        }

        #[test]
        fn a_command_in_any_job_of_a_batch_is_refused() {
            let dir = tempfile::tempdir().expect("tempdir");
            let file = config(
                dir.path(),
                "[[jobs]]\nname='a'\nsource='C:/a'\ndest='D:/a'\n\n[[jobs]]\nname='b'\nsource='C:/b'\ndest='D:/b'\npost_command='x'\n",
            );
            let error = validate_for_elevation(&file)
                .expect_err("job b")
                .to_string();
            assert!(error.contains("«b»"), "{error}");
        }

        #[test]
        fn a_task_running_an_elevated_copy_is_recognised_whatever_the_case() {
            assert!(is_elevated_command(
                "\"C:\\x\\robocopy_ingest.exe\" --config C:\\ProgramData\\rustcopy\\elevated\\notte\\config.toml --vss-snapshot"
            ));
            assert!(is_elevated_command(
                "c:\\programdata\\RUSTCOPY\\Elevated\\n\\config.toml"
            ));
            assert!(!is_elevated_command(
                "C:\\x\\robocopy_ingest.exe --config C:\\jobs\\a.toml"
            ));
        }

        #[test]
        fn the_scheduled_command_runs_the_copy_with_the_shadow_copy_on() {
            let dir = Path::new("C:/ProgramData/rustcopy/elevated/n");
            let copy = dir.join("config.toml");
            let args = elevated_arguments(&copy, dir);
            assert_eq!(args[0], "--config");
            assert_eq!(args[1], copy.display().to_string());
            assert_eq!(args[2], "--vss-snapshot");
            // Report and log inside the protected folder, not wherever the configuration said.
            assert!(args[4].starts_with(&dir.display().to_string()), "{args:?}");
            assert!(args[6].starts_with(&dir.display().to_string()), "{args:?}");
            // Never writes through a link left in the destination, with administrator rights.
            assert!(
                args.contains(&"--refuse-dest-links".to_string()),
                "{args:?}"
            );
        }
    }

    mod console_requests {
        use super::*;
        use crate::safety::SafetyLevel;

        fn config(dir: &Path, body: &str) -> std::path::PathBuf {
            let path = dir.join("job.toml");
            std::fs::write(&path, body).expect("write");
            path
        }

        const PLAIN: &str = "source = 'C:/a'\ndest = 'D:/b'\n";

        #[test]
        fn a_level_below_standard_cannot_prepare_or_remove_a_schedule() {
            let dir = tempfile::tempdir().expect("tempdir");
            let file = config(dir.path(), PLAIN);
            let refused = install_arguments(SafetyLevel::Prudent, &file, "daily@02:00", "copia");
            assert!(refused.is_err());
            assert!(removal_arguments(SafetyLevel::Prudent, "copia").is_err());
            assert!(
                install_arguments(SafetyLevel::Standard, &file, "daily@02:00", "copia").is_ok()
            );
            assert!(removal_arguments(SafetyLevel::Expert, "copia").is_ok());
        }

        #[test]
        fn the_argument_list_has_a_fixed_shape_with_no_destructive_flag() {
            let dir = tempfile::tempdir().expect("tempdir");
            let file = config(dir.path(), PLAIN);
            let args = install_arguments(
                SafetyLevel::Standard,
                &file,
                "weekly@MON,FRI@03:30",
                "copia notturna",
            )
            .expect("plain job");
            assert_eq!(args.len(), 6);
            assert_eq!(args[0], "--config");
            assert_eq!(args[2], "--install-schedule");
            assert_eq!(args[3], "weekly@MON,FRI@03:30");
            assert_eq!(args[4], "--schedule-name");
            assert_eq!(args[5], "copia notturna");
            let joined = args.join(" ");
            for forbidden in [
                "--force-purge",
                "--mirror",
                "--install-service",
                "--keep-generations",
                "--decrypt",
            ] {
                assert!(!joined.contains(forbidden), "{forbidden}: {joined}");
            }
            assert_eq!(
                removal_arguments(SafetyLevel::Standard, "copia").expect("ok"),
                vec!["--uninstall-schedule", "copia"]
            );
        }

        #[test]
        fn a_job_that_mirrors_or_prunes_is_never_scheduled_from_the_console() {
            let dir = tempfile::tempdir().expect("tempdir");
            for risky in [
                "source = 'C:/a'\ndest = 'D:/b'\nmirror = true\n",
                "source = 'C:/a'\ndest = 'D:/b'\nbackup_type = 'full'\nkeep_generations = 3\n",
            ] {
                let file = config(dir.path(), risky);
                let refused = install_arguments(SafetyLevel::Expert, &file, "daily@02:00", "copia");
                assert!(refused.is_err(), "{risky}");
            }
        }

        #[test]
        fn relative_paths_and_bad_names_and_bad_specs_are_refused() {
            let dir = tempfile::tempdir().expect("tempdir");
            let relative = config(dir.path(), "source = 'src'\ndest = 'D:/b'\n");
            assert!(
                install_arguments(SafetyLevel::Standard, &relative, "daily@02:00", "copia")
                    .is_err()
            );
            let file = config(dir.path(), PLAIN);
            for name in ["", " x", "a\\b", "x\"y", &"n".repeat(65)] {
                assert!(
                    install_arguments(SafetyLevel::Standard, &file, "daily@02:00", name).is_err(),
                    "{name:?}"
                );
            }
            assert!(install_arguments(SafetyLevel::Standard, &file, "every-day", "copia").is_err());
            assert!(install_arguments(
                SafetyLevel::Standard,
                &dir.path().join("missing.toml"),
                "daily@02:00",
                "copia"
            )
            .is_err());
        }
    }
}
