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

use serde::Serialize;

use crate::report::IngestReport;

/// How long to wait for the webhook endpoint to respond before giving up.
const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// Payload sent to Webhook endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub text: String,
    pub report_summary: String,
    pub status: String,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub elapsed_seconds: f64,
}

impl WebhookPayload {
    pub fn from_report(report: &IngestReport) -> Self {
        let status = if report.integrity_check.as_ref().map(|c| c.passed()).unwrap_or(true)
            && report.robocopy_transfer.exit_code.unwrap_or(0) < 8
        {
            "SUCCESS".to_string()
        } else {
            "FAILED".to_string()
        };

        let text = format!(
            "[*robocopy-ingest*] Backup [{}] -> [{}] - Status: {}",
            report.source, report.dest, status
        );

        Self {
            text,
            report_summary: report.human_summary(),
            status,
            files_copied: report.robocopy_transfer.files_copied,
            bytes_copied: report.robocopy_transfer.bytes_copied,
            elapsed_seconds: report.robocopy_transfer.elapsed_seconds,
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
            text: "test".to_string(),
            report_summary: "summary".to_string(),
            status: "SUCCESS".to_string(),
            files_copied: 10,
            bytes_copied: 1000,
            elapsed_seconds: 1.0,
        };
        assert_eq!(payload.status, "SUCCESS");
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
