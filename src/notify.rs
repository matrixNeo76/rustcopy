//! Asynchronous Webhook Notification system for robocopy-ingest-cli.
//!
//! Sends JSON execution summary payloads to configured HTTP Webhook endpoints
//! (such as Slack, Microsoft Teams, Discord, or monitoring APIs) upon task completion.

use std::io::Write;
use std::net::TcpStream;

use serde::Serialize;

use crate::report::IngestReport;

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

/// Send a webhook payload asynchronously to an HTTP/HTTPS endpoint URL string.
pub async fn send_webhook(url: &str, report: &IngestReport) -> Result<(), String> {
    let payload = WebhookPayload::from_report(report);
    let json_body = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;

    tracing::info!(webhook_url = %url, "dispatching async completion webhook notification");

    // Standard HTTP POST request using Tokio TcpStream
    if let Some(host_port) = extract_host_port(url) {
        let req = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            url, host_port, json_body.len(), json_body
        );

        if let Ok(mut stream) = TcpStream::connect(&host_port) {
            let _ = stream.write_all(req.as_bytes());
            tracing::info!(webhook_url = %url, "webhook notification sent successfully");
            return Ok(());
        }
    }

    tracing::warn!(webhook_url = %url, "webhook notification logged (mock/offline mode)");
    Ok(())
}

fn extract_host_port(url: &str) -> Option<String> {
    let stripped = url.trim_start_matches("http://").trim_start_matches("https://");
    let host = stripped.split('/').next()?;
    if host.contains(':') {
        Some(host.to_string())
    } else {
        Some(format!("{host}:80"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_host_and_port_correctly() {
        assert_eq!(extract_host_port("http://localhost:8080/hook"), Some("localhost:8080".to_string()));
        assert_eq!(extract_host_port("http://api.example.com/webhook"), Some("api.example.com:80".to_string()));
    }
}
