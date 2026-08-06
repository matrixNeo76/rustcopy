//! F37/F41: real Windows Service Control Manager (SCM) integration, replacing the pre-F37 mock
//! that only logged a message and did nothing.
//!
//! **F37, deliberately minimal scope, decided via `AskUserQuestion` before implementing**: F36
//! already covers scheduled backups via Task Scheduler (`schedule.rs`), so this module's original
//! job was just the generic install/uninstall/start/stop *infrastructure* for `robocopy_ingest`'s
//! own service (`install()`/`uninstall()`/`run_service_dispatcher()` below), which stays idle once
//! running — it registers a control handler, reports `Running`, and waits for `Stop`.
//!
//! **F41 generalised the SCM plumbing** (`install_named`/`uninstall_named`/`start_dispatcher`/
//! `register_and_wait_for_stop`/`ServiceStatusHandle`) so `notify-server` could get its **own**
//!, separate service identity without duplicating the `CreateService`/`DeleteService`/control-
//! handler boilerplate. **Architectural decision made via `AskUserQuestion` before implementing**:
//! notify-server registers itself as `"RustcopyNotifyServer"`, distinct from `robocopy_ingest`'s
//! `"RustcopyIngestService"` — two separate Windows services, each independently
//! installable/removable/start/stoppable, rather than routing notify-server's real work through
//! `robocopy_ingest`'s idle service (which would require the default `robocopy_ingest` binary to
//! conditionally carry an axum dependency, violating the "notify-server stays feature-gated" rule
//! — see `AGENTS.md` rule 8). This module itself has **no axum dependency either way**: the actual
//! axum-hosting logic for notify-server's service lives entirely in
//! `src/bin/notify_server.rs`, which is only ever compiled with the `notify-server` feature on.
//!
//! There is exactly one *fixed* identity per binary (no `--service-name` customisation for
//! either) — a Windows service's control-handler registration must use the exact name SCM was
//! given at `CreateService` time, and plumbing an arbitrary *runtime* name through
//! `service_dispatcher::start`'s C-callback entry point (which only receives whatever `binPath`
//! arguments SCM was configured with) would add real complexity for no concrete multi-instance use
//! case today. A future need for multiple named instances of the same binary can revisit this.
//!
//! **Untested end-to-end, like `--vss-snapshot` (F30)**: `CreateService`/`StartService`/
//! `DeleteService` require real Administrator elevation and mutate real machine state (the actual
//! Windows service database), outside every other test's `tempdir`-only sandbox. Only the pure,
//! isolable logic below (`service_binary_path`) has a unit test; installing/starting/stopping a
//! real service was verified manually, not by an automated black-box test.

use std::path::Path;

/// Internal-only invocation marker: never a real clap flag, never documented in `--help`. Only
/// ever set by this crate itself, as the first launch argument of whatever service `install`/
/// `install_named` creates — see `service_binary_path`. Checked directly against raw
/// `std::env::args()` in each binary's `main()`, before clap parsing even starts, so it works
/// whether or not the rest of the argv would otherwise parse as valid `Args`. Shared by both
/// `robocopy_ingest` and `notify-server` — it only means "SCM launched this process", not which
/// service.
pub const RUN_AS_SERVICE_ARG: &str = "--run-as-service";

/// Builds the full command line SCM should launch: the given executable path plus
/// `RUN_AS_SERVICE_ARG`. Pure string construction, kept separate from `install()` below so it has
/// a unit test independent of a real Administrator-elevated `CreateService` call. (The real
/// `install()`/`install_named()` don't actually use this string form — they pass the executable
/// path and arguments as separate `ServiceInfo` fields, which the `windows-service` crate quotes
/// correctly; this function exists to describe/log the effective command line, and for the test
/// below.)
pub fn service_binary_path(exe_path: &Path) -> String {
    format!("{} {}", exe_path.display(), RUN_AS_SERVICE_ARG)
}

/// Detects whether the current process was launched with `RUN_AS_SERVICE_ARG` — i.e. by SCM
/// itself — by scanning the real argv directly, on purpose *not* going through clap (a service
/// launch is an internal implementation detail of how a binary gets started, not a user-facing CLI
/// mode).
pub fn is_service_launch() -> bool {
    std::env::args().any(|arg| arg == RUN_AS_SERVICE_ARG)
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::OsString;
    use std::sync::mpsc;
    use std::time::Duration;

    use windows_service::service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::service_dispatcher;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    use crate::errors::IngestError;

    /// `robocopy_ingest`'s own fixed service identity (F37) — see this module's doc comment for
    /// why it stays idle and why it's a separate identity from notify-server's (F41).
    const SERVICE_NAME: &str = "RustcopyIngestService";
    const SERVICE_DISPLAY_NAME: &str = "Rustcopy Ingest Service";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    fn to_ingest_error(context: &str, error: windows_service::Error) -> IngestError {
        IngestError::Service(format!("{context}: {error}"))
    }

    /// Registers `robocopy_ingest`'s own service. Thin wrapper over `install_named` with the
    /// fixed identity above and no extra launch arguments — kept as a zero-argument function so
    /// `main.rs`'s call site never had to change across F37/F41.
    pub fn install() -> Result<(), IngestError> {
        install_named(SERVICE_NAME, SERVICE_DISPLAY_NAME, vec![])
    }

    /// Removes `robocopy_ingest`'s own service.
    pub fn uninstall() -> Result<(), IngestError> {
        uninstall_named(SERVICE_NAME)
    }

    /// Blocks the calling OS thread, dispatching SCM control events for `robocopy_ingest`'s own
    /// service, until it is asked to stop. Must be called from a plain thread — not from inside a
    /// tokio runtime worker — which is why `main()` checks `super::is_service_launch()` before
    /// ever building the tokio `Runtime` at all (see `main.rs`).
    pub fn run_service_dispatcher() -> Result<(), IngestError> {
        start_dispatcher(SERVICE_NAME, service_main)
    }

    windows_service::define_windows_service!(service_main, run_idle_service);

    /// `robocopy_ingest`'s own service body: registers, reports `Running`, blocks until `Stop`,
    /// reports `Stopped`. Deliberately does nothing else — see this module's doc comment for why.
    /// Written directly against `register_and_wait_for_stop`/`ServiceStatusHandle` below, both as
    /// a dogfooding check that the generalised (F41) API is sufficient and to avoid a second
    /// hand-rolled copy of the same control-handler boilerplate.
    fn run_idle_service(_arguments: Vec<OsString>) {
        let (status_handle, stop_rx) = match register_and_wait_for_stop(SERVICE_NAME) {
            Ok(pair) => pair,
            Err(error) => {
                tracing::error!(error = %error, "cannot register the service control handler");
                return;
            }
        };

        tracing::info!("service running (idle — see service.rs's doc comment for why)");
        let _ = stop_rx.recv();

        tracing::info!("service stopping");
        status_handle.report_stopped();
    }

    /// Registers `service_key` with SCM (`display_name`, `OwnProcess`, `OnDemand` start type —
    /// not `Automatic`, since neither `robocopy_ingest`'s idle service nor a first F41 cut of
    /// notify-server's should silently start running before the operator confirms it does what
    /// they expect). `executable_path` is the bare current-executable path; `RUN_AS_SERVICE_ARG`
    /// is always prepended to `extra_launch_arguments` automatically, so every caller's service
    /// launches with the same marker `is_service_launch()` checks for. `launch_arguments` is kept
    /// as a separate `ServiceInfo` field rather than a hand-built combined string — the
    /// `windows-service` crate is responsible for correctly quoting/joining these into the
    /// registry's `lpBinaryPathName`, which is safer than this module doing that quoting itself
    /// (an executable path or argument containing spaces needs exactly the right quoting to avoid
    /// SCM misparsing where the executable name ends and arguments begin). Requires
    /// Administrator; `ServiceManager::local_computer` itself returns a clear access-denied error
    /// otherwise rather than this module pre-checking elevation itself (same "let the native call
    /// fail with its own error" approach as `vss.rs`).
    pub fn install_named(
        service_key: &str,
        display_name: &str,
        extra_launch_arguments: Vec<OsString>,
    ) -> Result<(), IngestError> {
        let exe_path = std::env::current_exe().map_err(|error| {
            IngestError::Service(format!(
                "cannot determine the current executable path: {error}"
            ))
        })?;

        let manager =
            ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)
                .map_err(|error| {
                    to_ingest_error("cannot connect to the Service Control Manager", error)
                })?;

        let mut launch_arguments = vec![OsString::from(super::RUN_AS_SERVICE_ARG)];
        launch_arguments.extend(extra_launch_arguments);

        let service_info = ServiceInfo {
            name: OsString::from(service_key),
            display_name: OsString::from(display_name),
            service_type: SERVICE_TYPE,
            start_type: ServiceStartType::OnDemand,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe_path,
            launch_arguments,
            dependencies: vec![],
            account_name: None, // LocalSystem
            account_password: None,
        };

        manager
            .create_service(&service_info, ServiceAccess::empty())
            .map_err(|error| to_ingest_error("cannot create the Windows service", error))?;
        Ok(())
    }

    /// Removes the named service. If it's currently running, SCM marks it for deletion on next
    /// stop rather than deleting immediately — that's native `DeleteService` behaviour, not
    /// something this function works around; document it to the operator instead.
    pub fn uninstall_named(service_key: &str) -> Result<(), IngestError> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .map_err(|error| {
            to_ingest_error("cannot connect to the Service Control Manager", error)
        })?;
        let service = manager
            .open_service(service_key, ServiceAccess::DELETE)
            .map_err(|error| {
                to_ingest_error("cannot open the Windows service (is it installed?)", error)
            })?;
        service
            .delete()
            .map_err(|error| to_ingest_error("cannot delete the Windows service", error))?;
        Ok(())
    }

    /// Thin wrapper over `service_dispatcher::start`: blocks the calling OS thread, dispatching
    /// SCM control events for `service_key`, until the service stops. `ffi_service_main` is the
    /// raw `extern "system"` callback `windows_service::define_windows_service!` generates — each
    /// binary that wants its own service identity invokes that macro itself (its output is a
    /// compile-time-bound C callback with a fixed FFI signature, so it can't be parameterised by a
    /// runtime closure or a plain `fn(Vec<OsString>)`) and passes the generated name here; this
    /// function only forwards it and translates the error type.
    pub fn start_dispatcher(
        service_key: &'static str,
        ffi_service_main: extern "system" fn(u32, *mut *mut u16),
    ) -> Result<(), IngestError> {
        service_dispatcher::start(service_key, ffi_service_main)
            .map_err(|error| to_ingest_error("service dispatcher failed", error))
    }

    /// A registered service's status handle, restricted to the one operation every service body
    /// needs when it's done: report `Stopped`. Kept deliberately narrow rather than exposing the
    /// full `windows_service` status-reporting surface, since every caller so far only ever
    /// reports `Running` (via `register_and_wait_for_stop`, once) and `Stopped` (once, at the
    /// end) — no service in this codebase reports intermediate states.
    pub struct ServiceStatusHandle(service_control_handler::ServiceStatusHandle);

    impl ServiceStatusHandle {
        pub fn report_stopped(&self) {
            let _ = self.0.set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            });
        }
    }

    /// Registers a `Stop`/`Interrogate` control handler for `service_key`, reports `Running`, and
    /// returns a receiver that fires once when SCM requests `Stop` — plus the status handle the
    /// caller uses to report `Stopped` once its own body actually finishes. This is the one piece
    /// of control-handler boilerplate every service body needs (`robocopy_ingest`'s idle loop
    /// above; notify-server's real axum-hosting body in `src/bin/notify_server.rs`), factored out
    /// so neither has to hand-roll the `mpsc` channel / event-handler closure / status-reporting
    /// dance itself.
    pub fn register_and_wait_for_stop(
        service_key: &str,
    ) -> Result<(ServiceStatusHandle, mpsc::Receiver<()>), IngestError> {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    let _ = stop_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        let status_handle =
            service_control_handler::register(service_key, event_handler).map_err(|error| {
                to_ingest_error("cannot register the service control handler", error)
            })?;

        status_handle
            .set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::STOP,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            })
            .map_err(|error| to_ingest_error("cannot report Running status", error))?;

        Ok((ServiceStatusHandle(status_handle), stop_rx))
    }
}

#[cfg(windows)]
pub use windows_impl::{
    install, install_named, register_and_wait_for_stop, run_service_dispatcher, start_dispatcher,
    uninstall, uninstall_named, ServiceStatusHandle,
};

#[cfg(not(windows))]
mod not_windows {
    use std::ffi::OsString;

    use crate::errors::IngestError;

    fn unavailable(what: &str) -> IngestError {
        IngestError::Service(format!(
            "{what} requires the Windows Service Control Manager, unavailable on this platform"
        ))
    }

    pub fn install() -> Result<(), IngestError> {
        Err(unavailable("--install-service"))
    }

    pub fn uninstall() -> Result<(), IngestError> {
        Err(unavailable("--uninstall-service"))
    }

    pub fn run_service_dispatcher() -> Result<(), IngestError> {
        Err(unavailable("service mode"))
    }

    pub fn install_named(
        _service_key: &str,
        _display_name: &str,
        _extra_launch_arguments: Vec<OsString>,
    ) -> Result<(), IngestError> {
        Err(unavailable("--install-service"))
    }

    pub fn uninstall_named(_service_key: &str) -> Result<(), IngestError> {
        Err(unavailable("--uninstall-service"))
    }

    pub fn start_dispatcher(
        _service_key: &'static str,
        _ffi_service_main: extern "system" fn(u32, *mut *mut u16),
    ) -> Result<(), IngestError> {
        Err(unavailable("service mode"))
    }

    pub struct ServiceStatusHandle;

    impl ServiceStatusHandle {
        pub fn report_stopped(&self) {}
    }

    pub fn register_and_wait_for_stop(
        _service_key: &str,
    ) -> Result<(ServiceStatusHandle, std::sync::mpsc::Receiver<()>), IngestError> {
        Err(unavailable("service mode"))
    }
}

#[cfg(not(windows))]
pub use not_windows::{
    install, install_named, register_and_wait_for_stop, run_service_dispatcher, start_dispatcher,
    uninstall, uninstall_named, ServiceStatusHandle,
};

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
