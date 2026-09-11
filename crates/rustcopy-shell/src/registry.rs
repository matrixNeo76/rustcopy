//! `DllRegisterServer`/`DllUnregisterServer`'s registry writes, split out of `lib.rs` so the DLL
//! entry points themselves stay thin dispatchers -- same "thin wrapper, real logic elsewhere"
//! discipline this project already applies to `#[tauri::command]` (F53) and the CLI's own
//! `main.rs`.
//!
//! Symmetric by construction: [`register`] and [`unregister`] touch exactly the same three key
//! paths, nothing more -- `unregister` never needs `uninsdeletekey`-style external bookkeeping
//! (Inno Setup's `Flags: regserver` calls these two functions directly at install/uninstall time)
//! because the DLL that wrote a key is also the one responsible for removing it.

use windows::core::{Result, GUID, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegDeleteTreeW, RegSetValueExW, HKEY,
    HKEY_LOCAL_MACHINE, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use crate::handler::CLSID_RUSTCOPY_HANDLER;

const HANDLER_NAME: &str = "RustCopy";

/// Windows' own CLSID subkey convention requires braces (`{XXXXXXXX-...}`) -- `GUID`'s `Debug`
/// impl (verified by reading `windows-core`'s `guid.rs`) does not include them, only the hyphenated
/// hex body. Without this, a subkey written under `CLSID\<no-braces>\InprocServer32` is never
/// found by `CoCreateInstance`'s real lookup at `CLSID\{...}\InprocServer32` -- found live, 10 Set
/// 2026: the stub's `DragDropHandlers` keys existed but `InprocServer32` was silently unreachable.
fn guid_to_registry_string(guid: &GUID) -> String {
    format!("{{{guid:?}}}").to_uppercase()
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Writes a single default (unnamed) `REG_SZ` value under `HKEY_LOCAL_MACHINE\<subkey>`.
/// `value` is what the caller wants read back at `<subkey>\(Default)` -- for
/// `InprocServer32` that is the DLL's own path, for a `DragDropHandlers\RustCopy` key it is the
/// handler's CLSID string.
fn write_default_value(subkey: &str, value: &str) -> Result<()> {
    let subkey_wide = to_wide_null(subkey);
    let mut hkey = HKEY::default();
    unsafe {
        let status = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_wide.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if status != ERROR_SUCCESS {
            return Err(status.into());
        }

        let value_wide = to_wide_null(value);
        let value_bytes: &[u8] = std::slice::from_raw_parts(
            value_wide.as_ptr() as *const u8,
            value_wide.len() * std::mem::size_of::<u16>(),
        );
        let status = RegSetValueExW(hkey, PCWSTR::null(), None, REG_SZ, Some(value_bytes));
        let _ = RegCloseKey(hkey);
        if status != ERROR_SUCCESS {
            return Err(status.into());
        }
    }
    Ok(())
}

/// `HKLM\Software\Classes\CLSID\{...}\InprocServer32`'s `ThreadingModel` named value --
/// `Apartment`, the correct choice for a Shell extension (Explorer hosts extensions on its own
/// STA thread; a free-threaded object would need its own, unnecessary, synchronization).
fn write_threading_model(clsid_key: &str) -> Result<()> {
    let subkey = format!("{clsid_key}\\InprocServer32");
    let subkey_wide = to_wide_null(&subkey);
    let mut hkey = HKEY::default();
    unsafe {
        let status = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_wide.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if status != ERROR_SUCCESS {
            return Err(status.into());
        }
        let name_wide = to_wide_null("ThreadingModel");
        let value_wide = to_wide_null("Apartment");
        let value_bytes: &[u8] = std::slice::from_raw_parts(
            value_wide.as_ptr() as *const u8,
            value_wide.len() * std::mem::size_of::<u16>(),
        );
        let status = RegSetValueExW(
            hkey,
            PCWSTR(name_wide.as_ptr()),
            None,
            REG_SZ,
            Some(value_bytes),
        );
        let _ = RegCloseKey(hkey);
        if status != ERROR_SUCCESS {
            return Err(status.into());
        }
    }
    Ok(())
}

/// Writes the three keys this handler needs: the CLSID's own `InprocServer32` (pointing at
/// `dll_path`, the DLL's own real on-disk location -- resolved by the caller via
/// `GetModuleFileNameW` on the DLL's own module handle, never assumed), and the two
/// `shellex\DragDropHandlers\RustCopy` entries (`Directory` and `Drive`) that make Explorer's
/// drop-confirmation menu actually call this CLSID.
pub fn register(dll_path: &str) -> Result<()> {
    let clsid_string = guid_to_registry_string(&CLSID_RUSTCOPY_HANDLER);
    let clsid_key = format!("Software\\Classes\\CLSID\\{clsid_string}");

    write_default_value(&format!("{clsid_key}\\InprocServer32"), dll_path)?;
    write_threading_model(&clsid_key)?;

    for container in ["Directory", "Drive"] {
        let handler_key =
            format!("Software\\Classes\\{container}\\shellex\\DragDropHandlers\\{HANDLER_NAME}");
        write_default_value(&handler_key, &clsid_string)?;
    }

    Ok(())
}

/// Removes exactly what [`register`] wrote, nothing broader -- `RegDeleteTreeW` on the CLSID key
/// (it owns the whole `{...}` subtree, including `InprocServer32`), `RegDeleteKeyW` on each
/// `DragDropHandlers\RustCopy` leaf (never the shared `DragDropHandlers` key itself, which other
/// software may also register under). Tolerates `ERROR_FILE_NOT_FOUND` on any of the three --
/// found live, 10 Set 2026: a partial/manual registration (missing `InprocServer32`, the exact
/// bug `guid_to_registry_string` had) left only the two `DragDropHandlers` keys behind, and the
/// original strict version would have aborted on the missing CLSID key before ever reaching
/// them, leaving a real registry mess `regsvr32 /u` could not actually clean up.
pub fn unregister() -> Result<()> {
    let clsid_string = guid_to_registry_string(&CLSID_RUSTCOPY_HANDLER);
    let clsid_key = to_wide_null(&format!("Software\\Classes\\CLSID\\{clsid_string}"));
    unsafe {
        let status = RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(clsid_key.as_ptr()));
        if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
            return Err(status.into());
        }
    }

    for container in ["Directory", "Drive"] {
        let handler_key = to_wide_null(&format!(
            "Software\\Classes\\{container}\\shellex\\DragDropHandlers\\{HANDLER_NAME}"
        ));
        unsafe {
            let status = RegDeleteKeyW(HKEY_LOCAL_MACHINE, PCWSTR(handler_key.as_ptr()));
            if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
                return Err(status.into());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clsid_formats_as_a_registry_style_braced_uppercase_guid() {
        let s = guid_to_registry_string(&CLSID_RUSTCOPY_HANDLER);
        assert!(s.starts_with('{'));
        assert!(s.ends_with('}'));
        assert_eq!(s, s.to_uppercase(), "registry convention is uppercase");
    }

    #[test]
    fn to_wide_null_always_ends_in_a_zero_code_unit() {
        let wide = to_wide_null("abc");
        assert_eq!(wide.last(), Some(&0));
        assert_eq!(wide.len(), 4);
    }
}
