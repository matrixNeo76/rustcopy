//! Command line interface definition.

use std::path::{Path, PathBuf};

use clap::Parser;

use crate::engine::{CopyRequest, RetryPolicy};
use crate::errors::IngestError;

pub const MIN_THREADS: u8 = 1;
pub const MAX_THREADS: u16 = 128;

/// Number of copy threads to use when --threads is not specified.
fn default_threads() -> u16 {
    // num_cpus::get() returns logical CPUs, which is a sensible starting point.
    // Clamped to [1, 128] to honour robocopy's /MT constraints.
    (num_cpus::get() as u16).clamp(MIN_THREADS as u16, MAX_THREADS)
}

/// Ingest large CSV datasets with Robocopy and benchmark it against a naive copy.
#[derive(Parser, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    name = "robocopy_ingest",
    version,
    about = "Robocopy-based CSV ingestion with throughput reporting and baseline benchmarking",
    long_about = "Wraps Windows robocopy.exe to ingest large (50GB-scale) CSV datasets.\n\
                  Provides throughput-based progress, an outer retry loop on top of robocopy's \
                  own /R and /W retries, optional SHA-256 integrity verification and an optional \
                  baseline comparison against a naive recursive copy (the equivalent of \
                  `Get-ChildItem | Copy-Item`). Results are written as a JSON report.\n\n\
                  Note: real transfers require Windows because robocopy.exe is a Windows tool. \
                  The baseline engine and all report/verification logic are cross-platform."
)]
pub struct Args {
    /// Path to a TOML configuration file containing default argument settings.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Source directory containing the CSV files to ingest.
    /// Not required when --restore-from is given (it is derived from the backup report).
    ///
    /// F24 fix: this used to be a plain `PathBuf` with `default_value = ""` alongside
    /// `required_unless_present`, on the assumption that an empty-string default would let clap
    /// skip the arg when --restore-from was present. It didn't: clap treats an empty-string
    /// default as no default at all, so the arg stayed unconditionally required and
    /// `--restore-from` was unreachable from the CLI no matter what was passed (reproduced with a
    /// minimal clap repro outside this crate, and confirmed native PowerShell, not just this
    /// crate's own test harness, hit the same error). `Option<PathBuf>` sidesteps the whole
    /// default-value question: clap simply leaves it `None` when omitted, exactly like it already
    /// does for `--config`/`--restore-from` above.
    #[arg(long, value_name = "PATH", required_unless_present = "restore_from")]
    pub source: Option<PathBuf>,

    /// Destination directory for the ingested files.
    /// Not required when --restore-from is given (it is derived from the backup report).
    #[arg(long, value_name = "PATH", required_unless_present = "restore_from")]
    pub dest: Option<PathBuf>,

    /// File pattern to match (robocopy file filter / glob on the file name). Defaults to "*" (all files).
    #[arg(long, default_value = "*", value_name = "GLOB")]
    pub pattern: String,

    /// Number of copy threads, mapped to robocopy's /MT:N (1-128). Defaults to logical CPU count.
    #[arg(long, default_value_t = default_threads(), value_name = "N")]
    pub threads: u16,

    /// Retries per failed file, mapped to robocopy's /R:N and used as the outer retry budget.
    #[arg(long, default_value_t = 3, value_name = "N")]
    pub retries: u32,

    /// Seconds to wait between retries, mapped to robocopy's /W:N and the outer backoff base.
    #[arg(long, default_value_t = 5, value_name = "N")]
    pub retry_wait_seconds: u64,

    /// After the transfer, compare checksums of source and destination files.
    #[arg(long, default_value_t = false)]
    pub verify_integrity: bool,

    /// [NOT IMPLEMENTED] Reserved for selective integrity verification (only hash files this run
    /// actually modified/copied, via the incremental state cache); accepted for forward
    /// compatibility but currently has no effect — --verify-integrity always hashes every file in
    /// the source inventory. Real per-file "was this touched this run" tracking needs the
    /// incremental cache (see ROADMAP.md F28, which reuses cache.rs — a separate, currently-
    /// orphaned subsystem this fix was not scoped to wire up).
    #[arg(long, default_value_t = false)]
    pub fast_verify: bool,

    /// After --verify-integrity, treat missing/unreadable files matching well-known transient
    /// patterns (.log, .tmp, anything under .git/objects/) as expected rather than a verification
    /// failure — these can legitimately disappear between the copy and the verification pass (log
    /// rotation, temp-file cleanup, git garbage collection). Has no effect without
    /// --verify-integrity.
    #[arg(long, default_value_t = false)]
    pub ignore_transient_missing: bool,

    /// Hash algorithm for integrity checks: sha256 (default) or blake3 (3-5x faster).
    #[arg(long, default_value = "sha256", value_name = "ALGO")]
    pub hash_algo: crate::integrity::HashAlgorithm,

    /// Also run a naive recursive copy into a temporary destination and time it for comparison.
    #[arg(long, default_value_t = false)]
    pub compare_baseline: bool,

    /// Path of the final JSON report.
    #[arg(
        long,
        default_value = "./robocopy_ingest_report.json",
        value_name = "PATH"
    )]
    pub report_path: PathBuf,

    /// Path of the asynchronous log file.
    #[arg(long, default_value = "./robocopy_ingest.log", value_name = "PATH")]
    pub log_path: PathBuf,

    /// Show what would happen without copying anything (robocopy /L).
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    // ── F4.3: Mirror mode ───────────────────────────────────────────────────
    /// Mirror source to destination: delete files in the destination that are not in the source.
    /// Maps to robocopy /MIR.  CAUTION: files present only in dest will be DELETED.
    #[arg(long, default_value_t = false)]
    pub mirror: bool,

    /// Force purge during mirror mode without safety confirmation threshold check.
    #[arg(long, default_value_t = false)]
    pub force_purge: bool,

    // ── F4.1: Exclusion filters ─────────────────────────────────────────────
    /// Exclude files matching the given pattern(s) (repeatable, maps to /XF).
    /// Example: --exclude-files "*.tmp" --exclude-files "thumbs.db"
    #[arg(long, value_name = "GLOB", action = clap::ArgAction::Append)]
    pub exclude_files: Vec<String>,

    /// Exclude directories matching the given pattern(s) (repeatable, maps to /XD).
    /// Example: --exclude-dirs ".git" --exclude-dirs "node_modules"
    #[arg(long, value_name = "GLOB", action = clap::ArgAction::Append)]
    pub exclude_dirs: Vec<String>,

    // ── F4.2: Date-based filters ────────────────────────────────────────────
    /// Skip files modified more than N days ago (maps to robocopy /MINAGE:N).
    #[arg(long, value_name = "DAYS")]
    pub min_age_days: Option<u32>,

    /// Skip files modified less than N days ago (maps to robocopy /MAXAGE:N).
    #[arg(long, value_name = "DAYS")]
    pub max_age_days: Option<u32>,

    // ── F4.5: Bandwidth throttling ──────────────────────────────────────────
    /// Limit transfer bandwidth to approximately this many MB/s.
    /// Converted to robocopy's /IPG (inter-packet gap in ms).
    /// Example: --bandwidth-limit-mbps 100 to cap at ~100 MB/s.
    #[arg(long, value_name = "MB_PER_SEC")]
    pub bandwidth_limit_mbps: Option<u32>,

    // ── F2.6: Pre-scan control ──────────────────────────────────────────────
    /// Skip the upfront source tree walk that counts files and bytes before copying starts.
    /// Without a prescan the progress bar has no total and integrity check is unavailable.
    /// Useful for very large trees (millions of files) where the walk itself takes minutes.
    #[arg(long, default_value_t = false)]
    pub no_prescan: bool,

    // ── F6.1: Windows Long Path support ─────────────────────────────────────
    /// Prepend Windows long path prefix `\\?\` for deep path structures (> 260 chars).
    #[arg(long, default_value_t = false)]
    pub long_paths: bool,

    // ── F6.2: Metadata & ACL preservation ────────────────────────────────────
    /// Preserve directory timestamps (maps to robocopy /DCOPY:DAT).
    #[arg(long, default_value_t = false)]
    pub preserve_timestamps: bool,

    /// Preserve NTFS ACL security permissions (maps to robocopy /COPYALL).
    #[arg(long, default_value_t = false)]
    pub preserve_acl: bool,

    // ── F26d: junction/symlink consistency ───────────────────────────────────
    /// Exclude junction points and symlinked directories from the copy (maps to robocopy /XJ).
    /// Without this, robocopy follows them (its own default), which can duplicate data or
    /// recurse into a self-referencing junction; the source inventory used for the progress
    /// total and --verify-integrity follows the same rule, so what gets counted and what gets
    /// copied always agree.
    #[arg(long, default_value_t = false)]
    pub exclude_junctions: bool,

    // ── F7.1: Webhook completion notifications ──────────────────────────────
    /// Send an HTTP POST JSON execution summary to this Webhook URL upon completion.
    #[arg(long, value_name = "URL")]
    pub webhook_url: Option<String>,

    // F8.1: Disaster Recovery & Restore Mode ──────────────────────────────
    /// Path to a previous backup report JSON file to initiate reverse restore mode.
    #[arg(long, value_name = "REPORT_PATH")]
    pub restore_from: Option<PathBuf>,

    // ── F10.1: HTML Standalone Dashboard Report ──────────────────────────────
    /// Path to write an interactive HTML summary report.
    #[arg(long, value_name = "PATH")]
    pub html_report_path: Option<PathBuf>,

    // ── F11.1: State Cache & Deduplication ───────────────────────────────────
    /// [NOT IMPLEMENTED] Reserved for incremental state caching (.ingest_cache) to skip
    /// unchanged files; accepted for forward compatibility but currently has no effect.
    #[arg(long, default_value_t = false)]
    pub enable_dedup: bool,

    // ── F15.1: Zero-Trust Streaming Encryption ──────────────────────────────
    /// Encrypt every copied file in the destination with AES-256-GCM after the transfer
    /// completes. VALUE is the key material: `env:NAME` reads it from environment variable
    /// NAME, `file:PATH` reads it from the first line of PATH, and any other value is treated
    /// as a literal passphrase (avoid this on shared/multi-user hosts: it is visible in the
    /// process list). The key is stretched to 256 bits with SHA-256.
    #[arg(long, value_name = "KEY")]
    pub encrypt_aes256: Option<String>,

    // ── F25b: Zero-Trust Streaming Decryption ─────────────────────────────────
    /// Decrypt every copied file in the destination with AES-256-GCM after the transfer
    /// completes — the counterpart to --encrypt-aes256, using the same VALUE key format
    /// (`env:NAME`, `file:PATH`, or a literal passphrase). Typically combined with
    /// --restore-from to decrypt a backup while restoring it, but works after any successful
    /// transfer. Cannot be combined with --encrypt-aes256 in the same run.
    #[arg(long, value_name = "KEY")]
    pub decrypt: Option<String>,

    // ── F18.1: Direct Cloud Sync ─────────────────────────────────────────────
    /// [NOT IMPLEMENTED] Reserved for direct S3/Azure Blob sync; accepted for forward
    /// compatibility but currently has no effect (no cloud transfer is performed).
    #[arg(long, value_name = "URI")]
    pub cloud_sync_target: Option<String>,

    // ── F19.1: Windows Service Registration ──────────────────────────────────
    /// [NOT IMPLEMENTED] Reserved for Windows Service registration; accepted for forward
    /// compatibility but currently has no effect (no service is registered).
    #[arg(long, default_value_t = false)]
    pub install_service: bool,
}

impl Args {
    /// Real path to the source directory.
    ///
    /// Panics if called before `validate()` has confirmed `--source` was supplied — clap's
    /// `required_unless_present = "restore_from"` guarantees it is `Some` on every code path that
    /// isn't short-circuited by restore mode (`validate()` returns early for that case, and
    /// `restore::build_restore_args` always supplies both paths explicitly), so this is an
    /// invariant violation, not a user-facing error, if it ever fires.
    pub fn source(&self) -> &Path {
        self.source
            .as_deref()
            .expect("--source is required unless --restore-from is given, enforced by clap")
    }

    /// Real path to the destination directory. See [`Self::source`] for the invariant.
    pub fn dest(&self) -> &Path {
        self.dest
            .as_deref()
            .expect("--dest is required unless --restore-from is given, enforced by clap")
    }

    /// Merge non-None fields from `config` into `self` where CLI flags were not explicitly passed.
    pub fn merge_config(&mut self, config: crate::config::IngestConfig) {
        if self.source.is_none() {
            self.source = config.source;
        }
        if self.dest.is_none() {
            self.dest = config.dest;
        }
        if let Some(pat) = config.pattern {
            // Only apply the config file's pattern when the CLI still holds clap's own default
            // ("*"); otherwise an explicit `--pattern` on the command line would be silently
            // overwritten. This was previously checked against "*.csv", which never matched the
            // real default and made the config file's pattern dead on arrival.
            //
            // Caveat: this can't distinguish "user typed --pattern '*'" from "user didn't pass
            // --pattern at all" (both look identical here); doing that properly needs
            // `ArgMatches::value_source`, which isn't threaded through yet for any of the
            // merge_config fields (all of them share this limitation, not just pattern).
            if self.pattern == "*" {
                self.pattern = pat;
            }
        }
        if let Some(th) = config.threads {
            self.threads = th;
        }
        if let Some(ret) = config.retries {
            self.retries = ret;
        }
        if let Some(w) = config.retry_wait_seconds {
            self.retry_wait_seconds = w;
        }
        if let Some(v) = config.verify_integrity {
            self.verify_integrity = v;
        }
        if let Some(algo) = config.hash_algo {
            self.hash_algo = algo;
        }
        if let Some(base) = config.compare_baseline {
            self.compare_baseline = base;
        }
        if let Some(rep) = config.report_path {
            self.report_path = rep;
        }
        if let Some(log) = config.log_path {
            self.log_path = log;
        }
        if let Some(dry) = config.dry_run {
            self.dry_run = dry;
        }
        if let Some(mir) = config.mirror {
            self.mirror = mir;
        }
        if let Some(ex_files) = config.exclude_files {
            self.exclude_files.extend(ex_files);
        }
        if let Some(ex_dirs) = config.exclude_dirs {
            self.exclude_dirs.extend(ex_dirs);
        }
        if let Some(min_age) = config.min_age_days {
            self.min_age_days = Some(min_age);
        }
        if let Some(max_age) = config.max_age_days {
            self.max_age_days = Some(max_age);
        }
        if let Some(limit) = config.bandwidth_limit_mbps {
            self.bandwidth_limit_mbps = Some(limit);
        }
        if let Some(no_pre) = config.no_prescan {
            self.no_prescan = no_pre;
        }
        if let Some(lp) = config.long_paths {
            self.long_paths = lp;
        }
        if let Some(pt) = config.preserve_timestamps {
            self.preserve_timestamps = pt;
        }
        if let Some(pa) = config.preserve_acl {
            self.preserve_acl = pa;
        }
        if let Some(wh) = config.webhook_url {
            self.webhook_url = Some(wh);
        }
    }
    pub fn validate(&self) -> Result<(), IngestError> {
        // Checked before the restore-mode short-circuit below: --decrypt's primary use case is
        // exactly --restore-from, so this conflict must still be caught in that mode.
        if self.encrypt_aes256.is_some() && self.decrypt.is_some() {
            return Err(IngestError::EncryptAndDecryptConflict);
        }
        if self.restore_from.is_some() {
            return Ok(());
        }
        if !(MIN_THREADS as u16..=MAX_THREADS).contains(&self.threads) {
            return Err(IngestError::InvalidThreads(self.threads));
        }
        let source = self.source();
        let dest = self.dest();
        if !source.exists() {
            return Err(IngestError::SourceMissing(source.to_path_buf()));
        }
        if !source.is_dir() {
            return Err(IngestError::SourceNotADirectory(source.to_path_buf()));
        }
        if dest.exists() && !dest.is_dir() {
            return Err(IngestError::DestNotADirectory(dest.to_path_buf()));
        }
        if self.pattern.trim().is_empty() {
            return Err(IngestError::EmptyPattern);
        }
        // F3.4: prevent copying a directory into itself.
        let source_canonical = source.canonicalize().unwrap_or_else(|_| source.to_path_buf());
        let dest_canonical = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
        if source_canonical == dest_canonical {
            return Err(IngestError::SourceEqualsDestination(source_canonical));
        }
        // Check that dest is not inside source (would cause infinite recursion with /E).
        if dest_canonical.starts_with(&source_canonical) {
            return Err(IngestError::DestInsideSource {
                src: source_canonical,
                dest: dest_canonical,
            });
        }
        Ok(())
    }

    /// Outer retry policy: `--retries` extra attempts with `--retry-wait-seconds` as backoff base.
    pub fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy::new(self.retries, self.retry_wait_seconds)
    }

    /// Convert MB/s bandwidth limit to robocopy's /IPG (inter-packet gap in milliseconds).
    ///
    /// Robocopy's /IPG represents the gap in **milliseconds** inserted after every 64 KB packet.
    /// Formula, with `mbps` in megabytes (10^6 bytes) per second:
    ///   gap_ms = packet_bits / bandwidth_bps * 1000
    ///          = (65_536 bytes * 8 bits/byte) / (mbps * 1_000_000 bytes/s * 8 bits/byte) * 1000
    ///          = 65_536 / (mbps * 1_000_000) * 1000
    ///          = 65.536 / mbps
    ///
    /// (The previous implementation used 524_288 / mbps — the numerator in bits instead of
    /// converting through the correct byte/bit factors — which made the computed gap about
    /// 8000x too large: at 100 MB/s it produced a 5242 ms gap between 64 KB packets, throttling
    /// the real transfer down to roughly 12 KB/s instead of ~100 MB/s.)
    ///
    /// At high requested bandwidths the exact gap rounds below 1 ms, which robocopy cannot
    /// express; those requests are clamped to the smallest representable gap (1 ms) rather than
    /// silently becoming "no throttle".
    pub fn inter_packet_gap_ms(&self) -> Option<u32> {
        self.bandwidth_limit_mbps.and_then(|mbps| {
            if mbps == 0 {
                return None;
            }
            let gap_ms = 65.536_f64 / mbps as f64;
            Some(gap_ms.round().clamp(1.0, u32::MAX as f64) as u32)
        })
    }

    /// Request handed to the copy engines.
    pub fn copy_request(&self, dest: PathBuf) -> CopyRequest {
        CopyRequest {
            source: self.source().to_path_buf(),
            dest,
            pattern: self.pattern.clone(),
            threads: self.threads,
            file_retries: self.retries,
            retry_wait_seconds: self.retry_wait_seconds,
            dry_run: self.dry_run,
            mirror: self.mirror,
            exclude_files: self.exclude_files.clone(),
            exclude_dirs: self.exclude_dirs.clone(),
            min_age_days: self.min_age_days,
            max_age_days: self.max_age_days,
            inter_packet_gap_ms: self.inter_packet_gap_ms(),
            prescan: !self.no_prescan,
            long_paths: self.long_paths,
            preserve_timestamps: self.preserve_timestamps,
            preserve_acl: self.preserve_acl,
            exclude_junctions: self.exclude_junctions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn base_args() -> Vec<&'static str> {
        vec!["robocopy_ingest", "--source", ".", "--dest", "./out"]
    }

    #[test]
    fn clap_definition_is_valid() {
        Args::command().debug_assert();
    }

    #[test]
    fn defaults_match_specification() {
        let args = Args::try_parse_from(base_args()).expect("parse");
        assert_eq!(args.pattern, "*");
        // Thread count defaults to logical CPUs, clamped to [1, 128].
        assert!((MIN_THREADS as u16..=MAX_THREADS).contains(&args.threads));
        assert_eq!(args.retries, 3);
        assert_eq!(args.retry_wait_seconds, 5);
        assert!(!args.verify_integrity);
        assert!(!args.compare_baseline);
        assert!(!args.dry_run);
        assert!(!args.mirror);
        assert!(args.exclude_files.is_empty());
        assert!(args.exclude_dirs.is_empty());
        assert!(args.min_age_days.is_none());
        assert!(args.max_age_days.is_none());
        assert!(args.bandwidth_limit_mbps.is_none());
        assert!(!args.no_prescan);
        assert!(!args.exclude_junctions);
        assert!(!args.fast_verify);
        assert!(!args.ignore_transient_missing);
        assert_eq!(
            args.report_path,
            PathBuf::from("./robocopy_ingest_report.json")
        );
        assert_eq!(args.log_path, PathBuf::from("./robocopy_ingest.log"));
    }

    #[test]
    fn source_and_dest_are_required() {
        assert!(Args::try_parse_from(["robocopy_ingest"]).is_err());
        assert!(Args::try_parse_from(["robocopy_ingest", "--source", "."]).is_err());
    }

    /// F24 regression test: `--restore-from` alone, with neither `--source` nor `--dest`, must
    /// parse successfully. This is the exact invocation from the README's disaster-recovery
    /// example, and the one a minimal clap repro (outside this crate) proved would fail with
    /// "a value is required for '--source <PATH>' but none was supplied" — caused by
    /// `default_value = ""` making clap treat the arg as unconditionally required, silently
    /// ignoring `required_unless_present`. This test parses only (no filesystem access, no
    /// tempdir needed): it exists to catch a clap-level regression, not to exercise the restore
    /// flow itself (see `tests/cli_smoke.rs` for a black-box test that actually runs the compiled
    /// binary end-to-end against a tempdir-only fixture).
    #[test]
    fn restore_from_alone_is_sufficient() {
        let args = Args::try_parse_from(["robocopy_ingest", "--restore-from", "report.json"])
            .expect("--restore-from alone must parse without --source/--dest");
        assert!(args.source.is_none());
        assert!(args.dest.is_none());
        assert_eq!(args.restore_from, Some(PathBuf::from("report.json")));
        // validate() must also accept this shape (it short-circuits for restore mode without
        // ever touching the None source/dest).
        assert!(args.validate().is_ok());
    }

    #[test]
    #[should_panic(expected = "--source is required unless --restore-from is given")]
    fn source_accessor_panics_if_invariant_is_violated() {
        // Directly exercises the accessor's documented invariant (never reachable via the real
        // CLI, since clap enforces required_unless_present before validate() ever runs).
        let args = Args::try_parse_from(["robocopy_ingest", "--restore-from", "report.json"])
            .expect("parse");
        let _ = args.source();
    }

    #[test]
    fn flags_are_parsed() {
        let mut argv = base_args();
        argv.extend([
            "--threads",
            "32",
            "--verify-integrity",
            "--compare-baseline",
            "--dry-run",
        ]);
        let args = Args::try_parse_from(argv).expect("parse");
        assert_eq!(args.threads, 32);
        assert!(args.verify_integrity);
        assert!(args.compare_baseline);
        assert!(args.dry_run);
    }

    #[test]
    fn thread_count_bounds_are_enforced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().to_str().expect("utf8 path");

        // src == src → SourceEqualsDestination, not InvalidThreads.
        // Use a real different dest path for the bound checks.
        let dest = tempfile::tempdir().expect("dest");
        let dst = dest.path().to_str().expect("utf8 dest");

        for (threads, ok) in [("0", false), ("1", true), ("128", true), ("129", false)] {
            let args = Args::try_parse_from([
                "robocopy_ingest",
                "--source",
                src,
                "--dest",
                dst,
                "--threads",
                threads,
            ])
            .expect("parse");
            let result = args.validate();
            assert_eq!(
                result.is_ok(),
                ok,
                "threads={threads} expected ok={ok}, got: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn missing_source_is_rejected() {
        let args = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            "/definitely/not/here",
            "--dest",
            "/tmp",
        ])
        .expect("parse");
        assert!(matches!(
            args.validate(),
            Err(IngestError::SourceMissing(_))
        ));
    }

    #[test]
    fn empty_pattern_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().to_str().expect("utf8 path");
        let args = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            src,
            "--dest",
            src,
            "--pattern",
            "   ",
        ])
        .expect("parse");
        assert!(matches!(args.validate(), Err(IngestError::EmptyPattern)));
    }

    #[test]
    fn retry_policy_mirrors_cli_flags() {
        let mut argv = base_args();
        argv.extend(["--retries", "5", "--retry-wait-seconds", "7"]);
        let args = Args::try_parse_from(argv).expect("parse");

        let policy = args.retry_policy();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.total_attempts(), 6);
        assert_eq!(policy.backoff(0), std::time::Duration::from_secs(7));
        assert_eq!(policy.backoff(1), std::time::Duration::from_secs(14));
    }

    #[test]
    fn copy_request_targets_the_given_destination() {
        let mut argv = base_args();
        argv.extend(["--threads", "16", "--dry-run"]);
        let args = Args::try_parse_from(argv).expect("parse");

        let request = args.copy_request(PathBuf::from("/tmp/baseline"));
        assert_eq!(request.dest, PathBuf::from("/tmp/baseline"));
        assert_eq!(request.source, args.source());
        assert_eq!(request.threads, 16);
        assert_eq!(request.pattern, "*");
        assert!(request.dry_run);
        assert!(!request.mirror);
        assert!(request.exclude_files.is_empty());
        assert!(request.inter_packet_gap_ms.is_none());
        assert!(request.prescan);
    }

    #[test]
    fn new_feature_flags_are_parsed() {
        let mut argv = base_args();
        argv.extend([
            "--mirror",
            "--exclude-files",
            "*.tmp",
            "--exclude-dirs",
            ".git",
            "--min-age-days",
            "7",
            "--max-age-days",
            "30",
            "--bandwidth-limit-mbps",
            "10",
            "--no-prescan",
            "--exclude-junctions",
        ]);
        let args = Args::try_parse_from(argv).expect("parse");

        assert!(args.mirror);
        assert_eq!(args.exclude_files, vec!["*.tmp".to_string()]);
        assert_eq!(args.exclude_dirs, vec![".git".to_string()]);
        assert_eq!(args.min_age_days, Some(7));
        assert_eq!(args.max_age_days, Some(30));
        assert_eq!(args.bandwidth_limit_mbps, Some(10));
        assert!(args.no_prescan);
        assert!(args.exclude_junctions);

        let request = args.copy_request(PathBuf::from("/dst"));
        assert!(request.mirror);
        assert_eq!(request.min_age_days, Some(7));
        // 65.536 / 10 = 6.5536 -> rounds to 7 ms
        assert_eq!(request.inter_packet_gap_ms, Some(7));
        assert!(!request.prescan);
        assert!(request.exclude_junctions);
    }

    #[test]
    fn bandwidth_ipg_conversion_is_correct() {
        let mut argv = base_args();
        argv.extend(["--bandwidth-limit-mbps", "5"]);
        let args = Args::try_parse_from(argv).expect("parse");
        // 65.536 / 5 = 13.1072 -> rounds to 13 ms
        assert_eq!(args.inter_packet_gap_ms(), Some(13));

        // At high requested bandwidths the exact gap is below robocopy's 1ms granularity and
        // is clamped to the smallest representable value rather than silently disabling the
        // throttle.
        let mut argv = base_args();
        argv.extend(["--bandwidth-limit-mbps", "1000"]);
        let args = Args::try_parse_from(argv).expect("parse");
        assert_eq!(args.inter_packet_gap_ms(), Some(1));

        let mut argv = base_args();
        argv.extend(["--bandwidth-limit-mbps", "0"]);
        let args = Args::try_parse_from(argv).expect("parse");
        assert_eq!(args.inter_packet_gap_ms(), None);
    }

    /// F3.4: source == destination must be rejected.
    #[test]
    fn source_equals_dest_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_str().expect("utf8");
        let args = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            path,
            "--dest",
            path,
        ])
        .expect("parse");
        assert!(matches!(
            args.validate(),
            Err(IngestError::SourceEqualsDestination(_))
        ));
    }

    /// F3.4: destination inside source must be rejected.
    #[test]
    fn dest_inside_source_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().to_str().expect("utf8 src");
        let dest = dir.path().join("subdir");
        std::fs::create_dir_all(&dest).expect("create dest");
        let dst = dest.to_str().expect("utf8 dst");
        let args = Args::try_parse_from([
            "robocopy_ingest",
            "--source",
            src,
            "--dest",
            dst,
        ])
        .expect("parse");
        assert!(matches!(
            args.validate(),
            Err(IngestError::DestInsideSource { .. })
        ));
    }
}
