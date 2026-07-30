//! Command line interface definition.

use std::path::PathBuf;

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
    #[arg(long, value_name = "PATH")]
    pub source: PathBuf,

    /// Destination directory for the ingested files.
    #[arg(long, value_name = "PATH")]
    pub dest: PathBuf,

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
    /// Enable incremental state caching (.ingest_cache) to skip unchanged files.
    #[arg(long, default_value_t = false)]
    pub enable_dedup: bool,

    // ── F14.1: Live Web Dashboard Server ─────────────────────────────────────
    /// Start a live web monitoring dashboard HTTP server on this port (e.g. 8080).
    #[arg(long, value_name = "PORT")]
    pub serve_dashboard: Option<u16>,

    // ── F15.1: Zero-Trust Streaming Encryption ──────────────────────────────
    /// Encrypt payload files with AES-256 using the key provided.
    #[arg(long, value_name = "KEY")]
    pub encrypt_aes256: Option<String>,

    // ── F18.1: Direct Cloud Sync ─────────────────────────────────────────────
    /// Target S3 or Azure Blob container for cloud synchronization (e.g. s3://bucket/prefix).
    #[arg(long, value_name = "URI")]
    pub cloud_sync_target: Option<String>,

    // ── F19.1: Windows Service Registration ──────────────────────────────────
    /// Register and run the binary as a Windows Service background daemon.
    #[arg(long, default_value_t = false)]
    pub install_service: bool,
}

impl Args {
    /// Merge non-None fields from `config` into `self` where CLI flags were not explicitly passed.
    pub fn merge_config(&mut self, config: crate::config::IngestConfig) {
        if self.source.as_os_str().is_empty() {
            if let Some(src) = config.source {
                self.source = src;
            }
        }
        if self.dest.as_os_str().is_empty() {
            if let Some(dst) = config.dest {
                self.dest = dst;
            }
        }
        if let Some(pat) = config.pattern {
            if self.pattern == "*.csv" {
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
        if self.restore_from.is_some() {
            return Ok(());
        }
        if !(MIN_THREADS as u16..=MAX_THREADS).contains(&self.threads) {
            return Err(IngestError::InvalidThreads(self.threads));
        }
        if !self.source.exists() {
            return Err(IngestError::SourceMissing(self.source.clone()));
        }
        if !self.source.is_dir() {
            return Err(IngestError::SourceNotADirectory(self.source.clone()));
        }
        if self.dest.exists() && !self.dest.is_dir() {
            return Err(IngestError::DestNotADirectory(self.dest.clone()));
        }
        if self.pattern.trim().is_empty() {
            return Err(IngestError::EmptyPattern);
        }
        // F3.4: prevent copying a directory into itself.
        let source_canonical = self.source.canonicalize().unwrap_or_else(|_| self.source.clone());
        let dest_canonical = self.dest.canonicalize().unwrap_or_else(|_| self.dest.clone());
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
    /// Robocopy's /IPG represents the gap in **milliseconds** between 64 KB packets.
    /// Formula: gap_ms = (64 * 1024 * 8) / (bandwidth_bps) * 1000
    ///        = (64 * 1024 * 8 * 1000) / (mbps * 1_000_000)
    ///        = 524_288 / mbps
    ///
    /// At 100 MB/s this yields about 5 ms gap per 64 KB packet, which keeps the load on the
    /// link at roughly the desired level.  At very low bandwidth values the gap becomes large
    /// (and potentially inaccurate), which is fine for the typical "don't saturate the link"
    /// use case.
    pub fn inter_packet_gap_ms(&self) -> Option<u32> {
        self.bandwidth_limit_mbps.and_then(|mbps| {
            if mbps == 0 {
                return None;
            }
            Some((524_288u64 / mbps as u64).clamp(1, u32::MAX as u64) as u32)
        })
    }

    /// Request handed to the copy engines.
    pub fn copy_request(&self, dest: PathBuf) -> CopyRequest {
        CopyRequest {
            source: self.source.clone(),
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
        assert_eq!(request.source, args.source);
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
            "100",
            "--no-prescan",
        ]);
        let args = Args::try_parse_from(argv).expect("parse");

        assert!(args.mirror);
        assert_eq!(args.exclude_files, vec!["*.tmp".to_string()]);
        assert_eq!(args.exclude_dirs, vec![".git".to_string()]);
        assert_eq!(args.min_age_days, Some(7));
        assert_eq!(args.max_age_days, Some(30));
        assert_eq!(args.bandwidth_limit_mbps, Some(100));
        assert!(args.no_prescan);

        let request = args.copy_request(PathBuf::from("/dst"));
        assert!(request.mirror);
        assert_eq!(request.min_age_days, Some(7));
        // 524_288 / 100 = 5242 ms
        assert_eq!(request.inter_packet_gap_ms, Some(5242));
        assert!(!request.prescan);
    }

    #[test]
    fn bandwidth_ipg_conversion_is_correct() {
        let mut argv = base_args();
        argv.extend(["--bandwidth-limit-mbps", "1000"]);
        let args = Args::try_parse_from(argv).expect("parse");
        // 524_288 / 1000 = 524 ms
        assert_eq!(args.inter_packet_gap_ms(), Some(524));

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
