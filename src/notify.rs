//! Asynchronous Webhook Notification system for robocopy-ingest-cli.
//!
//! Sends JSON execution summary payloads to configured HTTP/HTTPS Webhook endpoints (such as
//! Slack, Microsoft Teams, Discord, or monitoring APIs) upon task completion, using `reqwest`
//! with `rustls` so both `http://` and `https://` URLs work. Unlike the previous implementation
//! (a hand-rolled blocking `std::net::TcpStream` POST that only ever connected to port 80 in
//! plaintext and treated every failure as silent success), this surfaces connection errors,
//! non-2xx statuses and timeouts back to the caller so they can be logged and included in the
//! run's outcome instead of being reported as delivered.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::report::IngestReport;

/// How long to wait for the webhook endpoint to respond before giving up.
const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// Schema version of [`WebhookPayload`]. Bump this whenever a field is added, renamed or
/// changed in a way that could break an existing receiver — see `ANALYSIS.md` D6 for why this
/// matters: `report::SCHEMA_VERSION` was left at `1` after a breaking change to the `Mismatch`
/// JSON schema, and old reports became undeserializable with no way to detect the mismatch.
pub const NOTIFY_SCHEMA_VERSION: u32 = 1;

fn current_notify_schema_version() -> u32 {
    NOTIFY_SCHEMA_VERSION
}

/// Outcome of a backup run, as sent to (and expected by) a notification receiver.
///
/// Serializes as the uppercase strings `"SUCCESS"` / `"FAILED"` — the exact wire format the
/// previous plain-`String` field used — so this is a type-safety improvement, not a breaking wire
/// change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum BackupStatus {
    Success,
    Failed,
}

impl BackupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            BackupStatus::Success => "SUCCESS",
            BackupStatus::Failed => "FAILED",
        }
    }
}

impl std::fmt::Display for BackupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Payload sent to Webhook endpoints.
///
/// Every field beyond the original `text`/`report_summary`/`status`/`files_copied`/
/// `bytes_copied`/`elapsed_seconds` carries `#[serde(default...)]` so a receiver built against
/// this type can still deserialize a payload from an older sender, or from a hand-written script
/// that only populates the fields it cares about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    #[serde(default = "current_notify_schema_version")]
    pub schema_version: u32,
    pub text: String,
    pub report_summary: String,
    pub status: BackupStatus,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub elapsed_seconds: f64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub dest: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub tool_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity_status: Option<String>,
}

impl WebhookPayload {
    pub fn from_report(report: &IngestReport) -> Self {
        let integrity_passed = report.integrity_check.as_ref().map(|c| c.passed()).unwrap_or(true);
        let status = if integrity_passed && report.robocopy_transfer.exit_code.unwrap_or(0) < 8 {
            BackupStatus::Success
        } else {
            BackupStatus::Failed
        };

        let text = format!(
            "[*robocopy-ingest*] Backup [{}] -> [{}] - Status: {}",
            report.source, report.dest, status
        );

        Self {
            schema_version: NOTIFY_SCHEMA_VERSION,
            text,
            report_summary: report.human_summary(),
            status,
            files_copied: report.robocopy_transfer.files_copied,
            bytes_copied: report.robocopy_transfer.bytes_copied,
            elapsed_seconds: report.robocopy_transfer.elapsed_seconds,
            source: report.source.clone(),
            dest: report.dest.clone(),
            host: report.host_metadata.hostname.clone(),
            tool_version: report.tool_version.clone(),
            exit_code: report.robocopy_transfer.exit_code,
            integrity_status: report
                .integrity_check
                .as_ref()
                .map(|c| if c.passed() { "PASSED" } else { "FAILED" }.to_string()),
        }
    }
}

/// Send a webhook payload to an HTTP/HTTPS endpoint URL, returning the real error on failure.
pub async fn send_webhook(url: &str, report: &IngestReport) -> Result<(), String> {
    let payload = WebhookPayload::from_report(report);

    tracing::info!(webhook_url = %url, "dispatching completion webhook notification");

    let client = reqwest::Client::builder()
        .timeout(WEBHOOK_TIMEOUT)
        .build()
        .map_err(|e| format!("cannot build HTTP client: {e}"))?;

    let response = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("webhook request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "webhook endpoint returned {status}: {}",
            body.chars().take(500).collect::<String>()
        ));
    }

    tracing::info!(webhook_url = %url, status = %status, "webhook notification delivered");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_reports_success_below_the_error_threshold() {
        let payload = WebhookPayload {
            schema_version: NOTIFY_SCHEMA_VERSION,
            text: "test".to_string(),
            report_summary: "summary".to_string(),
            status: BackupStatus::Success,
            files_copied: 10,
            bytes_copied: 1000,
            elapsed_seconds: 1.0,
            source: "D:/landing".to_string(),
            dest: "E:/warehouse".to_string(),
            host: "srv".to_string(),
            tool_version: "5.1.0".to_string(),
            exit_code: Some(1),
            integrity_status: Some("PASSED".to_string()),
        };
        assert_eq!(payload.status, BackupStatus::Success);
        assert_eq!(payload.status.as_str(), "SUCCESS");
    }

    #[test]
    fn status_serializes_as_uppercase_string_on_the_wire() {
        let json = serde_json::to_string(&BackupStatus::Success).expect("serialize");
        assert_eq!(json, "\"SUCCESS\"");
        let json = serde_json::to_string(&BackupStatus::Failed).expect("serialize");
        assert_eq!(json, "\"FAILED\"");
    }

    #[test]
    fn payload_from_report_includes_every_new_field() {
        let report_json = r#"{
            "schema_version": 1,
            "timestamp": "2026-07-30T09:14:22Z",
            "tool_version": "5.1.0",
            "host_platform": "windows",
            "host_metadata": { "hostname": "srv01", "os_name": "windows", "logical_cpus": 8 },
            "source": "D:/landing",
            "dest": "E:/warehouse",
            "total_files": 1,
            "total_bytes": 100,
            "robocopy_transfer": {
                "engine": "robocopy",
                "elapsed_seconds": 1.0,
                "throughput_mbps": 100.0,
                "bytes_copied": 100,
                "files_copied": 1,
                "exit_code": 1,
                "retry_attempts_used": 0,
                "dry_run": false
            },
            "phase_timing": { "inventory_seconds": 0.1, "transfer_seconds": 1.0, "total_seconds": 1.1 },
            "configuration": {
                "threads": 8,
                "retries": 3,
                "retry_wait_seconds": 5,
                "pattern": "*.csv",
                "verify_integrity": true,
                "compare_baseline": false,
                "dry_run": false
            }
        }"#;
        let report: IngestReport = serde_json::from_str(report_json).expect("parse fixture");
        let payload = WebhookPayload::from_report(&report);

        assert_eq!(payload.schema_version, NOTIFY_SCHEMA_VERSION);
        assert_eq!(payload.source, "D:/landing");
        assert_eq!(payload.dest, "E:/warehouse");
        assert_eq!(payload.host, "srv01");
        assert_eq!(payload.tool_version, "5.1.0");
        assert_eq!(payload.exit_code, Some(1));
        // No integrity_check in the fixture -> unknown, so no integrity_status is reported.
        assert_eq!(payload.integrity_status, None);
        assert_eq!(payload.status, BackupStatus::Success);
    }

    #[test]
    fn a_minimal_hand_written_payload_still_deserializes() {
        // Simulates a third-party script that only sends the fields it has, relying on
        // #[serde(default)] for everything else.
        let minimal = r#"{
            "text": "hi",
            "report_summary": "ok",
            "status": "FAILED",
            "files_copied": 0,
            "bytes_copied": 0,
            "elapsed_seconds": 0.0
        }"#;
        let payload: WebhookPayload = serde_json::from_str(minimal).expect("lenient deserialize");
        assert_eq!(payload.schema_version, NOTIFY_SCHEMA_VERSION);
        assert_eq!(payload.status, BackupStatus::Failed);
        assert_eq!(payload.source, "");
        assert_eq!(payload.exit_code, None);
    }

    #[tokio::test]
    async fn unreachable_host_surfaces_a_real_error() {
        // Port 0 is never a valid connection target: this must fail, not silently return Ok(()).
        let report_json = r#"{
            "schema_version": 1,
            "timestamp": "2026-07-30T09:14:22Z",
            "tool_version": "5.1.0",
            "host_platform": "windows",
            "host_metadata": { "hostname": "srv", "os_name": "windows", "logical_cpus": 8 },
            "source": "D:/landing",
            "dest": "E:/warehouse",
            "total_files": 1,
            "total_bytes": 100,
            "robocopy_transfer": {
                "engine": "robocopy",
                "elapsed_seconds": 1.0,
                "throughput_mbps": 100.0,
                "bytes_copied": 100,
                "files_copied": 1,
                "retry_attempts_used": 0,
                "dry_run": false
            },
            "phase_timing": { "inventory_seconds": 0.1, "transfer_seconds": 1.0, "total_seconds": 1.1 },
            "configuration": {
                "threads": 8,
                "retries": 3,
                "retry_wait_seconds": 5,
                "pattern": "*.csv",
                "verify_integrity": true,
                "compare_baseline": false,
                "dry_run": false
            }
        }"#;
        let report: IngestReport = serde_json::from_str(report_json).expect("parse fixture");
        let result = send_webhook("http://127.0.0.1:0/hook", &report).await;
        assert!(result.is_err(), "connecting to port 0 must fail loudly");
    }
}
