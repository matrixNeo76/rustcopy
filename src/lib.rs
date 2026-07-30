//! Robocopy-based CSV ingestion with throughput reporting and baseline benchmarking.
//!
//! The crate is split so that everything except the actual `robocopy.exe` invocation is portable
//! and unit-testable on any platform:
//!
//! * [`cli`] argument parsing and validation;
//! * [`engine`] the [`engine::CopyEngine`] abstraction plus the robocopy and naive baseline
//!   implementations and the outer retry loop;
//! * [`exit_code`] interpretation of robocopy's bitmask exit codes;
//! * [`progress`] throughput-based progress reporting;
//! * [`integrity`] SHA-256 verification of source vs destination;
//! * [`logging`] non-blocking file logger;
//! * [`report`] the JSON report;
//! * [`scan`] source inventory and directory sizing;
//! * [`testkit`] test doubles shared by unit and integration tests.

pub mod cache;
pub mod cli;
pub mod cloud;
pub mod config;
pub mod crypto;
pub mod engine;
pub mod errors;
pub mod exit_code;
pub mod html_report;
pub mod integrity;
pub mod logging;
pub mod notify;
pub mod oem_codec;
pub mod progress;
pub mod report;
pub mod restore;
pub mod scan;
pub mod server;
pub mod service;
pub mod testkit;
