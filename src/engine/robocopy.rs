//! Robocopy-backed copy engine.
//!
//! Flag construction and output parsing are pure functions, and process execution goes through the
//! [`CommandRunner`] trait, so everything except the actual `robocopy.exe` spawn is unit-tested on
//! any platform. Flag semantics follow the Microsoft Learn robocopy reference:
//! <https://learn.microsoft.com/windows-server/administration/windows-commands/robocopy>

use std::time::Instant;

use crate::errors::IngestError;
use crate::progress::ProgressSink;

use super::{CopyEngine, CopyOutcome, CopyRequest};

pub const ENGINE_NAME: &str = "robocopy";

/// Program name; on Windows this resolves through `%PATH%` to `%SystemRoot%\System32\robocopy.exe`.
pub const PROGRAM: &str = "robocopy.exe";

/// Executes a program and streams its stdout line by line.
pub trait CommandRunner: Send + Sync {
    /// Run `program` with `args`, invoking `on_line` for every stdout line, and return the exit code.
    fn run(
        &self,
        program: &str,
        args: &[String],
        on_line: &mut dyn FnMut(&str),
    ) -> Result<i32, IngestError>;
}

/// Build the robocopy command line for `request`.
///
/// Flags used (all documented in the Microsoft Learn reference above):
/// * `/E` copy subdirectories, including empty ones;
/// * `/MT:n` multithreaded copying with n threads;
/// * `/R:n` retries per failed file, `/W:n` wait between those retries;
/// * `/BYTES` print sizes as plain byte counts, which is what makes throughput parsing possible;
/// * `/NP` suppress the per-file percentage so each file yields exactly one parseable line;
/// * `/L` list only, used for `--dry-run`.
/// * `/MIR` mirror mode (F4.3): syncs destination to source, deleting extra files.
/// * `/XF` exclude files matching patterns (F4.1).
/// * `/XD` exclude directories matching patterns (F4.1).
/// * `/MINAGE`/`/MAXAGE` date-based filtering (F4.2).
/// * `/IPG` inter-packet gap for bandwidth throttling (F4.5).
///
/// F1.2: paths are normalised to strip trailing separators before use, preventing Windows
/// argument-quoting bugs where a trailing `\` escapes the closing `"`.
pub fn build_args(request: &CopyRequest) -> Vec<String> {
    // F1.2: strip trailing path separators so `"C:\data\"` doesn't become `"C:\data\"` in
    // the shell, which would escape the closing quote.
    let source = normalize_path_arg(&request.source.to_string_lossy(), request.long_paths);
    let dest = normalize_path_arg(&request.dest.to_string_lossy(), request.long_paths);

    let mut args = vec![
        source,
        dest,
        request.pattern.clone(),
        "/E".to_string(),
        format!("/MT:{}", request.threads),
        format!("/R:{}", request.file_retries),
        format!("/W:{}", request.retry_wait_seconds),
        "/BYTES".to_string(),
        "/NP".to_string(),
    ];

    // F6.2: preserve directory timestamps.
    if request.preserve_timestamps {
        args.push("/DCOPY:DAT".to_string());
    }

    // F6.2: preserve NTFS ACL permissions.
    if request.preserve_acl {
        args.push("/COPYALL".to_string());
    }

    // F4.3: mirror mode.
    if request.mirror {
        // /MIR implies /E and /PURGE; keep /E explicit for clarity.
        args.push("/MIR".to_string());
    }

    // F4.1: file exclusion patterns.
    for pat in &request.exclude_files {
        args.push("/XF".to_string());
        args.push(pat.clone());
    }

    // F4.1: directory exclusion patterns.
    for pat in &request.exclude_dirs {
        args.push("/XD".to_string());
        args.push(pat.clone());
    }

    // F4.2: minimum/maximum file age in days.
    if let Some(min_age) = request.min_age_days {
        args.push(format!("/MINAGE:{min_age}"));
    }
    if let Some(max_age) = request.max_age_days {
        args.push(format!("/MAXAGE:{max_age}"));
    }

    // F4.5: bandwidth throttling (inter-packet gap in milliseconds).
    if let Some(ipg) = request.inter_packet_gap_ms {
        args.push(format!("/IPG:{ipg}"));
    }

    if request.dry_run {
        args.push("/L".to_string());
    }
    args
}

/// F1.2: Strip trailing path separators to prevent Windows argument-quoting bugs.
///
/// A path like `C:\data\` would produce `"C:\data\"` in a quoted shell argument, where the
/// `\"` is parsed as an escaped quote rather than a closing quote, merging the path with the
/// following arguments. Stripping the trailing separator avoids this entirely.
pub fn normalize_path_arg(path: &str, long_paths: bool) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    if long_paths && cfg!(windows) && !trimmed.starts_with(r"\\?\") && trimmed.len() > 240 {
        format!(r"\\?\{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Render a command line for logs and reports, quoting fragments that contain spaces.
pub fn command_line(program: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(quote(program));
    parts.extend(args.iter().map(|a| quote(a)));
    parts.join(" ")
}

fn quote(value: &str) -> String {
    if value.contains(' ') && !value.starts_with('"') {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

/// Statuses that mark a line as "this file was transferred".
const COPIED_STATUSES: [&str; 6] = [
    "New File", "Newer", "Older", "Changed", "Tweaked", "Modified",
];

/// Statuses whose lines describe something that was *not* transferred.
const IGNORED_STATUSES: [&str; 6] = [
    "EXTRA File",
    "EXTRA Dir",
    "New Dir",
    "Same",
    "same",
    "Mismatch",
];

/// Extract the byte count of a transferred file from one robocopy output line.
///
/// With `/BYTES` and `/NP` a copied file is rendered as `<status>\t<bytes>\t<name>` (columns are
/// tab separated and padded). Header, summary, directory and "extra file" lines are rejected.
/// Status keywords are only looked for in the leading columns, so a file *named* `Same.csv` is
/// still counted.
pub fn parse_file_bytes(line: &str) -> Option<u64> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.contains('%') || is_labelled_line(trimmed) {
        return None;
    }

    let fields = split_fields(trimmed);
    let status: String = fields
        .iter()
        .take_while(|field| parse_byte_count(field).is_none())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");

    if IGNORED_STATUSES.iter().any(|s| status.contains(s)) {
        return None;
    }
    let has_copy_status = COPIED_STATUSES.iter().any(|s| status.contains(s));

    for (index, field) in fields.iter().enumerate() {
        if let Some(bytes) = parse_byte_count(field) {
            let followed_by_name = fields[index + 1..]
                .iter()
                .any(|f| parse_byte_count(f).is_none());
            if has_copy_status || followed_by_name {
                return Some(bytes);
            }
        }
    }
    None
}

/// Byte counts from robocopy's `Bytes :` summary row (requires `/BYTES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SummaryRow {
    pub total: u64,
    pub copied: u64,
    pub skipped: u64,
    pub mismatch: u64,
    pub failed: u64,
    pub extras: u64,
}

/// Parse a robocopy summary row such as `Bytes :  1024  1024  0  0  0  0`.
///
/// `label` is `"Bytes"` or `"Files"`. Returns `None` for any other line.
pub fn parse_summary_row(line: &str, label: &str) -> Option<SummaryRow> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix(label)?
        .trim_start()
        .strip_prefix(':')?;
    let numbers: Vec<u64> = rest
        .split_whitespace()
        .map(parse_byte_count)
        .collect::<Option<Vec<u64>>>()?;
    if numbers.len() < 2 {
        return None;
    }
    Some(SummaryRow {
        total: numbers[0],
        copied: numbers[1],
        skipped: numbers.get(2).copied().unwrap_or_default(),
        mismatch: numbers.get(3).copied().unwrap_or_default(),
        failed: numbers.get(4).copied().unwrap_or_default(),
        extras: numbers.get(5).copied().unwrap_or_default(),
    })
}

fn is_labelled_line(trimmed: &str) -> bool {
    // Summary/header rows look like `Bytes :`, `Options :`, `Started :`. Windows paths such as
    // `C:\data\x.csv` must not match, hence the requirement of whitespace before the colon.
    trimmed
        .split_once(':')
        .is_some_and(|(head, _)| !head.is_empty() && head.ends_with(char::is_whitespace))
}

fn split_fields(trimmed: &str) -> Vec<&str> {
    let tabbed: Vec<&str> = trimmed
        .split('\t')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .collect();
    if tabbed.len() >= 2 {
        tabbed
    } else {
        trimmed.split_whitespace().collect()
    }
}

fn parse_byte_count(field: &str) -> Option<u64> {
    if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    field.parse().ok()
}

/// Copy engine that delegates to `robocopy.exe`.
pub struct RobocopyEngine<R: CommandRunner> {
    runner: R,
    program: String,
}

impl RobocopyEngine<ProcessRunner> {
    /// Engine backed by a real process spawn. Only functional on Windows.
    pub fn new() -> Self {
        Self::with_runner(ProcessRunner)
    }
}

impl Default for RobocopyEngine<ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: CommandRunner> RobocopyEngine<R> {
    pub fn with_runner(runner: R) -> Self {
        Self {
            runner,
            program: PROGRAM.to_string(),
        }
    }

    /// Override the program name (used by tests and by non-standard deployments).
    pub fn with_program(mut self, program: impl Into<String>) -> Self {
        self.program = program.into();
        self
    }
}

impl<R: CommandRunner> CopyEngine for RobocopyEngine<R> {
    fn name(&self) -> &'static str {
        ENGINE_NAME
    }

    fn copy(
        &self,
        request: &CopyRequest,
        sink: &dyn ProgressSink,
    ) -> Result<CopyOutcome, IngestError> {
        let args = build_args(request);
        tracing::info!(command = %command_line(&self.program, &args), "invoking robocopy");

        let mut streamed_bytes = 0u64;
        let mut streamed_files = 0u64;
        let mut summary: Option<SummaryRow> = None;
        let mut file_summary: Option<SummaryRow> = None;

        let started = Instant::now();
        let exit_code = {
            let mut on_line = |line: &str| {
                if let Some(row) = parse_summary_row(line, "Bytes") {
                    summary = Some(row);
                    return;
                }
                if let Some(row) = parse_summary_row(line, "Files") {
                    file_summary = Some(row);
                    return;
                }
                if let Some(bytes) = parse_file_bytes(line) {
                    streamed_bytes += bytes;
                    streamed_files += 1;
                    sink.add_bytes(bytes);
                    sink.add_file();
                    tracing::debug!(bytes, line = line.trim(), "robocopy transferred file");
                }
            };
            self.runner.run(&self.program, &args, &mut on_line)?
        };
        let elapsed = started.elapsed();

        // The summary row is authoritative when present; streamed counters are the fallback for
        // truncated output (for example when robocopy is killed mid-run).
        let bytes_copied = summary
            .map(|s| s.copied)
            .unwrap_or(streamed_bytes)
            .max(streamed_bytes);
        let files_copied = file_summary
            .map(|s| s.copied)
            .unwrap_or(streamed_files)
            .max(streamed_files);

        if let Some(row) = summary {
            sink.observe_total_bytes(row.copied);
            if row.failed > 0 {
                tracing::warn!(failed_bytes = row.failed, "robocopy reported failed bytes");
            }
        }

        Ok(CopyOutcome {
            engine: ENGINE_NAME,
            bytes_copied,
            files_copied,
            elapsed,
            exit_code: Some(exit_code),
            retry_attempts_used: 0,
            dry_run: request.dry_run,
        })
    }
}

/// Real process runner.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessRunner;

#[cfg(windows)]
impl CommandRunner for ProcessRunner {
    fn run(
        &self,
        program: &str,
        args: &[String],
        on_line: &mut dyn FnMut(&str),
    ) -> Result<i32, IngestError> {
        use std::process::{Command, Stdio};

        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            // F1.1: stderr is set to null (not piped) to prevent a deadlock.
            // If robocopy writes more stderr than the OS pipe buffer (4–64 KB), it blocks
            // waiting to write while we block waiting for it to exit — permanent deadlock.
            // Setting null avoids the pipe entirely; any stderr output is simply discarded.
            // This is safe because robocopy writes diagnostics to stdout (parsed above) and
            // uses stderr only for catastrophic failures that will be visible in the exit code.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| IngestError::SpawnFailed {
                program: program.to_string(),
                source,
            })?;

        if let Some(stdout) = child.stdout.take() {
            // Robocopy emits OEM/ANSI text (e.g. CP850/CP437 on Windows), which is not always
            // valid UTF-8. We read raw bytes and decode lossily so that:
            // a) invalid byte sequences are replaced with U+FFFD rather than aborting;
            // b) the per-line progress callback still receives clean &str slices.
            //
            // F2.5: reuse a single Vec<u8> buffer to avoid per-line heap allocations.
            use std::io::BufRead;
            let mut reader = std::io::BufReader::with_capacity(65_536, stdout);
            let mut raw_line: Vec<u8> = Vec::with_capacity(512);
            loop {
                raw_line.clear();
                match reader.read_until(b'\n', &mut raw_line) {
                    Ok(0) => break,
                    Ok(_) => {
                        // Robocopy uses \r\n; strip both from the end.
                        while raw_line.last() == Some(&b'\n') || raw_line.last() == Some(&b'\r') {
                            raw_line.pop();
                        }
                        // F22: Decode OEM CP850 text accurately (handles Italian/European accented characters correctly).
                        let encoding = encoding_rs::Encoding::for_label(b"ibm850").unwrap_or(encoding_rs::UTF_8);
                        let (decoded, _, _) = encoding.decode(&raw_line);
                        on_line(&decoded);
                    }
                    Err(source) => {
                        return Err(IngestError::SpawnFailed {
                            program: program.to_string(),
                            source,
                        });
                    }
                }
            }
        }

        let status = child.wait().map_err(|source| IngestError::SpawnFailed {
            program: program.to_string(),
            source,
        })?;
        Ok(status.code().unwrap_or(-1))
    }
}

#[cfg(not(windows))]
impl CommandRunner for ProcessRunner {
    fn run(
        &self,
        _program: &str,
        _args: &[String],
        _on_line: &mut dyn FnMut(&str),
    ) -> Result<i32, IngestError> {
        Err(IngestError::RobocopyUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::CountingProgress;
    use crate::testkit::ScriptedRunner;
    use std::path::PathBuf;

    fn request() -> CopyRequest {
        CopyRequest {
            source: PathBuf::from("D:\\landing"),
            dest: PathBuf::from("E:\\warehouse"),
            pattern: "*.csv".to_string(),
            threads: 16,
            file_retries: 4,
            retry_wait_seconds: 9,
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
    fn args_start_with_source_dest_and_pattern() {
        let args = build_args(&request());
        assert_eq!(args[0], "D:\\landing");
        assert_eq!(args[1], "E:\\warehouse");
        assert_eq!(args[2], "*.csv");
    }

    #[test]
    fn args_map_cli_values_to_robocopy_flags() {
        let args = build_args(&request());
        assert!(args.contains(&"/MT:16".to_string()));
        assert!(args.contains(&"/R:4".to_string()));
        assert!(args.contains(&"/W:9".to_string()));
        assert!(args.contains(&"/E".to_string()));
        assert!(
            args.contains(&"/BYTES".to_string()),
            "/BYTES is required for byte parsing"
        );
        assert!(
            args.contains(&"/NP".to_string()),
            "/NP keeps one line per file"
        );
        assert!(!args.contains(&"/L".to_string()), "not a dry run");
    }

    #[test]
    fn dry_run_appends_list_only_flag() {
        let mut request = request();
        request.dry_run = true;
        assert!(build_args(&request).contains(&"/L".to_string()));
    }

    #[test]
    fn command_line_quotes_paths_with_spaces() {
        let mut request = request();
        request.source = PathBuf::from("D:\\csv landing");
        let rendered = command_line(PROGRAM, &build_args(&request));
        assert!(rendered.contains("\"D:\\csv landing\""), "got {rendered}");
        assert!(rendered.starts_with("robocopy.exe "));
    }

    #[test]
    fn parses_tab_separated_new_file_line() {
        let line = "\t    New File  \t\t     52428800\tsales_2026_01.csv";
        assert_eq!(parse_file_bytes(line), Some(52_428_800));
    }

    #[test]
    fn parses_space_padded_lines_and_other_statuses() {
        assert_eq!(
            parse_file_bytes("      Newer          1024   a.csv"),
            Some(1024)
        );
        assert_eq!(
            parse_file_bytes("      Older          2048   b.csv"),
            Some(2048)
        );
        assert_eq!(
            parse_file_bytes("    Modified         4096   c.csv"),
            Some(4096)
        );
        assert_eq!(parse_file_bytes("\t\t      512\td.csv"), Some(512));
    }

    #[test]
    fn status_keywords_are_only_matched_in_leading_columns() {
        assert_eq!(
            parse_file_bytes("\t    New File  \t\t   100\tSameDay_Mismatch.csv"),
            Some(100),
            "keywords inside a file name must not disqualify the line"
        );
    }

    #[test]
    fn ignores_headers_summaries_and_metadata() {
        for line in [
            "",
            "   ",
            "-------------------------------------------------------------------------------",
            "   ROBOCOPY     ::     Robust File Copy for Windows",
            "  Started : Thursday, 30 July 2026 10:00:00",
            "   Source : D:\\landing\\",
            "     Dest : E:\\warehouse\\",
            "    Files : *.csv",
            "  Options : /BYTES /S /E /DCOPY:DA /COPY:DAT /MT:16 /R:4 /W:9",
            "               Total    Copied   Skipped  Mismatch    FAILED    Extras",
            "   Bytes :  104857600 104857600         0         0         0         0",
            "   Times :   0:00:12     0:00:11                       0:00:00     0:00:00",
            "   Speed :           9532156 Bytes/sec.",
        ] {
            assert_eq!(parse_file_bytes(line), None, "should ignore: {line:?}");
        }
    }

    #[test]
    fn ignores_directory_and_extra_file_lines() {
        assert_eq!(
            parse_file_bytes("          New Dir          4    D:\\landing\\day1\\"),
            None
        );
        assert_eq!(
            parse_file_bytes("\t*EXTRA File \t\t   4096\tstale.csv"),
            None
        );
        assert_eq!(
            parse_file_bytes("\t    Same   \t\t   4096\tunchanged.csv"),
            None
        );
        assert_eq!(
            parse_file_bytes("          Newer      50%    1024   partial.csv"),
            None
        );
    }

    #[test]
    fn parses_bytes_and_files_summary_rows() {
        let bytes = parse_summary_row("   Bytes :  104857600 104857600  0  0  0  0", "Bytes")
            .expect("bytes row");
        assert_eq!(bytes.total, 104_857_600);
        assert_eq!(bytes.copied, 104_857_600);

        let files = parse_summary_row(
            "   Files :        12        10         1         0         1         0",
            "Files",
        )
        .expect("files row");
        assert_eq!(files.total, 12);
        assert_eq!(files.copied, 10);
        assert_eq!(files.skipped, 1);
        assert_eq!(files.failed, 1);

        assert_eq!(
            parse_summary_row("   Dirs :  1  0  1  0  0  0", "Bytes"),
            None
        );
        assert_eq!(
            parse_summary_row("   Files : *.csv", "Files"),
            None,
            "options echo is not a row"
        );
    }

    #[test]
    fn engine_reports_bytes_files_and_exit_code() {
        let output = vec![
            "-------------------------------------------------------------------------------"
                .to_string(),
            "   Source : D:\\landing\\".to_string(),
            "\t    New File  \t\t     1048576\tpart-0001.csv".to_string(),
            "\t    New File  \t\t     2097152\tpart-0002.csv".to_string(),
            "               Total    Copied   Skipped  Mismatch    FAILED    Extras".to_string(),
            "   Files :         2         2         0         0         0         0".to_string(),
            "   Bytes :   3145728   3145728         0         0         0         0".to_string(),
        ];
        let runner = ScriptedRunner::new(vec![(output, 1)]);
        let engine = RobocopyEngine::with_runner(runner);
        let sink = CountingProgress::default();

        let outcome = engine.copy(&request(), &sink).expect("copy runs");

        assert_eq!(outcome.engine, ENGINE_NAME);
        assert_eq!(outcome.bytes_copied, 3_145_728);
        assert_eq!(outcome.files_copied, 2);
        assert_eq!(outcome.exit_code, Some(1));
        assert!(outcome.is_success());
        assert_eq!(sink.files(), 2, "progress sink saw both files");
        assert_eq!(sink.bytes(), 3_145_728);
    }

    #[test]
    fn engine_falls_back_to_streamed_bytes_without_summary() {
        let output = vec!["\t    New File  \t\t     4096\tonly.csv".to_string()];
        let engine = RobocopyEngine::with_runner(ScriptedRunner::new(vec![(output, 8)]));
        let sink = CountingProgress::default();

        let outcome = engine.copy(&request(), &sink).expect("copy runs");

        assert_eq!(outcome.bytes_copied, 4096);
        assert_eq!(outcome.files_copied, 1);
        assert_eq!(outcome.exit_code, Some(8));
        assert!(!outcome.is_success(), "exit code 8 is a failure");
    }

    #[test]
    fn engine_forwards_the_expected_command_line_to_the_runner() {
        let runner = ScriptedRunner::new(vec![(vec![], 0)]);
        let recorded = runner.recorded();
        let engine = RobocopyEngine::with_runner(runner);

        engine
            .copy(&request(), &CountingProgress::default())
            .expect("copy runs");

        let invocations = recorded.lock().expect("lock");
        assert_eq!(invocations.len(), 1);
        let (program, args) = &invocations[0];
        assert_eq!(program, PROGRAM);
        assert_eq!(args, &build_args(&request()));
    }

    #[test]
    fn engine_propagates_runner_errors() {
        let engine = RobocopyEngine::with_runner(ScriptedRunner::always_failing());
        let error = engine
            .copy(&request(), &CountingProgress::default())
            .expect_err("runner fails");
        assert!(matches!(error, IngestError::RobocopyUnavailable));
    }

    #[cfg(not(windows))]
    #[test]
    fn real_process_runner_is_unavailable_off_windows() {
        let error = ProcessRunner
            .run(PROGRAM, &[], &mut |_| {})
            .expect_err("robocopy cannot run here");
        assert!(matches!(error, IngestError::RobocopyUnavailable));
    }

    #[cfg(windows)]
    #[test]
    fn real_robocopy_reports_its_version() {
        // Only meaningful on Windows, where robocopy.exe actually exists.
        let mut lines = Vec::new();
        let code = ProcessRunner
            .run(PROGRAM, &["/?".to_string()], &mut |line| {
                lines.push(line.to_string())
            })
            .expect("robocopy should be present on Windows");
        assert!(code >= 0);
        assert!(lines.iter().any(|l| l.contains("ROBOCOPY")));
    }

    /// F1.2: trailing backslash on paths would escape the closing quote in Windows shell.
    #[test]
    fn trailing_backslash_is_stripped_from_source_and_dest() {
        let mut req = request();
        req.source = PathBuf::from("D:\\landing\\");
        req.dest = PathBuf::from("E:\\warehouse\\");
        let args = build_args(&req);
        assert_eq!(args[0], "D:\\landing", "source trailing sep stripped");
        assert_eq!(args[1], "E:\\warehouse", "dest trailing sep stripped");
    }

    #[test]
    fn normalize_path_arg_strips_various_separators() {
        assert_eq!(normalize_path_arg("C:\\foo\\", false), "C:\\foo");
        assert_eq!(normalize_path_arg("/mnt/data/", false), "/mnt/data");
        assert_eq!(normalize_path_arg("C:\\foo", false), "C:\\foo", "no trailing sep: unchanged");
        assert_eq!(normalize_path_arg("C:\\\\", false), "C:", "multiple seps stripped");
    }

    /// F4.1: exclusion patterns produce /XF and /XD flags.
    #[test]
    fn exclude_flags_are_generated() {
        let mut req = request();
        req.exclude_files = vec!["*.tmp".to_string(), "thumbs.db".to_string()];
        req.exclude_dirs = vec![".git".to_string()];
        let args = build_args(&req);

        // /XF *.tmp /XF thumbs.db
        let xf_positions: Vec<_> = args.iter().enumerate().filter(|(_, a)| a == &"/XF").collect();
        assert_eq!(xf_positions.len(), 2, "two /XF flags");
        assert_eq!(args[xf_positions[0].0 + 1], "*.tmp");
        assert_eq!(args[xf_positions[1].0 + 1], "thumbs.db");

        let xd_pos = args.iter().position(|a| a == "/XD").expect("/XD present");
        assert_eq!(args[xd_pos + 1], ".git");
    }

    /// F4.2: date filters produce /MINAGE and /MAXAGE flags.
    #[test]
    fn date_filter_flags_are_generated() {
        let mut req = request();
        req.min_age_days = Some(7);
        req.max_age_days = Some(30);
        let args = build_args(&req);
        assert!(args.contains(&"/MINAGE:7".to_string()));
        assert!(args.contains(&"/MAXAGE:30".to_string()));

        let req2 = request(); // no date filters
        let args2 = build_args(&req2);
        assert!(!args2.iter().any(|a| a.starts_with("/MINAGE")));
        assert!(!args2.iter().any(|a| a.starts_with("/MAXAGE")));
    }

    /// F4.3: mirror mode produces /MIR flag.
    #[test]
    fn mirror_flag_is_generated() {
        let mut req = request();
        req.mirror = true;
        let args = build_args(&req);
        assert!(args.contains(&"/MIR".to_string()));

        let req2 = request(); // no mirror
        assert!(!build_args(&req2).contains(&"/MIR".to_string()));
    }

    /// F4.5: bandwidth throttle produces /IPG flag.
    #[test]
    fn bandwidth_throttle_flag_is_generated() {
        let mut req = request();
        req.inter_packet_gap_ms = Some(5242);
        let args = build_args(&req);
        assert!(args.contains(&"/IPG:5242".to_string()));

        let req2 = request(); // no throttle
        assert!(!build_args(&req2).iter().any(|a| a.starts_with("/IPG")));
    }
}
