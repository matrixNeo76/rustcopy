//! One window per user session (CATALOGO_COMPORTAMENTI_GUI.md E12, RF-Y11).
//!
//! TeraCopy hands a second launch to the window already open; so must this console, because a drop
//! from Explorer starts a new process (`rustcopy-gui.exe --auto-config <toml>`) and a second window
//! on the same copy would be confusing. A named mutex decides who is first; a named pipe carries the
//! second launch's request to it.
//!
//! The pipe accepts exactly two requests, `activate` and `auto-config <file>`, and the file must be
//! one of the throw-away configurations the Explorer extension itself writes
//! (`runner::cancel_file_dir()/shell-drop-*.toml`). Without that restriction any local process able
//! to write to the pipe could make an open console start any configuration, mirror ones included.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// What a second launch asks the first window to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    /// Just bring the window forward.
    Activate,
    /// Start the Explorer extension's configuration.
    OpenConfig(PathBuf),
}

/// Builds the message for this launch's command line (`args` without the program name).
pub fn encode(args: &[String]) -> String {
    match auto_config_arg(args) {
        Some(path) => format!("auto-config\t{path}"),
        None => "activate".to_string(),
    }
}

/// The value after `--auto-config`, if present.
pub fn auto_config_arg(args: &[String]) -> Option<String> {
    let at = args.iter().position(|arg| arg == "--auto-config")?;
    args.get(at + 1).cloned()
}

/// Parses a message received on the pipe. Anything unknown, or a file that is not an Explorer drop
/// configuration, is refused (`None`).
pub fn parse(message: &str) -> Option<Request> {
    let message = message.trim();
    if message == "activate" {
        return Some(Request::Activate);
    }
    let path = message.strip_prefix("auto-config\t")?;
    let path = PathBuf::from(path);
    is_shell_drop_config(&path, &robocopy_ingest::runner::cancel_file_dir())
        .then_some(Request::OpenConfig(path))
}

/// `true` when `path` is `shell-drop-*.toml` directly inside `dir`, with no `..` tricks.
pub fn is_shell_drop_config(path: &Path, dir: &Path) -> bool {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
    else {
        return false;
    };
    let lower = name.to_lowercase();
    if !(lower.starts_with("shell-drop-") && lower.ends_with(".toml")) {
        return false;
    }
    // The temp directory may be spelled in its 8.3 short form by one process and in its long form by
    // another; resolving both avoids refusing the Explorer extension's own file.
    let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x.to_string_lossy().eq_ignore_ascii_case(&y.to_string_lossy()),
        _ => a
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['\\', '/'])),
    };
    same(parent, dir)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::OsStr;
    use std::fs::File;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, HANDLE,
        INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_INBOUND;
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::CreateMutexW;

    /// Keeps the mutex alive for the life of the process. Dropping it lets a new first window start.
    pub struct Instance {
        mutex: HANDLE,
    }

    impl Drop for Instance {
        fn drop(&mut self) {
            // SAFETY: `mutex` was returned by `CreateMutexW` and is closed exactly once, here.
            unsafe { CloseHandle(self.mutex) };
        }
    }

    pub enum Role {
        Primary(Instance),
        Secondary,
    }

    fn wide(text: &str) -> Vec<u16> {
        OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn pipe_name(tag: &str) -> String {
        let user = std::env::var("USERNAME").unwrap_or_default();
        format!(r"\\.\pipe\{tag}-{user}")
    }

    /// Becomes the first window, or reports that one already exists.
    pub fn acquire(tag: &str) -> Role {
        let name = wide(&format!("Local\\{tag}"));
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if mutex.is_null() {
            // Cannot tell: behave as the first window rather than refuse to start at all.
            return Role::Primary(Instance {
                mutex: std::ptr::null_mut(),
            });
        }
        // SAFETY: reading the calling thread's last error immediately after the call.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: closing the handle just obtained.
            unsafe { CloseHandle(mutex) };
            Role::Secondary
        } else {
            Role::Primary(Instance { mutex })
        }
    }

    /// Sends `message` to the first window, retrying while it is still starting up.
    pub fn send(tag: &str, message: &str) -> Result<(), String> {
        use std::io::Write;
        let path = pipe_name(tag);
        let mut last = String::new();
        for _ in 0..40 {
            match std::fs::OpenOptions::new().write(true).open(&path) {
                Ok(mut pipe) => {
                    return pipe
                        .write_all(message.as_bytes())
                        .map_err(|e| e.to_string())
                }
                Err(error) => last = error.to_string(),
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(format!("la finestra già aperta non risponde: {last}"))
    }

    /// Serves the pipe on a background thread; `on_message` runs on that thread for every request.
    pub fn listen(tag: &str, on_message: impl Fn(String) + Send + 'static) {
        let name = wide(&pipe_name(tag));
        std::thread::spawn(move || loop {
            // SAFETY: `name` is a NUL-terminated UTF-16 string; a null security descriptor gives the
            // default ACL (creator and administrators write, others read only).
            let handle = unsafe {
                CreateNamedPipeW(
                    name.as_ptr(),
                    PIPE_ACCESS_INBOUND,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_UNLIMITED_INSTANCES,
                    4096,
                    4096,
                    0,
                    std::ptr::null(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
            // SAFETY: blocking connect on the pipe just created; `ERROR_PIPE_CONNECTED` means a client
            // connected between the create and the connect, which is also success.
            let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) } != 0
                || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            if !connected {
                // SAFETY: closing the handle just obtained.
                unsafe { CloseHandle(handle) };
                continue;
            }
            // SAFETY: `handle` is a valid pipe handle this thread owns; `File` takes ownership and closes it.
            let mut pipe = unsafe { File::from_raw_handle(handle as _) };
            let mut text = String::new();
            // 8 KiB is far more than a path; a longer message is truncated and then fails to parse.
            let _ = (&mut pipe).take(8192).read_to_string(&mut text);
            drop(pipe);
            if !text.is_empty() {
                on_message(text);
            }
        });
    }
}

#[cfg(windows)]
pub use imp::{acquire, listen, send, Role};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launch_without_arguments_only_activates() {
        assert_eq!(parse(&encode(&[])), Some(Request::Activate));
    }

    #[test]
    fn the_auto_config_value_is_found_after_its_flag() {
        let args = vec![
            "--x".to_string(),
            "--auto-config".to_string(),
            r"C:\t\shell-drop-1.toml".to_string(),
        ];
        assert_eq!(
            auto_config_arg(&args).as_deref(),
            Some(r"C:\t\shell-drop-1.toml")
        );
        assert_eq!(auto_config_arg(&["--auto-config".to_string()]), None);
    }

    #[test]
    fn only_explorer_drop_configurations_are_accepted() {
        let dir = Path::new(r"C:\Users\x\AppData\Local\Temp\rustcopy");
        assert!(is_shell_drop_config(
            &dir.join("shell-drop-20261008-1.toml"),
            dir
        ));
        assert!(
            is_shell_drop_config(&dir.join("SHELL-DROP-1.TOML"), dir),
            "case-insensitive on Windows"
        );
        assert!(
            !is_shell_drop_config(&dir.join("backup.toml"), dir),
            "any other configuration is refused"
        );
        assert!(
            !is_shell_drop_config(Path::new(r"C:\evil\shell-drop-1.toml"), dir),
            "wrong directory"
        );
        assert!(
            !is_shell_drop_config(&dir.join(r"..\evil\shell-drop-1.toml"), dir),
            "no parent-directory tricks"
        );
        assert!(!is_shell_drop_config(&dir.join("shell-drop-1.exe"), dir));
    }

    #[test]
    fn unknown_messages_are_refused() {
        assert_eq!(parse("format c:"), None);
        assert_eq!(parse("auto-config\t"), None);
        assert_eq!(parse(""), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_second_acquire_is_secondary_and_its_message_reaches_the_first() {
        use std::sync::mpsc;
        let tag = format!("rustcopy-ui-test-{}", std::process::id());
        let first = acquire(&tag);
        assert!(
            matches!(first, Role::Primary(_)),
            "the first caller owns the window"
        );
        let (tx, rx) = mpsc::channel();
        listen(&tag, move |message| {
            let _ = tx.send(message);
        });
        assert!(
            matches!(acquire(&tag), Role::Secondary),
            "the second caller must hand over"
        );
        send(&tag, "activate").expect("delivery");
        let received = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the first window got the message");
        assert_eq!(received, "activate");
    }
}
