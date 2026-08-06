//! True end-to-end test: spawns the real, compiled `notify-server` binary and the real, compiled
//! `robocopy_ingest` binary and lets them talk over a real HTTP connection.
//!
//! This whole file only compiles/runs under `cargo test --features notify-server` (the
//! `notify-server` binary only exists in that build). Deliberately not calling internal library
//! functions directly: D1 (`--restore-from` unreachable via clap) survived 140 passing tests
//! precisely because the one test touching it called `build_restore_args()` and skipped clap
//! entirely, never exercising the real CLI surface a user actually invokes.
#![cfg(feature = "notify-server")]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use robocopy_ingest::testkit::fixture_tree;

const NOTIFY_SERVER_BIN: &str = env!("CARGO_BIN_EXE_notify-server");
const INGEST_BIN: &str = env!("CARGO_BIN_EXE_robocopy_ingest");

/// Handle on a spawned `notify-server` child that kills it on drop, so a test failure (panic)
/// can't leave an orphaned server bound to a port for the rest of the test run.
struct NotifyServerHandle {
    child: Child,
    addr: String,
}

impl Drop for NotifyServerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_notify_server(token: Option<&str>) -> NotifyServerHandle {
    let mut command = Command::new(NOTIFY_SERVER_BIN);
    command
        .args(["--bind", "127.0.0.1:0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(token) = token {
        command.env("ROBOCOPY_NOTIFY_TOKEN", token);
    } else {
        command.env_remove("ROBOCOPY_NOTIFY_TOKEN");
    }

    let mut child = command.spawn().expect("spawn notify-server");
    let stdout = child.stdout.take().expect("piped stdout");
    let mut reader = BufReader::new(stdout);

    // The binary prints exactly one plain (non-tracing-formatted) line as soon as it has bound
    // the listener, specifically so callers (tests, operators using --bind ...:0) can discover
    // the real port without parsing tracing's formatted log output.
    let mut line = String::new();
    reader.read_line(&mut line).expect("read listening line");
    let addr = line
        .trim()
        .strip_prefix("robocopy-ingest notify-server listening on ")
        .unwrap_or_else(|| panic!("unexpected notify-server startup line: {line:?}"))
        .to_string();

    NotifyServerHandle { child, addr }
}

// Needs a real robocopy.exe transfer to succeed — Windows-only, like every other test elsewhere
// in this crate that expects a real backup to actually complete (e.g. cli_smoke.rs's
// a_real_dry_run_succeeds_on_windows). Caught only now that CI runs the notify-server feature set
// on Linux for the first time.
#[cfg(windows)]
#[test]
fn real_backup_delivers_to_a_real_notify_server() {
    let server = spawn_notify_server(None);

    let source = fixture_tree(&[("a.csv", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let report_path = workdir.path().join("report.json");

    let output = Command::new(INGEST_BIN)
        .args([
            "--source",
            source.path().to_str().expect("utf8"),
            "--dest",
            workdir.path().join("out").to_str().expect("utf8"),
            "--log-path",
            workdir.path().join("ingest.log").to_str().expect("utf8"),
            "--report-path",
            report_path.to_str().expect("utf8"),
            "--webhook-url",
            &format!("http://{}/notify", server.addr),
        ])
        .output()
        .expect("run robocopy_ingest");

    assert!(
        output.status.success(),
        "backup must succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read report"))
            .expect("valid json");
    assert!(
        report.get("webhook_error").is_none(),
        "delivery to a live notify-server must not produce a webhook_error; report: {report}"
    );
}

#[tokio::test]
async fn notify_server_health_endpoint_responds() {
    let server = spawn_notify_server(None);
    let url = format!("http://{}/health", server.addr);

    // A tiny retry loop: the child process has already printed its "listening on" line by the
    // time spawn_notify_server returns, but the very first connection can still occasionally
    // race the listener's accept loop starting up.
    let mut last_err = None;
    for _ in 0..20 {
        match reqwest::get(&url).await {
            Ok(response) => {
                assert_eq!(response.status(), 200);
                let body: serde_json::Value = response.json().await.expect("json body");
                assert!(body["schema_version"].is_number());
                return;
            }
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    panic!("could not reach notify-server /health: {last_err:?}");
}

// Same reason as real_backup_delivers_to_a_real_notify_server above: needs a real robocopy.exe
// transfer to succeed.
#[cfg(windows)]
#[test]
fn notify_server_requires_the_configured_token() {
    let server = spawn_notify_server(Some("integration-test-token"));

    let source = fixture_tree(&[("a.csv", 10)]);
    let workdir = tempfile::tempdir().expect("workdir");
    let report_path = workdir.path().join("report.json");

    // robocopy_ingest itself never sends an Authorization header, so pointing it at a
    // token-protected server must produce a webhook_error (401), not a delivered notification.
    let output = Command::new(INGEST_BIN)
        .args([
            "--source",
            source.path().to_str().expect("utf8"),
            "--dest",
            workdir.path().join("out").to_str().expect("utf8"),
            "--log-path",
            workdir.path().join("ingest.log").to_str().expect("utf8"),
            "--report-path",
            report_path.to_str().expect("utf8"),
            "--webhook-url",
            &format!("http://{}/notify", server.addr),
        ])
        .output()
        .expect("run robocopy_ingest");

    assert!(
        output.status.success(),
        "the backup itself must still succeed"
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read report"))
            .expect("valid json");
    let webhook_error = report["webhook_error"]
        .as_str()
        .expect("webhook_error present");
    assert!(webhook_error.contains("401"), "got: {webhook_error}");
}

/// F41 black-box test: `--install-service` and `--uninstall-service` on the `notify-server`
/// binary must be rejected together by clap — they mean opposite things, same as the equivalent
/// flags on `robocopy_ingest` (F37).
#[test]
fn install_service_and_uninstall_service_together_are_rejected() {
    let output = Command::new(NOTIFY_SERVER_BIN)
        .args(["--install-service", "--uninstall-service"])
        .output()
        .expect("run notify-server");
    assert!(!output.status.success());
}

/// `net session` only succeeds when the current process is Administrator-elevated — a well-known,
/// dependency-free way to detect elevation on Windows without a WinAPI binding just for this.
/// Needed because GitHub-hosted `windows-latest` runners execute jobs as Administrator by default
/// (unlike a normal interactive session), which this test's whole premise assumes is *not* the
/// case.
fn is_elevated() -> bool {
    Command::new("net")
        .args(["session"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// F41 black-box test: without Administrator elevation, `--install-service`/`--uninstall-service`
/// must fail cleanly with a service-related error instead of panicking, hanging, or silently
/// succeeding. A genuine `CreateService`/`DeleteService` round trip against the real SCM needs real
/// elevation and real machine state, same declared limitation as `robocopy_ingest`'s own
/// `--install-service` (F37) and `--vss-snapshot` (F30) — see `install_and_uninstall_service_round_trip`
/// below for the `#[ignore]`d elevated version of this test.
///
/// Skips its assertions (rather than failing) when the process turns out to already be elevated —
/// GitHub Actions' `windows-latest` runners run as Administrator, which would make `--install-service`
/// genuinely succeed and falsify this test's premise, not indicate a regression. Any service that
/// does get installed in that case is still cleaned up via `WindowsServiceGuard` before returning.
#[test]
fn install_and_uninstall_service_fail_cleanly_without_elevation() {
    if is_elevated() {
        eprintln!(
            "skipping: this process is Administrator-elevated (e.g. a GitHub Actions windows-latest \
             runner), so --install-service would genuinely succeed here — this test only asserts the \
             non-elevated failure path. See install_and_uninstall_service_round_trip for the elevated case."
        );
        let _guard = WindowsServiceGuard("RustcopyNotifyServer");
        let _ = Command::new(NOTIFY_SERVER_BIN)
            .arg("--install-service")
            .output();
        return;
    }

    let install = Command::new(NOTIFY_SERVER_BIN)
        .arg("--install-service")
        .output()
        .expect("run notify-server");
    assert!(!install.status.success());
    assert!(
        String::from_utf8_lossy(&install.stderr).contains("service"),
        "stderr: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let uninstall = Command::new(NOTIFY_SERVER_BIN)
        .arg("--uninstall-service")
        .output()
        .expect("run notify-server");
    assert!(!uninstall.status.success());
    assert!(
        String::from_utf8_lossy(&uninstall.stderr).contains("service"),
        "stderr: {}",
        String::from_utf8_lossy(&uninstall.stderr)
    );
}

/// Deletes the named Windows service on drop, best-effort.
struct WindowsServiceGuard(&'static str);

impl Drop for WindowsServiceGuard {
    fn drop(&mut self) {
        let _ = Command::new("sc.exe").args(["delete", self.0]).output();
    }
}

/// F41 elevated round-trip test: `--install-service` actually registers `RustcopyNotifyServer`
/// with the real Service Control Manager, and `--uninstall-service` actually removes it — a real
/// round trip, not a mock, for notify-server's own (separate from `robocopy_ingest`'s) service
/// identity. **Requires Administrator elevation** and mutates real machine state, so it's marked
/// `#[ignore]` and never runs as part of the normal suite. Run it manually from an elevated
/// prompt with:
///   cargo test --features notify-server --test notify_server_e2e -- --ignored install_and_uninstall_service_round_trip
#[test]
#[ignore = "requires Administrator elevation; run manually with `cargo test -- --ignored`"]
fn install_and_uninstall_service_round_trip() {
    const SERVICE_NAME: &str = "RustcopyNotifyServer";
    let _guard = WindowsServiceGuard(SERVICE_NAME);

    let install = Command::new(NOTIFY_SERVER_BIN)
        .arg("--install-service")
        .output()
        .expect("run notify-server");
    assert!(
        install.status.success(),
        "install must succeed when run elevated; stderr: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let query = Command::new("sc.exe")
        .args(["query", SERVICE_NAME])
        .output()
        .expect("run sc.exe query");
    assert!(
        query.status.success(),
        "sc query must find the freshly installed service; stderr: {}",
        String::from_utf8_lossy(&query.stderr)
    );

    let uninstall = Command::new(NOTIFY_SERVER_BIN)
        .arg("--uninstall-service")
        .output()
        .expect("run notify-server");
    assert!(
        uninstall.status.success(),
        "uninstall must succeed when run elevated; stderr: {}",
        String::from_utf8_lossy(&uninstall.stderr)
    );

    let query_after = Command::new("sc.exe")
        .args(["query", SERVICE_NAME])
        .output()
        .expect("run sc.exe query");
    assert!(
        !query_after.status.success(),
        "sc query must no longer find the service after uninstall"
    );
}
