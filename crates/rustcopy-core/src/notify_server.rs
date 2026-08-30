//! HTTP surface for the `notify-server` binary: an axum [`Router`] that receives
//! [`crate::notify::WebhookPayload`] POSTs (the same payload `--webhook-url` sends) and dispatches
//! them to the configured [`crate::notify_sink::NotificationSink`]s.
//!
//! Kept in the library, gated behind the `notify-server` feature, so integration tests can build
//! and drive a real [`Router`] over a real TCP socket on an ephemeral port instead of only calling
//! internal functions directly — the same mistake that let D1 (`--restore-from` unreachable via
//! clap) survive 140 passing tests, none of which ran the compiled binary with real arguments.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Json, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;

use crate::notify::{WebhookPayload, NOTIFY_SCHEMA_VERSION};
use crate::notify_sink::NotificationSink;

/// Body size cap for `POST /notify`. A real payload is a few hundred bytes to a few KB; this is
/// generous headroom without leaving the endpoint open to an unbounded body.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

pub struct AppState {
    /// When `Some`, `POST /notify` requires `Authorization: Bearer <token>` to match exactly.
    pub token: Option<String>,
    pub sinks: Vec<Box<dyn NotificationSink>>,
}

/// Build the router with `state` already attached (`Router<()>`, ready for `axum::serve`).
pub fn build_router(state: AppState) -> Router {
    let state = Arc::new(state);
    Router::new()
        .route("/health", get(health_handler))
        .route("/notify", post(notify_handler))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "schema_version": NOTIFY_SCHEMA_VERSION,
    }))
}

fn is_authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(expected) = &state.token else {
        return true; // no token configured: authentication is not required (loopback-only use).
    };
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.strip_prefix("Bearer ").unwrap_or(value))
        .is_some_and(|presented| presented == expected)
}

async fn notify_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    payload: Result<Json<WebhookPayload>, JsonRejection>,
) -> Response {
    if !is_authorized(&state, &headers) {
        tracing::warn!("rejected /notify: missing or incorrect bearer token");
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let payload = match payload {
        Ok(Json(payload)) => payload,
        Err(rejection) => {
            tracing::warn!(error = %rejection, "rejected /notify: malformed payload");
            return (StatusCode::UNPROCESSABLE_ENTITY, rejection.body_text()).into_response();
        }
    };

    tracing::info!(
        status = %payload.status,
        source = %payload.source,
        dest = %payload.dest,
        host = %payload.host,
        schema_version = payload.schema_version,
        "notification received"
    );

    let failures = crate::notify_sink::dispatch_to_all(&state.sinks, &payload).await;
    if failures.is_empty() {
        StatusCode::OK.into_response()
    } else {
        let message = failures
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        (StatusCode::BAD_GATEWAY, message).into_response()
    }
}

/// Refuses a configuration that would expose the endpoint on a non-loopback address without
/// authentication: an unauthenticated `/notify` reachable from the network lets anyone on the
/// LAN inject fabricated backup notifications (or trigger whatever the configured sinks do).
pub fn check_bind_security(addr: &SocketAddr, token: &Option<String>) -> Result<(), String> {
    if !addr.ip().is_loopback() && token.is_none() {
        return Err(format!(
            "refusing to bind {addr}: it is not a loopback address and no auth token is \
             configured (set ROBOCOPY_NOTIFY_TOKEN). Binding an unauthenticated /notify endpoint \
             to a network-reachable address would let anyone on the network inject fabricated \
             backup notifications."
        ));
    }
    Ok(())
}

/// Serve `router` on `listener` until a graceful-shutdown signal (Ctrl+C / SIGTERM) fires,
/// draining in-flight requests before returning.
pub async fn serve_until_shutdown(
    listener: tokio::net::TcpListener,
    router: Router,
) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

/// Same as [`serve_until_shutdown`], but also shuts down gracefully when `extra_signal` resolves —
/// used by `src/bin/notify_server.rs`'s F41 Windows-service path, where SCM's `Stop` request has
/// to trigger the same graceful drain as Ctrl+C/SIGTERM do for the normal foreground run. Kept as
/// a separate function (not a parameter added to `serve_until_shutdown`) so the existing, already
/// covered Ctrl+C/SIGTERM-only path is untouched — a `Future` that never resolves (e.g.
/// `std::future::pending()`) is a valid `extra_signal` for callers that don't have anything extra
/// to wait on.
pub async fn serve_until_shutdown_or(
    listener: tokio::net::TcpListener,
    router: Router,
    extra_signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            tokio::select! {
                _ = shutdown_signal() => {}
                _ = extra_signal => {}
            }
        })
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::BackupStatus;
    use crate::notify_sink::{LogSink, NotifyError};
    use async_trait::async_trait;

    fn sample_payload() -> WebhookPayload {
        WebhookPayload {
            schema_version: NOTIFY_SCHEMA_VERSION,
            text: "test".to_string(),
            report_summary: "summary".to_string(),
            status: BackupStatus::Success,
            files_copied: 1,
            bytes_copied: 10,
            elapsed_seconds: 1.0,
            source: "C:/data".to_string(),
            dest: "E:/backup".to_string(),
            host: "srv01".to_string(),
            tool_version: "5.1.0".to_string(),
            exit_code: Some(0),
            integrity_status: Some("PASSED".to_string()),
        }
    }

    struct AlwaysFailingSink;
    #[async_trait]
    impl NotificationSink for AlwaysFailingSink {
        fn name(&self) -> &'static str {
            "always_failing"
        }
        async fn deliver(&self, _payload: &WebhookPayload) -> Result<(), NotifyError> {
            Err(NotifyError {
                sink: "always_failing",
                message: "simulated failure".to_string(),
            })
        }
    }

    /// Spins up the real router on a real loopback TCP socket (ephemeral port) so these tests
    /// exercise actual HTTP + JSON (de)serialization, not just the handler function in isolation.
    async fn spawn_test_server(state: AppState) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        let router = build_router(state);
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.ok();
        });
        (addr, handle)
    }

    #[tokio::test]
    async fn health_endpoint_reports_schema_version() {
        let (addr, handle) = spawn_test_server(AppState {
            token: None,
            sinks: vec![Box::new(LogSink)],
        })
        .await;

        let response = reqwest::get(format!("http://{addr}/health"))
            .await
            .expect("health request");
        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(body["schema_version"], NOTIFY_SCHEMA_VERSION);

        handle.abort();
    }

    #[tokio::test]
    async fn notify_succeeds_with_no_token_configured() {
        let (addr, handle) = spawn_test_server(AppState {
            token: None,
            sinks: vec![Box::new(LogSink)],
        })
        .await;

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/notify"))
            .json(&sample_payload())
            .send()
            .await
            .expect("notify request");
        assert_eq!(response.status(), 200);

        handle.abort();
    }

    #[tokio::test]
    async fn notify_rejects_missing_token_with_401() {
        let (addr, handle) = spawn_test_server(AppState {
            token: Some("secret-token".to_string()),
            sinks: vec![Box::new(LogSink)],
        })
        .await;

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/notify"))
            .json(&sample_payload())
            .send()
            .await
            .expect("notify request");
        assert_eq!(response.status(), 401);

        handle.abort();
    }

    #[tokio::test]
    async fn notify_rejects_wrong_token_with_401() {
        let (addr, handle) = spawn_test_server(AppState {
            token: Some("secret-token".to_string()),
            sinks: vec![Box::new(LogSink)],
        })
        .await;

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/notify"))
            .header("Authorization", "Bearer wrong-token")
            .json(&sample_payload())
            .send()
            .await
            .expect("notify request");
        assert_eq!(response.status(), 401);

        handle.abort();
    }

    #[tokio::test]
    async fn notify_accepts_the_correct_bearer_token() {
        let (addr, handle) = spawn_test_server(AppState {
            token: Some("secret-token".to_string()),
            sinks: vec![Box::new(LogSink)],
        })
        .await;

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/notify"))
            .header("Authorization", "Bearer secret-token")
            .json(&sample_payload())
            .send()
            .await
            .expect("notify request");
        assert_eq!(response.status(), 200);

        handle.abort();
    }

    #[tokio::test]
    async fn notify_rejects_malformed_json_with_422() {
        let (addr, handle) = spawn_test_server(AppState {
            token: None,
            sinks: vec![Box::new(LogSink)],
        })
        .await;

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/notify"))
            .header("Content-Type", "application/json")
            .body("{ this is not valid json")
            .send()
            .await
            .expect("notify request");
        assert_eq!(response.status(), 422);

        handle.abort();
    }

    #[tokio::test]
    async fn notify_reports_502_when_a_sink_fails() {
        let (addr, handle) = spawn_test_server(AppState {
            token: None,
            sinks: vec![Box::new(LogSink), Box::new(AlwaysFailingSink)],
        })
        .await;

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/notify"))
            .json(&sample_payload())
            .send()
            .await
            .expect("notify request");
        assert_eq!(response.status(), 502);
        let body = response.text().await.expect("body");
        assert!(body.contains("always_failing"), "got: {body}");

        handle.abort();
    }

    #[test]
    fn bind_security_check_allows_loopback_without_token() {
        let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        assert!(check_bind_security(&addr, &None).is_ok());
    }

    #[test]
    fn bind_security_check_refuses_non_loopback_without_token() {
        let addr: SocketAddr = "0.0.0.0:3000".parse().unwrap();
        assert!(check_bind_security(&addr, &None).is_err());
    }

    #[test]
    fn bind_security_check_allows_non_loopback_with_token() {
        let addr: SocketAddr = "0.0.0.0:3000".parse().unwrap();
        assert!(check_bind_security(&addr, &Some("token".to_string())).is_ok());
    }
}
