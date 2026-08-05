//! F37: real Windows Service Control Manager (SCM) integration, replacing the pre-F37 mock that
//! only logged a message and did nothing.
//!
//! **Deliberately minimal scope, decided via `AskUserQuestion` before implementing**: F36 already
//! covers scheduled backups via Task Scheduler (`schedule.rs`), so this module's only job is the
//! generic install/uninstall/start/stop *infrastructure* — a real service that SCM can control.
//! Once running, the service is idle: it registers a control handler, reports `Running`, and
//! waits for a `Stop` request. What the service should actually *do* while running is left to
//! F41 (persistent notify-server), which is expected to build on this rather than duplicate it.
//!
//! There is exactly one service identity in this first cut (`SERVICE_NAME`/`SERVICE_DISPLAY_NAME`
//! below) — no `--service-name` customisation. Task Scheduler's per-task naming (F36) doesn't
//! carry over here: a Windows service's control-handler registration must use the *exact* name
//! SCM was given at `CreateService` time, and plumbing an arbitrary runtime name through the
//! service's C-callback entry point (`service_dispatcher::start`, which only receives whatever
//! `binPath` arguments SCM was configured with) adds real complexity for no concrete use case in
//! this idle-only v1. A future multi-instance need can revisit this.
//!
//! **Untested end-to-end, like `--vss-snapshot` (F30)**: `CreateService`/`StartService`/
//! `DeleteService` require real Administrator elevation and mutate real machine state (the actual
//! Windows service database), outside every other test's `tempdir`-only sandbox. Only the pure,
//! isolable logic below (`service_binary_path`) has a unit test; installing/starting/stopping a
//! real service was verified manually, not by an automated black-box test.

use std::path::Path;

/// Internal-only invocation marker: never a real clap flag, never documented in `--help`. Only
/// ever set by this crate itself, as an argument in the `binPath` of the service `install()`
/// creates — see `service_binary_path`. Checked directly against raw `std::env::args()` in
/// `main()`, before clap parsing even starts, so it works whether or not the rest of the argv
/// would otherwise parse as valid `Args`.
pub const RUN_AS_SERVICE_ARG: &str = "--run-as-service";

/// Builds the full command line SCM should launch: the given executable path plus
/// `RUN_AS_SERVICE_ARG`. Pure string construction, kept separate from `install()` below so it has
/// a unit test independent of a real Administrator-elevated `CreateService` call.
pub fn service_binary_path(exe_path: &Path) -> String {
    format!("{} {}", exe_path.display(), RUN_AS_SERVICE_ARG)
}

/// Detects whether the current process was launched with `RUN_AS_SERVICE_ARG` — i.e. by SCM
/// itself, via the `binPath` `install()` configured — by scanning the real argv directly, on
/// purpose *not* going through clap (a service launch is an internal implementation detail of how
/// this binary gets started, not a user-facing CLI mode).
pub fn is_service_launch() -> bool {
    std::env::args().any(|arg| arg == RUN_AS_SERVICE_ARG)
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::OsString;
    use std::sync::mpsc;
    use std::time::Duration;

    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState, ServiceStatus,
        ServiceType, ServiceControl, ServiceControlAccept, ServiceExitCode,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::service_dispatcher;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    use crate::errors::IngestError;

    /// The SCM-internal service key — must match exactly between `install()`, `uninstall()`, and
    /// the control-handler registration in `run()` (see this module's doc comment for why it
    /// isn't user-configurable in this first cut).
    const SERVICE_NAME: &str = "RustcopyIngestService";
    const SERVICE_DISPLAY_NAME: &str = "Rustcopy Ingest Service";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    fn to_ingest_error(context: &str, error: windows_service::Error) -> IngestError {
        IngestError::Service(format!("{context}: {error}"))
    }

    /// Registers the service with SCM, pointing its `binPath` at the current executable plus
    /// `super::RUN_AS_SERVICE_ARG`. `StartType::OnDemand` (not `Automatic`) is the deliberately
    /// conservative default for a v1 that does nothing useful yet once running — the operator can
    /// switch it to Automatic via `services.msc`/`sc config` once F41 gives it real work to do.
    /// Requires Administrator; `ServiceManager::local_computer` itself returns a clear
    /// access-denied error otherwise rather than this module pre-checking elevation itself (same
    /// "let the native call fail with its own error" approach as `vss.rs`).
    pub fn install() -> Result<(), IngestError> {
        let exe_path = std::env::current_exe()
            .map_err(|error| IngestError::Service(format!("cannot determine the current executable path: {error}")))?;

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)
            .map_err(|error| to_ingest_error("cannot connect to the Service Control Manager", error))?;

        // `executable_path` is the bare binary path; `launch_arguments` is what actually carries
        // `RUN_AS_SERVICE_ARG` — the crate itself is responsible for correctly quoting/joining
        // these into the registry's `lpBinaryPathName`, which is safer than this module hand-
        // building that combined string itself (a path containing spaces would need exactly the
        // right quoting to avoid SCM misparsing where the executable name ends and args begin).
        let service_info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: SERVICE_TYPE,
            start_type: ServiceStartType::OnDemand,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe_path,
            launch_arguments: vec![OsString::from(super::RUN_AS_SERVICE_ARG)],
            dependencies: vec![],
            account_name: None, // LocalSystem
            account_password: None,
        };

        manager
            .create_service(&service_info, ServiceAccess::empty())
            .map_err(|error| to_ingest_error("cannot create the Windows service", error))?;
        Ok(())
    }

    /// Removes the service. If it's currently running, SCM marks it for deletion on next stop
    /// rather than deleting immediately — that's native `DeleteService` behaviour, not something
    /// this function works around; document it to the operator instead.
    pub fn uninstall() -> Result<(), IngestError> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .map_err(|error| to_ingest_error("cannot connect to the Service Control Manager", error))?;
        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::DELETE)
            .map_err(|error| to_ingest_error("cannot open the Windows service (is it installed?)", error))?;
        service
            .delete()
            .map_err(|error| to_ingest_error("cannot delete the Windows service", error))?;
        Ok(())
    }

    /// Blocks the calling OS thread, dispatching SCM control events, until the service is asked
    /// to stop. Must be called from a plain thread — not from inside a tokio runtime worker —
    /// which is why `main()` checks `super::is_service_launch()` before ever building the tokio
    /// `Runtime` at all (see `main.rs`).
    pub fn run_service_dispatcher() -> Result<(), IngestError> {
        service_dispatcher::start(SERVICE_NAME, service_main)
            .map_err(|error| to_ingest_error("service dispatcher failed", error))
    }

    windows_service::define_windows_service!(service_main, run_idle_service);

    /// The service's actual body: registers a `Stop`-only control handler, reports `Running`,
    /// blocks until `Stop` arrives, reports `Stopped`. Deliberately does nothing else — see this
    /// module's doc comment for why.
    fn run_idle_service(_arguments: Vec<OsString>) {
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    let _ = shutdown_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        let status_handle = match service_control_handler::register(SERVICE_NAME, event_handler) {
            Ok(handle) => handle,
            Err(error) => {
                tracing::error!(error = %error, "cannot register the service control handler");
                return;
            }
        };

        let report = |state: ServiceState, controls_accepted: ServiceControlAccept| {
            let _ = status_handle.set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: state,
                controls_accepted,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            });
        };

        report(ServiceState::Running, ServiceControlAccept::STOP);
        tracing::info!("service running (idle — see service.rs's doc comment for why)");

        // Blocks this thread until the control handler above signals a Stop request.
        let _ = shutdown_rx.recv();

        tracing::info!("service stopping");
        report(ServiceState::Stopped, ServiceControlAccept::empty());
    }
}

#[cfg(windows)]
pub use windows_impl::{install, run_service_dispatcher, uninstall};

#[cfg(not(windows))]
pub fn install() -> Result<(), crate::errors::IngestError> {
    Err(crate::errors::IngestError::Service(
        "--install-service requires the Windows Service Control Manager, unavailable on this platform".to_string(),
    ))
}

#[cfg(not(windows))]
pub fn uninstall() -> Result<(), crate::errors::IngestError> {
    Err(crate::errors::IngestError::Service(
        "--uninstall-service requires the Windows Service Control Manager, unavailable on this platform"
            .to_string(),
    ))
}

#[cfg(not(windows))]
pub fn run_service_dispatcher() -> Result<(), crate::errors::IngestError> {
    Err(crate::errors::IngestError::Service(
        "service mode requires the Windows Service Control Manager, unavailable on this platform".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn service_binary_path_appends_the_run_as_service_marker() {
        let exe = PathBuf::from("C:\\Program Files\\rustcopy\\robocopy_ingest.exe");
        assert_eq!(
            service_binary_path(&exe),
            "C:\\Program Files\\rustcopy\\robocopy_ingest.exe --run-as-service"
        );
    }

    #[test]
    fn is_service_launch_is_false_under_the_normal_test_argv() {
        // The test harness's own argv never contains the marker.
        assert!(!is_service_launch());
    }
}
