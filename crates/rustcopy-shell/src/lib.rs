//! Windows Shell drag-and-drop handler (`tidy-sniffing-river.md`): loaded on-demand by
//! `explorer.exe` when a drop-confirmation menu needs to know whether this handler wants to add
//! an item. This file is deliberately just the DLL entry points
//! (`DllGetClassObject`/`DllCanUnloadNow`/`DllRegisterServer`/`DllUnregisterServer`) plus the
//! `IClassFactory` boilerplate they need -- the actual COM object lives in [`handler`], the
//! registry writes in [`registry`], the child-process spawn in [`spawn`], same "entry point stays
//! thin" discipline as `#[tauri::command]` (F53) and `main.rs`.
//!
//! # History
//!
//! Milestone 1 shipped a stub whose `InvokeCommand` only appended a log line, to answer one
//! question empirically on the real Windows 11 dev machine before investing further: does a
//! classic `IShellExtInit`+`IContextMenu` `DragDropHandlers` registration still show up on the
//! drop confirmation menu? **Confirmed live, 10 Set 2026**: it does, including correct pidl-to-path
//! resolution for a UNC network destination. Milestone 2 replaced the log-only stub with a real,
//! direct spawn of `robocopy_ingest.exe`, gated on every dropped item being a directory -- and
//! was itself replaced the same day, live feedback from the user: a silent background copy of
//! hundreds of MB with no window, no console and no indication anything was happening at all was
//! not an acceptable trade for "zero UI thread impact". Milestone 3 (this version) hands the drop
//! off to the desktop console instead (`spawn::spawn_console_run`, `--auto-config`), which already
//! has a working progress bar, current-file indicator and Cancel button (F49) -- see `spawn.rs`'s
//! module doc for the full reasoning.
//!
//! # What is not, and cannot be, automatically tested
//! The COM plumbing below (`IClassFactory`, `DllGetClassObject`, the vtable dispatch
//! `#[implement]` generates) has no test coverage -- no COM host exists inside `cargo test`,
//! same declared limitation this project already accepts for VSS (F30) and `--install-service`
//! (F37). What *is* unit-tested lives in `handler`'s pure helpers (`all_are_directories`),
//! `log.rs`, `spawn.rs`'s pure path computations, and `registry.rs`'s pure string-formatting
//! functions.

mod handler;
mod log;
mod registry;
mod spawn;

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{implement, Interface, Result, BOOL, GUID, HRESULT};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, E_NOINTERFACE, HINSTANCE, MAX_PATH, S_FALSE, S_OK,
};
use windows::Win32::System::Com::IClassFactory;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

use handler::{RustCopyHandler, CLSID_RUSTCOPY_HANDLER, DLL_MODULE_HANDLE};

/// Outstanding COM object instances + active `LockServer` locks combined -- `DllCanUnloadNow`
/// only needs to know "is it safe to unload", not which of the two kept it alive. Every
/// `RustCopyHandler` increments this on construction and decrements on `Drop`; `LockServer`
/// increments/decrements it directly. Mirrors the reference-counting discipline COM requires of
/// every in-proc server, not something specific to this project.
static OUTSTANDING: AtomicU32 = AtomicU32::new(0);

struct InstanceGuard;
impl InstanceGuard {
    fn new() -> Self {
        OUTSTANDING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        OUTSTANDING.fetch_sub(1, Ordering::SeqCst);
    }
}

#[implement(IClassFactory)]
struct ClassFactory {
    _guard: InstanceGuard,
}

impl ClassFactory {
    fn new() -> Self {
        Self {
            _guard: InstanceGuard::new(),
        }
    }
}

impl windows::Win32::System::Com::IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        outer: windows::core::Ref<'_, windows::core::IUnknown>,
        riid: *const GUID,
        object: *mut *mut c_void,
    ) -> Result<()> {
        if outer.is_some() {
            // Aggregation is not supported -- no caller in this project's own tree needs it, and
            // every other COM server this codebase could plausibly ship (none exist yet) has no
            // precedent for it either.
            return Err(windows::core::Error::from(
                windows::Win32::Foundation::CLASS_E_NOAGGREGATION,
            ));
        }
        let _guard = InstanceGuard::new();
        let unknown: windows::core::IUnknown = RustCopyHandler::new().into();
        unsafe { unknown.query(riid, object) }.ok()
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            OUTSTANDING.fetch_add(1, Ordering::SeqCst);
        } else {
            OUTSTANDING.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

/// Captures the DLL's own module handle -- the one piece of state a Shell extension DLL cannot
/// get any other way (`std::env::current_exe()` would return `explorer.exe`'s path from in
/// here, not this DLL's; see `handler::DLL_MODULE_HANDLE`'s doc comment for why that matters).
#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllMain(module: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        DLL_MODULE_HANDLE.store(module.0 as isize, Ordering::SeqCst);
    }
    BOOL(1)
}

#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    let requested = unsafe { &*rclsid };
    if *requested != CLSID_RUSTCOPY_HANDLER {
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    let factory: IClassFactory = ClassFactory::new().into();
    match unsafe { factory.query(riid, ppv) }.ok() {
        Ok(()) => S_OK,
        Err(_) => E_NOINTERFACE,
    }
}

#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OUTSTANDING.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

/// Resolves the DLL's own on-disk path via `GetModuleFileNameW` on the handle `DllMain` captured
/// -- never `std::env::current_exe()`, which from inside a DLL loaded into `explorer.exe` would
/// resolve to Explorer's own path, not this DLL's. Directly analogous to the anti-PATH-hijack
/// reasoning behind `runner::cli_beside` in `rustcopy-core`, applied one level earlier (finding
/// *this* module, before `cli_beside` can find `robocopy_ingest.exe` beside it in Milestone 2).
pub(crate) fn own_dll_path() -> Option<String> {
    let handle = HINSTANCE(DLL_MODULE_HANDLE.load(Ordering::SeqCst) as *mut c_void);
    let mut buffer = [0u16; MAX_PATH as usize];
    let len = unsafe { GetModuleFileNameW(Some(handle.into()), &mut buffer) };
    if len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..len as usize]))
}

#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllRegisterServer() -> HRESULT {
    let Some(dll_path) = own_dll_path() else {
        return windows::Win32::Foundation::E_UNEXPECTED;
    };
    match registry::register(&dll_path) {
        Ok(()) => S_OK,
        Err(error) => error.code(),
    }
}

#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllUnregisterServer() -> HRESULT {
    match registry::unregister() {
        Ok(()) => S_OK,
        Err(error) => error.code(),
    }
}
