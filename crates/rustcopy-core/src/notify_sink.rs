//! Notification channel abstraction for the `notify-server` binary.
//!
//! Kept in the library crate (not gated behind the `notify-server` feature) so it compiles and
//! is unit-tested on every platform, the same way [`crate::engine::robocopy::CommandRunner`] and
//! [`crate::progress::ProgressSink`] are: the axum HTTP surface (feature-gated, in
//! `src/bin/notify_server.rs`) only has to dispatch to a `dyn NotificationSink`, never construct
//! one from scratch in a test.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::notify::WebhookPayload;

/// A channel failed to deliver a notification.
#[derive(Debug, thiserror::Error)]
#[error("{sink}: {message}")]
pub struct NotifyError {
    pub sink: &'static str,
    pub message: String,
}

/// One notification destination (log, ntfy, a generic Slack/Teams-style webhook, ...).
///
/// Modelled on [`crate::engine::robocopy::CommandRunner`] and [`crate::progress::ProgressSink`]:
/// production code depends on `dyn NotificationSink`, so tests can substitute a scripted double
/// (see `ScriptedSink` below) instead of hitting a real network endpoint.
#[async_trait]
pub trait NotificationSink: Send + Sync {
    /// Stable identifier used in logs and in [`NotifyError`].
    fn name(&self) -> &'static str;

    /// Deliver `payload`. Delivery is synchronous from the caller's point of view: the HTTP
    /// handler in `notify_server` only answers the original POST request once every configured
    /// sink has been tried, so a "delivered" response is never sent for a notification that
    /// silently failed to reach its destination.
    async fn deliver(&self, payload: &WebhookPayload) -> Result<(), NotifyError>;
}

/// Builds an HTTP client with `timeout` applied, logging rather than discarding a builder failure.
///
/// `Client::builder().timeout(t).build().unwrap_or_default()` looked harmless and was not:
/// `Client::default()` carries **no request timeout at all**, so a builder failure silently traded
/// a bounded request for an unbounded one. Since `notify_server::notify_handler` awaits every sink
/// before answering the original POST, one unreachable endpoint would then hold that request open
/// indefinitely.
///
/// The fallback is kept — a sink that cannot be built at all would be worse — but the deadline no
/// longer depends on it: [`NtfySink::deliver`] and [`GenericWebhookSink::deliver`] wrap the request
/// in `tokio::time::timeout` regardless, so the bound holds on every path.
fn build_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|error| {
            tracing::warn!(
                %error,
                "could not build an HTTP client carrying the configured timeout; the deadline is                  enforced at the call site instead"
            );
            reqwest::Client::default()
        })
}

/// Bounds `future` by `timeout`, turning an overrun into a `NotifyError` rather than a hang.
///
/// Applied by both HTTP sinks unconditionally. `reqwest`'s own timeout usually does the job, but
/// it is not guaranteed to be present (see [`build_client`]), and the caller that matters here —
/// `notify_server::notify_handler` — awaits every sink before it answers the original POST.
async fn with_deadline<F>(
    timeout: Duration,
    sink: &'static str,
    future: F,
) -> Result<(), NotifyError>
where
    F: std::future::Future<Output = Result<(), NotifyError>>,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(result) => result,
        Err(_) => Err(NotifyError {
            sink,
            message: format!("no response within {timeout:?}"),
        }),
    }
}

/// Always-available sink that logs the event via `tracing`. Useful on its own (a plain log line
/// per backup run) and as a fallback when no other channel is configured.
#[derive(Debug, Default)]
pub struct LogSink;

#[async_trait]
impl NotificationSink for LogSink {
    fn name(&self) -> &'static str {
        "log"
    }

    async fn deliver(&self, payload: &WebhookPayload) -> Result<(), NotifyError> {
        match payload.status {
            crate::notify::BackupStatus::Success => {
                tracing::info!(
                    source = %payload.source,
                    dest = %payload.dest,
                    host = %payload.host,
                    "backup notification: success"
                );
            }
            crate::notify::BackupStatus::Failed => {
                tracing::warn!(
                    source = %payload.source,
                    dest = %payload.dest,
                    host = %payload.host,
                    exit_code = ?payload.exit_code,
                    integrity_status = ?payload.integrity_status,
                    "backup notification: failed"
                );
            }
        }
        Ok(())
    }
}

/// Posts a plain-text summary to an [ntfy](https://ntfy.sh) topic URL.
pub struct NtfySink {
    pub topic_url: String,
    client: reqwest::Client,
    /// Enforced by `deliver` via `tokio::time::timeout`, not only by the client — see
    /// [`build_client`] for why the client alone is not a guarantee.
    timeout: Duration,
}

impl NtfySink {
    pub fn new(topic_url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            topic_url: topic_url.into(),
            client: build_client(timeout),
            timeout,
        }
    }
}

#[async_trait]
impl NotificationSink for NtfySink {
    fn name(&self) -> &'static str {
        "ntfy"
    }

    async fn deliver(&self, payload: &WebhookPayload) -> Result<(), NotifyError> {
        with_deadline(self.timeout, "ntfy", self.send(payload)).await
    }
}

impl NtfySink {
    async fn send(&self, payload: &WebhookPayload) -> Result<(), NotifyError> {
        let body = format!("{}\n{}", payload.text, payload.report_summary);
        let response = self
            .client
            .post(&self.topic_url)
            .header("Title", format!("robocopy-ingest: {}", payload.status))
            .body(body)
            .send()
            .await
            .map_err(|e| NotifyError {
                sink: "ntfy",
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(NotifyError {
                sink: "ntfy",
                message: format!("endpoint returned {}", response.status()),
            });
        }
        Ok(())
    }
}

/// Posts the full [`WebhookPayload`] JSON to a generic webhook endpoint (Slack incoming webhook,
/// Microsoft Teams connector, or any other JSON-POST-based integration).
pub struct GenericWebhookSink {
    pub url: String,
    client: reqwest::Client,
    /// Enforced by `deliver` via `tokio::time::timeout`, not only by the client — see
    /// [`build_client`] for why the client alone is not a guarantee.
    timeout: Duration,
}

impl GenericWebhookSink {
    pub fn new(url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            url: url.into(),
            client: build_client(timeout),
            timeout,
        }
    }
}

#[async_trait]
impl NotificationSink for GenericWebhookSink {
    fn name(&self) -> &'static str {
        "generic_webhook"
    }

    async fn deliver(&self, payload: &WebhookPayload) -> Result<(), NotifyError> {
        with_deadline(self.timeout, "generic-webhook", self.send(payload)).await
    }
}

impl GenericWebhookSink {
    async fn send(&self, payload: &WebhookPayload) -> Result<(), NotifyError> {
        let response = self
            .client
            .post(&self.url)
            .json(payload)
            .send()
            .await
            .map_err(|e| NotifyError {
                sink: "generic_webhook",
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(NotifyError {
                sink: "generic_webhook",
                message: format!("endpoint returned {}", response.status()),
            });
        }
        Ok(())
    }
}

/// TOML configuration for `notify-server` (`notify-server.toml`), mirroring the
/// [`crate::config::IngestConfig`] pattern: every field optional, deserialized with `serde`.
///
/// Channel secrets (ntfy topic being effectively a bearer token if private, webhook URLs with
/// embedded tokens) belong in this file or in environment variables, never hardcoded — this
/// mirrors the project's existing rule for `--encrypt-aes256` keys (`crypto::resolve_key`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NotifyServerConfig {
    /// Address to bind to, e.g. `"127.0.0.1:3000"`. Defaults to loopback when absent.
    pub bind: Option<String>,
    pub ntfy: Option<NtfyChannelConfig>,
    pub generic_webhook: Option<GenericWebhookChannelConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NtfyChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    pub topic_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenericWebhookChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    pub url: String,
}

impl NotifyServerConfig {
    pub fn load_from(path: &std::path::Path) -> Result<Self, crate::errors::IngestError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| crate::errors::IngestError::io(path, e))?;
        toml::from_str(&content).map_err(|e| {
            crate::errors::IngestError::io(
                path,
                std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            )
        })
    }

    /// Build the configured sinks. `LogSink` is always included, so a run is never silent even
    /// with no config file (or an empty one).
    pub fn build_sinks(&self, timeout: Duration) -> Vec<Box<dyn NotificationSink>> {
        let mut sinks: Vec<Box<dyn NotificationSink>> = vec![Box::new(LogSink)];

        if let Some(ntfy) = &self.ntfy {
            if ntfy.enabled {
                sinks.push(Box::new(NtfySink::new(ntfy.topic_url.clone(), timeout)));
            }
        }
        if let Some(webhook) = &self.generic_webhook {
            if webhook.enabled {
                sinks.push(Box::new(GenericWebhookSink::new(
                    webhook.url.clone(),
                    timeout,
                )));
            }
        }
        sinks
    }
}

/// Dispatch `payload` to every sink, returning `Err` listing every sink that failed (an empty
/// `Vec` means full success). Every sink is tried even if an earlier one fails, so one broken
/// channel doesn't hide the result of the others.
pub async fn dispatch_to_all(
    sinks: &[Box<dyn NotificationSink>],
    payload: &WebhookPayload,
) -> Vec<NotifyError> {
    let mut failures = Vec::new();
    for sink in sinks {
        if let Err(error) = sink.deliver(payload).await {
            tracing::error!(sink = sink.name(), error = %error, "notification sink failed");
            failures.push(error);
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::BackupStatus;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn sample_payload(status: BackupStatus) -> WebhookPayload {
        WebhookPayload {
            schema_version: crate::notify::NOTIFY_SCHEMA_VERSION,
            text: "test event".to_string(),
            report_summary: "1 file, 100 bytes".to_string(),
            status,
            files_copied: 1,
            bytes_copied: 100,
            elapsed_seconds: 1.0,
            source: "D:/landing".to_string(),
            dest: "E:/warehouse".to_string(),
            host: "srv01".to_string(),
            tool_version: "5.1.0".to_string(),
            exit_code: Some(0),
            integrity_status: Some("PASSED".to_string()),
        }
    }

    /// [`NotificationSink`] double that records every payload it receives and can be scripted to
    /// fail, mirroring [`crate::testkit::ScriptedRunner`].
    struct ScriptedSink {
        name: &'static str,
        should_fail: bool,
        calls: Mutex<Vec<WebhookPayload>>,
        call_count: AtomicUsize,
    }

    impl ScriptedSink {
        fn new(name: &'static str, should_fail: bool) -> Self {
            Self {
                name,
                should_fail,
                calls: Mutex::new(Vec::new()),
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl NotificationSink for ScriptedSink {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn deliver(&self, payload: &WebhookPayload) -> Result<(), NotifyError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            self.calls.lock().expect("lock").push(payload.clone());
            if self.should_fail {
                Err(NotifyError {
                    sink: self.name,
                    message: "scripted failure".to_string(),
                })
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn log_sink_never_fails() {
        let sink = LogSink;
        assert!(sink
            .deliver(&sample_payload(BackupStatus::Success))
            .await
            .is_ok());
        assert!(sink
            .deliver(&sample_payload(BackupStatus::Failed))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn dispatch_tries_every_sink_even_after_a_failure() {
        let failing = ScriptedSink::new("failing", true);
        let ok_sink = ScriptedSink::new("ok", false);

        // We need owned references for the trait objects but also want to inspect call counts
        // afterward, so wrap in Arc and clone the trait-object Vec from references.
        let failing = std::sync::Arc::new(failing);
        let ok_sink = std::sync::Arc::new(ok_sink);

        struct ArcSink<T>(std::sync::Arc<T>);
        #[async_trait]
        impl<T: NotificationSink> NotificationSink for ArcSink<T> {
            fn name(&self) -> &'static str {
                self.0.name()
            }
            async fn deliver(&self, payload: &WebhookPayload) -> Result<(), NotifyError> {
                self.0.deliver(payload).await
            }
        }

        let sinks: Vec<Box<dyn NotificationSink>> = vec![
            Box::new(ArcSink(std::sync::Arc::clone(&failing))),
            Box::new(ArcSink(std::sync::Arc::clone(&ok_sink))),
        ];

        let failures = dispatch_to_all(&sinks, &sample_payload(BackupStatus::Success)).await;

        assert_eq!(failures.len(), 1, "exactly one sink must have failed");
        assert_eq!(failures[0].sink, "failing");
        assert_eq!(failing.calls(), 1, "the failing sink must still be called");
        assert_eq!(
            ok_sink.calls(),
            1,
            "a later sink must run despite an earlier failure"
        );
    }

    #[test]
    fn config_always_includes_the_log_sink() {
        let config = NotifyServerConfig::default();
        let sinks = config.build_sinks(Duration::from_secs(5));
        assert_eq!(sinks.len(), 1);
        assert_eq!(sinks[0].name(), "log");
    }

    #[test]
    fn config_adds_enabled_channels_only() {
        let config = NotifyServerConfig {
            bind: None,
            ntfy: Some(NtfyChannelConfig {
                enabled: true,
                topic_url: "https://ntfy.sh/mytopic".to_string(),
            }),
            generic_webhook: Some(GenericWebhookChannelConfig {
                enabled: false,
                url: "https://example.invalid/hook".to_string(),
            }),
        };
        let sinks = config.build_sinks(Duration::from_secs(5));
        let names: Vec<_> = sinks.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            vec!["log", "ntfy"],
            "disabled channels must not be built"
        );
    }

    #[test]
    fn config_parses_from_toml() {
        let toml_str = r#"
            bind = "127.0.0.1:4000"

            [ntfy]
            enabled = true
            topic_url = "https://ntfy.sh/my-backups"
        "#;
        let config: NotifyServerConfig = toml::from_str(toml_str).expect("valid toml");
        assert_eq!(config.bind, Some("127.0.0.1:4000".to_string()));
        assert!(config.ntfy.expect("ntfy section").enabled);
    }
    /// The deadline must hold even when the future never resolves. Before this, a sink whose
    /// client had lost its timeout (see `build_client`) would await forever, and
    /// `notify_server::notify_handler` awaits every sink before answering the original POST — so
    /// one unreachable endpoint held that request open indefinitely.
    #[tokio::test]
    async fn with_deadline_turns_a_hang_into_an_error() {
        let never = async {
            // Longer than any plausible test run; the deadline must fire first.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok(())
        };

        let started = std::time::Instant::now();
        let result = with_deadline(Duration::from_millis(50), "ntfy", never).await;
        let elapsed = started.elapsed();

        let error = result.expect_err("a future that never resolves must not report success");
        assert_eq!(error.sink, "ntfy");
        assert!(
            error.message.contains("no response within"),
            "the error must say it timed out, got: {}",
            error.message
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "it must return at the deadline, not wait out the future; took {elapsed:?}"
        );
    }

    /// A future that completes inside the deadline passes through untouched, error included.
    #[tokio::test]
    async fn with_deadline_does_not_interfere_with_a_prompt_result() {
        let ok = with_deadline(Duration::from_secs(30), "ntfy", async { Ok(()) }).await;
        assert!(ok.is_ok());

        let failed = with_deadline(Duration::from_secs(30), "ntfy", async {
            Err(NotifyError {
                sink: "ntfy",
                message: "endpoint returned 500".to_string(),
            })
        })
        .await;
        assert_eq!(
            failed.expect_err("propagated").message,
            "endpoint returned 500",
            "the sink's own error must survive, not be replaced by a timeout"
        );
    }
}
