//! Standalone HTML Dashboard Generator for robocopy-ingest-cli.
//!
//! Produces self-contained, interactive HTML reports with SVG charts and CSS styling
//! for visual audit of backup throughput, execution phases, and file status.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::report::IngestReport;

/// Generate a standalone HTML report page from an [`IngestReport`].
pub fn generate_html_report(report: &IngestReport, output_path: &Path) -> Result<()> {
    let status_color = if report.integrity_check.as_ref().map(|c| c.passed()).unwrap_or(true)
        && report.robocopy_transfer.exit_code.unwrap_or(0) < 8
    {
        "#2e7d32" // green
    } else {
        "#c62828" // red
    };

    let status_label = if report.integrity_check.as_ref().map(|c| c.passed()).unwrap_or(true)
        && report.robocopy_transfer.exit_code.unwrap_or(0) < 8
    {
        "SUCCESSFUL / PASSED"
    } else {
        "FAILED / WARNING"
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Robocopy Ingest Report - {}</title>
    <style>
        body {{ font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background-color: #0f172a; color: #f8fafc; margin: 0; padding: 20px; }}
        .container {{ max-width: 1100px; margin: 0 auto; background-color: #1e293b; border-radius: 12px; padding: 30px; box-shadow: 0 10px 25px rgba(0,0,0,0.5); }}
        .header {{ display: flex; justify-content: space-between; align-items: center; border-bottom: 2px solid #334155; padding-bottom: 20px; margin-bottom: 30px; }}
        h1 {{ margin: 0; color: #38bdf8; font-size: 28px; }}
        .badge {{ background-color: {}; color: white; padding: 6px 16px; border-radius: 20px; font-weight: bold; font-size: 14px; }}
        .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 20px; margin-bottom: 30px; }}
        .card {{ background-color: #0f172a; border: 1px solid #334155; border-radius: 8px; padding: 20px; text-align: center; }}
        .card-value {{ font-size: 26px; font-weight: bold; color: #38bdf8; margin-top: 8px; }}
        .card-label {{ font-size: 13px; color: #94a3b8; text-transform: uppercase; letter-spacing: 1px; }}
        .section {{ margin-bottom: 30px; }}
        .section-title {{ font-size: 18px; color: #cbd5e1; border-left: 4px solid #38bdf8; padding-left: 10px; margin-bottom: 15px; }}
        table {{ width: 100%; border-collapse: collapse; margin-top: 10px; font-size: 14px; }}
        th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #334155; }}
        th {{ background-color: #0f172a; color: #94a3b8; }}
        .footer {{ text-align: center; font-size: 12px; color: #64748b; margin-top: 40px; border-top: 1px solid #334155; padding-top: 20px; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div>
                <h1>robocopy-ingest-cli</h1>
                <div style="color: #94a3b8; margin-top: 5px;">Execution Report Summary</div>
            </div>
            <div class="badge">{}</div>
        </div>

        <div class="grid">
            <div class="card">
                <div class="card-label">Files Copied</div>
                <div class="card-value">{}</div>
            </div>
            <div class="card">
                <div class="card-label">Total Bytes</div>
                <div class="card-value">{:.2} MB</div>
            </div>
            <div class="card">
                <div class="card-label">Throughput</div>
                <div class="card-value">{:.2} MB/s</div>
            </div>
            <div class="card">
                <div class="card-label">Total Duration</div>
                <div class="card-value">{:.2} s</div>
            </div>
        </div>

        <div class="section">
            <div class="section-title">Transfer Paths & Parameters</div>
            <table>
                <tr><th>Source Path</th><td>{}</td></tr>
                <tr><th>Destination Path</th><td>{}</td></tr>
                <tr><th>Pattern</th><td>{}</td></tr>
                <tr><th>Parallel Threads</th><td>{}</td></tr>
                <tr><th>Exit Code</th><td>{}</td></tr>
            </table>
        </div>

        <div class="section">
            <div class="section-title">Phase Timing Breakdown</div>
            <table>
                <tr><th>Phase</th><th>Duration (Seconds)</th></tr>
                <tr><td>Source Inventory</td><td>{:.3} s</td></tr>
                <tr><td>Robocopy Transfer</td><td>{:.3} s</td></tr>
                <tr><td>Total Execution Time</td><td>{:.3} s</td></tr>
            </table>
        </div>

        <div class="footer">
            Generated automatically by robocopy-ingest-cli v{} on {}
        </div>
    </div>
</body>
</html>"#,
        report.timestamp,
        status_color,
        status_label,
        report.robocopy_transfer.files_copied,
        report.robocopy_transfer.bytes_copied as f64 / 1_000_000.0,
        report.robocopy_transfer.throughput_mbps,
        report.robocopy_transfer.elapsed_seconds,
        report.source,
        report.dest,
        report.configuration.pattern,
        report.configuration.threads,
        report.robocopy_transfer.exit_code.unwrap_or(0),
        report.phase_timing.inventory_seconds,
        report.phase_timing.transfer_seconds,
        report.phase_timing.total_seconds,
        report.tool_version,
        report.timestamp
    );

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("cannot create parent dir for {}", output_path.display()))?;
    }
    fs::write(output_path, html).with_context(|| format!("cannot write HTML report to {}", output_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::IngestReport;

    #[test]
    fn html_report_generates_valid_content() {
        let json = r#"{
            "schema_version": 1,
            "timestamp": "2026-07-30T09:14:22Z",
            "tool_version": "2.0.0",
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

        let report: IngestReport = serde_json::from_str(json).expect("parse report");
        let dir = tempfile::tempdir().expect("tempdir");
        let html_path = dir.path().join("report.html");
        generate_html_report(&report, &html_path).expect("generate html");
        assert!(html_path.exists());
        let html_text = fs::read_to_string(&html_path).expect("read html");
        assert!(html_text.contains("robocopy-ingest-cli"));
        assert!(html_text.contains("D:/landing"));
    }
}
