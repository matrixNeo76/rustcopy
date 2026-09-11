//! Milestone 3 (`tidy-sniffing-river.md`): the real drag-and-drop handler. `QueryContextMenu`
//! only offers "Copia con RustCopy" when every dropped item is a folder (see the plan's Design
//! section for why -- a loose file would need copying synchronously inside `explorer.exe`'s own
//! process to avoid a second, different code path, which is exactly the blocking risk this
//! feature exists to avoid). `InvokeCommand` hands every dropped folder off to the desktop
//! console (`spawn::spawn_console_run`) for visible progress, fire-and-forget, and returns
//! immediately -- see `spawn.rs`'s module doc for why this replaced Milestone 2's direct,
//! silent `robocopy_ingest.exe` spawn.

use std::path::PathBuf;
use std::sync::atomic::AtomicIsize;
use std::sync::Mutex;

use windows::core::{implement, Result, GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    DragQueryFileW, IContextMenu, IContextMenu_Impl, IShellExtInit, IShellExtInit_Impl,
    SHGetPathFromIDListW, CMINVOKECOMMANDINFO, HDROP,
};
use windows::Win32::UI::WindowsAndMessaging::{InsertMenuW, MF_BYPOSITION, MF_STRING};

use crate::{log, own_dll_path, spawn};

const MENU_TEXT: &str = "Copia con RustCopy\0";

/// Reads every path out of an `IDataObject`'s `CF_HDROP` data, the format Explorer always
/// populates a drag source with (file/folder paths dropped together). Returns an empty `Vec` on
/// any failure along the way -- an empty list makes `QueryContextMenu`'s "every item is a
/// folder" check trivially false (an empty selection is not "all folders"), which is the correct,
/// safe default: never show the menu item when the source list could not be read at all.
fn read_dropped_paths(data_object: &IDataObject) -> Vec<PathBuf> {
    let format = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };

    let Ok(mut medium) = (unsafe { data_object.GetData(&format) }) else {
        return Vec::new();
    };

    let hglobal = unsafe { medium.u.hGlobal };
    let hdrop = HDROP(hglobal.0);

    let count = unsafe { DragQueryFileW(hdrop, 0xFFFF_FFFF, None) };
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        let mut buffer = [0u16; MAX_PATH as usize];
        let len = unsafe { DragQueryFileW(hdrop, index, Some(&mut buffer)) };
        if len > 0 {
            paths.push(PathBuf::from(String::from_utf16_lossy(
                &buffer[..len as usize],
            )));
        }
    }

    unsafe {
        windows::Win32::System::Ole::ReleaseStgMedium(&mut medium);
    }
    paths
}

/// Pure classification: the menu item only ever appears when every dropped path is a real,
/// existing directory. Split out from `QueryContextMenu` so it is testable without a real
/// `IDataObject` -- see the module doc in `lib.rs` for why the COM plumbing itself cannot be.
pub fn all_are_directories(paths: &[PathBuf]) -> bool {
    !paths.is_empty() && paths.iter().all(|p| p.is_dir())
}

#[implement(IShellExtInit, IContextMenu)]
pub struct RustCopyHandler {
    dest_folder: Mutex<Option<PathBuf>>,
    dropped_paths: Mutex<Vec<PathBuf>>,
}

impl RustCopyHandler {
    pub fn new() -> Self {
        Self {
            dest_folder: Mutex::new(None),
            dropped_paths: Mutex::new(Vec::new()),
        }
    }
}

impl Default for RustCopyHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl IShellExtInit_Impl for RustCopyHandler_Impl {
    fn Initialize(
        &self,
        pidlfolder: *const ITEMIDLIST,
        pdtobj: windows::core::Ref<'_, IDataObject>,
        _hkeyprogid: windows::Win32::System::Registry::HKEY,
    ) -> Result<()> {
        let mut path_buf = [0u16; MAX_PATH as usize];
        let dest = unsafe {
            if SHGetPathFromIDListW(pidlfolder, &mut path_buf).as_bool() {
                let len = path_buf.iter().position(|&c| c == 0).unwrap_or(0);
                Some(PathBuf::from(String::from_utf16_lossy(&path_buf[..len])))
            } else {
                None
            }
        };
        *self.dest_folder.lock().expect("lock poisoned") = dest.clone();

        let dropped = pdtobj.as_ref().map(read_dropped_paths).unwrap_or_default();
        let _ = log::append_line(
            &log::log_path(),
            &format!(
                "Initialize: dest = {dest:?}, dropped = {dropped:?}, all_dirs = {}",
                all_are_directories(&dropped)
            ),
        );
        *self.dropped_paths.lock().expect("lock poisoned") = dropped;

        Ok(())
    }
}

impl IContextMenu_Impl for RustCopyHandler_Impl {
    /// See the module doc: the item is added only when every dropped path is a directory.
    /// Return-value convention same as the Milestone 1 stub -- the low 16 bits of a success
    /// `HRESULT` must carry the count of command identifiers used, `0` when nothing was added.
    fn QueryContextMenu(
        &self,
        hmenu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        indexmenu: u32,
        idcmdfirst: u32,
        _idcmdlast: u32,
        _uflags: u32,
    ) -> HRESULT {
        let dropped = self.dropped_paths.lock().expect("lock poisoned");
        if !all_are_directories(&dropped) {
            return HRESULT(0);
        }
        drop(dropped);

        let text: Vec<u16> = MENU_TEXT.encode_utf16().collect();
        let inserted = unsafe {
            InsertMenuW(
                hmenu,
                indexmenu,
                MF_BYPOSITION | MF_STRING,
                idcmdfirst as usize,
                PCWSTR(text.as_ptr()),
            )
        };
        if let Err(error) = inserted {
            return error.code();
        }
        HRESULT(1)
    }

    /// Spawns one `robocopy_ingest.exe` per dropped folder, fire-and-forget, then returns.
    /// Never blocks on the children finishing -- a shell extension that waited here would hold
    /// `explorer.exe`'s UI thread hostage for as long as the copy takes, the exact failure mode
    /// this whole design avoids.
    fn InvokeCommand(&self, pici: *const CMINVOKECOMMANDINFO) -> Result<()> {
        let _ = unsafe { (*pici).lpVerb };

        let dest_folder = self.dest_folder.lock().expect("lock poisoned").clone();
        let dropped = self.dropped_paths.lock().expect("lock poisoned").clone();

        let Some(dest_folder) = dest_folder else {
            let _ = log::append_line(&log::log_path(), "InvokeCommand: no dest_folder, aborting");
            return Ok(());
        };
        if !all_are_directories(&dropped) {
            // Defensive re-check: QueryContextMenu already gates this, but InvokeCommand must
            // never trust that its own earlier decision still holds without re-verifying, the
            // same discipline `job_editor::apply_draft` (rustcopy-core) applies to its own
            // client-side-then-server-side checks.
            let _ = log::append_line(
                &log::log_path(),
                "InvokeCommand: dropped paths are no longer all directories, aborting",
            );
            return Ok(());
        }

        let Some(dll_path) = own_dll_path() else {
            let _ = log::append_line(
                &log::log_path(),
                "InvokeCommand: could not resolve own DLL path",
            );
            return Ok(());
        };
        let gui = match robocopy_ingest::runner::gui_beside(&PathBuf::from(&dll_path)) {
            Ok(gui) => gui,
            Err(error) => {
                let _ = log::append_line(
                    &log::log_path(),
                    &format!("InvokeCommand: gui_beside failed: {error}"),
                );
                return Ok(());
            }
        };

        let items: Vec<(PathBuf, PathBuf)> = dropped
            .iter()
            .filter_map(|item| {
                spawn::per_item_destination(&dest_folder, item).map(|dest| (item.clone(), dest))
            })
            .collect();
        if items.is_empty() {
            let _ = log::append_line(
                &log::log_path(),
                "InvokeCommand: no item had a usable name, nothing to hand off",
            );
            return Ok(());
        }

        match spawn::spawn_console_run(&gui, &items) {
            Ok(_child) => {
                let _ = log::append_line(
                    &log::log_path(),
                    &format!(
                        "InvokeCommand: handed {} item(s) to the console: {items:?}",
                        items.len()
                    ),
                );
            }
            Err(error) => {
                let _ = log::append_line(
                    &log::log_path(),
                    &format!("InvokeCommand: failed to hand off to the console: {error}"),
                );
            }
        }

        Ok(())
    }

    fn GetCommandString(
        &self,
        _idcmd: usize,
        _uflags: u32,
        _preserved: *const u32,
        _pszname: windows::core::PSTR,
        _cchmax: u32,
    ) -> Result<()> {
        Ok(())
    }
}

/// The stable identity of this handler, registered under both `CLSID\{...}\InprocServer32` and
/// the two `shellex\DragDropHandlers` keys. Generated once (`[guid]::NewGuid()`), never to be
/// regenerated -- changing it would orphan every existing installation's registry entries, same
/// reasoning as `crypto::CREDENTIAL_SERVICE`'s stability note in `CLAUDE.md`.
pub const CLSID_RUSTCOPY_HANDLER: GUID = GUID::from_u128(0x59139f3e_0f3d_443e_bfc6_a5ce7cb466fc);

/// The DLL's own module handle, captured once at `DLL_PROCESS_ATTACH` (see `lib.rs::DllMain`).
pub static DLL_MODULE_HANDLE: AtomicIsize = AtomicIsize::new(0);
