//! Pausing a copy (CATALOGO_COMPORTAMENTI_GUI.md L35): the run's whole process tree -- the command line and
//! the robocopy it started -- has every thread suspended, and later resumed. Nothing in the copy knows it
//! was paused; nothing is killed, so nothing is lost.
//!
//! The honest limit: a paused robocopy keeps its network connections open and silent, and a share (SMB)
//! may drop a connection that stays idle too long. The console therefore resumes a pause by itself after
//! [`PAUSE_LIMIT`] and says so; if a connection was lost anyway, robocopy's own retries deal with it like
//! any interruption.

use std::collections::HashSet;
use std::time::Duration;

/// How long a pause may last before the console resumes the copy on its own.
pub const PAUSE_LIMIT: Duration = Duration::from_secs(10 * 60);

/// `root` and every process descended from it, given every `(pid, parent pid)` pair of the machine.
/// Pure, so the tree walk is tested without touching a process.
pub fn descendants(root: u32, processes: &[(u32, u32)]) -> Vec<u32> {
    let mut found: Vec<u32> = vec![root];
    let mut seen: HashSet<u32> = HashSet::from([root]);
    let mut next = 0;
    while next < found.len() {
        let parent = found[next];
        next += 1;
        for &(pid, ppid) in processes {
            // A process whose id equals its parent's (the idle process) must not loop, and an id
            // seen once is never taken twice.
            if ppid == parent && pid != parent && seen.insert(pid) {
                found.push(pid);
            }
        }
    }
    found
}

#[cfg(windows)]
mod win {
    use std::collections::HashSet;
    use std::mem::size_of;

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, Thread32First, Thread32Next,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::Threading::{
        OpenThread, ResumeThread, SuspendThread, THREAD_SUSPEND_RESUME,
    };

    fn processes() -> Vec<(u32, u32)> {
        let mut found = Vec::new();
        // SAFETY: a snapshot handle is created, walked with a correctly sized, zeroed entry whose
        // `dwSize` is set as the API requires, and closed before returning.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return found;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                found.push((entry.th32ProcessID, entry.th32ParentProcessID));
                more = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        found
    }

    fn threads_of(pids: &HashSet<u32>) -> Vec<u32> {
        let mut found = Vec::new();
        // SAFETY: same pattern as `processes`, over the thread snapshot.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return found;
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            let mut more = Thread32First(snapshot, &mut entry) != 0;
            while more {
                if pids.contains(&entry.th32OwnerProcessID) {
                    found.push(entry.th32ThreadID);
                }
                more = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        found
    }

    /// Suspends or resumes every thread of `root` and its descendants. Suspending repeats until a pass
    /// finds no thread it has not already suspended (a running thread may start another meanwhile).
    /// Returns how many threads were touched.
    pub fn set_suspended(root: u32, suspend: bool) -> usize {
        let mut touched: HashSet<u32> = HashSet::new();
        let passes = if suspend { 6 } else { 1 };
        for _ in 0..passes {
            let tree: HashSet<u32> = super::descendants(root, &processes()).into_iter().collect();
            let fresh: Vec<u32> = threads_of(&tree)
                .into_iter()
                .filter(|tid| !touched.contains(tid))
                .collect();
            if fresh.is_empty() {
                break;
            }
            for tid in fresh {
                // SAFETY: the handle is opened for suspend/resume only, used once and closed.
                unsafe {
                    let handle = OpenThread(THREAD_SUSPEND_RESUME, 0, tid);
                    if !handle.is_null() {
                        if suspend {
                            SuspendThread(handle);
                        } else {
                            ResumeThread(handle);
                        }
                        CloseHandle(handle);
                        touched.insert(tid);
                    }
                }
            }
        }
        touched.len()
    }
}

/// Suspends (`true`) or resumes (`false`) the process tree rooted at `root`. Returns how many threads
/// were touched; `0` means nothing could be reached (the process is gone, or access was refused).
#[cfg(windows)]
pub fn set_suspended(root: u32, suspend: bool) -> usize {
    win::set_suspended(root, suspend)
}

#[cfg(not(windows))]
pub fn set_suspended(_root: u32, _suspend: bool) -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_includes_children_and_grandchildren_but_not_siblings() {
        // 1 -> 2 -> 4; 1 -> 3; 9 is unrelated.
        let table = [(2, 1), (3, 1), (4, 2), (9, 8), (1, 0)];
        let mut tree = descendants(1, &table);
        tree.sort_unstable();
        assert_eq!(tree, vec![1, 2, 3, 4]);
        assert_eq!(descendants(2, &table).len(), 2, "2 and 4");
    }

    #[test]
    fn a_root_with_no_children_is_just_itself_and_a_cycle_cannot_loop() {
        assert_eq!(descendants(7, &[]), vec![7]);
        // A malformed table where a process is its own parent, and two processes parent each other.
        assert_eq!(descendants(1, &[(1, 1)]), vec![1]);
        let mut loop_tree = descendants(1, &[(2, 1), (1, 2)]);
        loop_tree.sort_unstable();
        assert_eq!(loop_tree, vec![1, 2]);
    }

    #[cfg(windows)]
    #[test]
    fn a_real_child_can_be_suspended_and_resumed() {
        // A short-lived real process: `ping` to localhost for a few seconds.
        let mut child = std::process::Command::new("ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("ping starts");
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            set_suspended(child.id(), true) > 0,
            "threads were suspended"
        );
        assert!(set_suspended(child.id(), false) > 0, "threads were resumed");
        let _ = child.kill();
        let _ = child.wait();
    }
}
