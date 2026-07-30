# Graph Report - .  (2026-07-30)

## Corpus Check
- Corpus is ~20,666 words - fits in a single context window. You may not need a graph.

## Summary
- 580 nodes · 1174 edges · 22 communities
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 20 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Robocopy Process Engine|Robocopy Process Engine]]
- [[_COMMUNITY_Copy Engine Abstraction & Retries|Copy Engine Abstraction & Retries]]
- [[_COMMUNITY_Throughput Progress Tracking|Throughput Progress Tracking]]
- [[_COMMUNITY_Naive Baseline Copy Engine|Naive Baseline Copy Engine]]
- [[_COMMUNITY_Test Doubles & Mocks|Test Doubles & Mocks]]
- [[_COMMUNITY_JSON Report Generation|JSON Report Generation]]
- [[_COMMUNITY_Async Bounded Logger|Async Bounded Logger]]
- [[_COMMUNITY_CLI Orchestration & Safety Checks|CLI Orchestration & Safety Checks]]
- [[_COMMUNITY_CLI Argument Parsing|CLI Argument Parsing]]
- [[_COMMUNITY_Integrity Verification (Hashing)|Integrity Verification (Hashing)]]
- [[_COMMUNITY_Robocopy Exit Code Decoding|Robocopy Exit Code Decoding]]
- [[_COMMUNITY_AES-256-GCM Encryption|AES-256-GCM Encryption]]
- [[_COMMUNITY_Dedup Cache (Unimplemented)|Dedup Cache (Unimplemented)]]
- [[_COMMUNITY_TOML Configuration|TOML Configuration]]
- [[_COMMUNITY_HTML Report Escaping|HTML Report Escaping]]
- [[_COMMUNITY_Webhook Notifications|Webhook Notifications]]
- [[_COMMUNITY_CP850 OEM Decoding|CP850 OEM Decoding]]
- [[_COMMUNITY_Cloud Sync (Unimplemented)|Cloud Sync (Unimplemented)]]
- [[_COMMUNITY_Error Types & Retry Classification|Error Types & Retry Classification]]
- [[_COMMUNITY_Disaster Recovery Restore|Disaster Recovery Restore]]
- [[_COMMUNITY_Windows Service (Unimplemented)|Windows Service (Unimplemented)]]

## God Nodes (most connected - your core abstractions)
1. `run_with_retries()` - 24 edges
2. `fixture_tree()` - 22 edges
3. `ScriptedEngine` - 19 edges
4. `ThroughputProgress` - 18 edges
5. `request()` - 15 edges
6. `verify()` - 15 edges
7. `IngestReport` - 15 edges
8. `build_args()` - 14 edges
9. `LogHandle` - 14 edges
10. `CopyRequestBuilder` - 13 edges

## Surprising Connections (you probably didn't know these)
- `identical_trees_pass()` --calls--> `fixture_tree()`  [INFERRED]
  src/integrity.rs → src/testkit.rs
- `corrupted_destination_is_detected()` --calls--> `fixture_tree()`  [INFERRED]
  src/integrity.rs → src/testkit.rs
- `truncated_destination_is_detected()` --calls--> `fixture_tree()`  [INFERRED]
  src/integrity.rs → src/testkit.rs
- `truncated_destination_is_detected()` --calls--> `write_fixture_file()`  [INFERRED]
  src/integrity.rs → src/testkit.rs
- `missing_destination_files_are_listed()` --calls--> `fixture_tree()`  [INFERRED]
  src/integrity.rs → src/testkit.rs

## Import Cycles
- 1-file cycle: `src/cache.rs -> src/cache.rs`
- 1-file cycle: `src/cli.rs -> src/cli.rs`
- 1-file cycle: `src/cloud.rs -> src/cloud.rs`
- 1-file cycle: `src/config.rs -> src/config.rs`
- 1-file cycle: `src/crypto.rs -> src/crypto.rs`
- 1-file cycle: `src/engine/mod.rs -> src/engine/mod.rs`
- 1-file cycle: `src/report.rs -> src/report.rs`
- 1-file cycle: `src/engine/naive.rs -> src/engine/naive.rs`
- 1-file cycle: `src/progress.rs -> src/progress.rs`
- 1-file cycle: `src/engine/robocopy.rs -> src/engine/robocopy.rs`
- 1-file cycle: `src/errors.rs -> src/errors.rs`
- 1-file cycle: `src/html_report.rs -> src/html_report.rs`
- 1-file cycle: `src/integrity.rs -> src/integrity.rs`
- 1-file cycle: `src/logging.rs -> src/logging.rs`
- 1-file cycle: `src/main.rs -> src/main.rs`
- 1-file cycle: `src/notify.rs -> src/notify.rs`
- 1-file cycle: `src/restore.rs -> src/restore.rs`
- 1-file cycle: `src/scan.rs -> src/scan.rs`
- 1-file cycle: `src/testkit.rs -> src/testkit.rs`

## Communities (22 total, 0 thin omitted)

### Community 0 - "Robocopy Process Engine"
Cohesion: 0.07
Nodes (50): args_map_cli_values_to_robocopy_flags(), args_start_with_source_dest_and_pattern(), bandwidth_throttle_flag_is_generated(), build_args(), command_line(), command_line_quotes_paths_with_spaces(), CommandRunner, date_filter_flags_are_generated() (+42 more)

### Community 1 - "Copy Engine Abstraction & Retries"
Cohesion: 0.10
Nodes (33): backoff_is_exponential_and_capped(), CopyEngine, CopyOutcome, CopyRequest, CopyRequestBuilder, elapsed_time_accumulates_across_attempts(), engine_without_exit_code_is_successful(), fatal_exit_code_is_not_retried() (+25 more)

### Community 2 - "Throughput Progress Tracking"
Cohesion: 0.06
Nodes (22): Instant, ProgressBar, ProgressStyle, counting_sink_accumulates(), CountingProgress, hidden_bar_tracks_max_of_both_sources(), NoopProgress, observed_total_never_regresses() (+14 more)

### Community 3 - "Naive Baseline Copy Engine"
Cohesion: 0.09
Nodes (39): copied_content_is_identical(), copies_matching_files_preserving_the_tree(), copy_one(), creates_a_missing_destination_directory(), dry_run_counts_without_writing(), empty_source_yields_an_empty_outcome(), missing_source_directory_is_an_error(), NaiveCopyEngine (+31 more)

### Community 4 - "Test Doubles & Mocks"
Cohesion: 0.07
Nodes (33): AtomicUsize, Box, CommandRunner, F, Fn, I, Invocations, Mutex (+25 more)

### Community 5 - "JSON Report Generation"
Cohesion: 0.13
Nodes (36): DateTime, From, args(), baseline_outcome(), ConfigurationReport, failed_integrity_is_serialized_with_details(), format_bytes(), HostMetadata (+28 more)

### Community 6 - "Async Bounded Logger"
Cohesion: 0.11
Nodes (31): DefaultGuard, MakeWriter, Sender, appends_to_an_existing_log(), build(), ChannelWriter, dropped_lines_starts_at_zero(), init() (+23 more)

### Community 7 - "CLI Orchestration & Safety Checks"
Cohesion: 0.17
Nodes (31): ExitCode, baseline(), baseline_dir(), check_mirror_safety(), encrypt_destination(), execute(), inventory_source(), kill_active_child() (+23 more)

### Community 8 - "CLI Argument Parsing"
Cohesion: 0.09
Nodes (19): IngestConfig, RetryPolicy, Args, bandwidth_ipg_conversion_is_correct(), base_args(), copy_request_targets_the_given_destination(), defaults_match_specification(), flags_are_parsed() (+11 more)

### Community 9 - "Integrity Verification (Hashing)"
Cohesion: 0.15
Nodes (26): ScannedFile, blake3_file(), blake3_hashing_passes_and_is_correct(), copy_tree(), corrupted_destination_is_detected(), empty_inventory_passes(), FileVerificationOutcome, hash_file() (+18 more)

### Community 10 - "Robocopy Exit Code Decoding"
Cohesion: 0.17
Nodes (8): abnormal_termination_is_retried(), bit_decoding_is_correct(), codes_one_to_seven_are_success(), describe_lists_every_active_bit(), RobocopyStatus, Self, String, zero_means_nothing_to_do()

### Community 11 - "AES-256-GCM Encryption"
Cohesion: 0.18
Nodes (12): Aes256Gcm, crypto_round_trip_is_symmetric(), CryptoManager, each_encryption_uses_a_fresh_nonce(), resolve_key(), IngestError, Result, Self (+4 more)

### Community 12 - "Dedup Cache (Unimplemented)"
Cohesion: 0.19
Nodes (11): HashMap, cache_skips_unchanged_files(), default_cache_path(), FileCacheEntry, IngestCache, Option, Path, PathBuf (+3 more)

### Community 13 - "TOML Configuration"
Cohesion: 0.19
Nodes (10): IngestConfig, HashAlgorithm, IngestError, Option, Path, PathBuf, Result, Self (+2 more)

### Community 14 - "HTML Report Escaping"
Cohesion: 0.29
Nodes (8): escape_html(), generate_html_report(), html_report_generates_valid_content(), malicious_path_in_report_is_escaped_in_the_generated_html(), IngestReport, Path, Result, String

### Community 15 - "Webhook Notifications"
Cohesion: 0.31
Nodes (7): IngestReport, Result, Self, String, send_webhook(), unreachable_host_surfaces_a_real_error(), WebhookPayload

### Community 16 - "CP850 OEM Decoding"
Cohesion: 0.33
Nodes (6): active_oem_code_page(), decode_cp850(), decode_robocopy_output(), every_byte_value_has_a_mapping(), Option, String

### Community 17 - "Cloud Sync (Unimplemented)"
Cohesion: 0.39
Nodes (7): cloud_sync_request_constructs_properly(), CloudProvider, CloudSyncRequest, Path, Result, String, sync_to_cloud()

### Community 18 - "Error Types & Retry Classification"
Cohesion: 0.32
Nodes (5): IngestError, Error, Into, PathBuf, Self

### Community 19 - "Disaster Recovery Restore"
Cohesion: 0.36
Nodes (7): build_restore_args(), restore_args_reverses_source_and_dest(), Args, Option, Path, PathBuf, Result

### Community 20 - "Windows Service (Unimplemented)"
Cohesion: 0.29
Nodes (3): register_windows_service(), Result, String

## Knowledge Gaps
- **89 isolated node(s):** `Self`, `Result`, `PathBuf`, `String`, `HashAlgorithm` (+84 more)
  These have ≤1 connection - possible missing edges or undocumented components.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `fixture_tree()` connect `Naive Baseline Copy Engine` to `Integrity Verification (Hashing)`, `Test Doubles & Mocks`?**
  _High betweenness centrality (0.153) - this node is a cross-community bridge._
- **Why does `Instant` connect `Throughput Progress Tracking` to `Robocopy Process Engine`, `Naive Baseline Copy Engine`?**
  _High betweenness centrality (0.142) - this node is a cross-community bridge._
- **Are the 17 inferred relationships involving `fixture_tree()` (e.g. with `copied_content_is_identical()` and `copies_matching_files_preserving_the_tree()`) actually correct?**
  _`fixture_tree()` has 17 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Self`, `Result`, `PathBuf` to the rest of the system?**
  _89 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Robocopy Process Engine` be split into smaller, more focused modules?**
  _Cohesion score 0.06923076923076923 - nodes in this community are weakly interconnected._
- **Should `Copy Engine Abstraction & Retries` be split into smaller, more focused modules?**
  _Cohesion score 0.1003921568627451 - nodes in this community are weakly interconnected._
- **Should `Throughput Progress Tracking` be split into smaller, more focused modules?**
  _Cohesion score 0.058823529411764705 - nodes in this community are weakly interconnected._