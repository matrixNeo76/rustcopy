//! Keeps the computer from going to sleep while a copy started by this window is running
//! (CATALOGO_COMPORTAMENTI_GUI.md L42). A long copy must not stop because a laptop decided nobody
//! was at the keyboard.
//!
//! What it does **not** do, on purpose: it only covers copies this window started (a scheduled
//! run belongs to Task Scheduler, which has its own wake settings), it does not keep the *screen*
//! on (the monitor may turn off, the system stays up), and it holds nothing once the window is
//! gone: Windows releases the request when the thread that made it ends.
//!
//! The request is per thread, so it is made and withdrawn from the one UI thread. [`KeepAwake`] only
//! decides *when* to ask, so that decision is a unit test; the Windows call is one small function.

use std::cell::Cell;

/// Tracks whether the request is currently held and asks the system only when that changes.
#[derive(Default)]
pub struct KeepAwake {
    held: Cell<bool>,
}

impl KeepAwake {
    /// Makes the request match `wanted`, calling `apply` only on a change. Returns whether it did.
    pub fn update(&self, wanted: bool, apply: impl FnOnce(bool)) -> bool {
        if self.held.get() == wanted {
            return false;
        }
        self.held.set(wanted);
        apply(wanted);
        true
    }

    #[cfg(test)]
    pub fn is_held(&self) -> bool {
        self.held.get()
    }
}

/// Asks Windows to keep the system awake (`true`) or withdraws the request (`false`).
#[cfg(windows)]
pub fn set_system_awake(awake: bool) {
    use windows_sys::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    let flags = if awake {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    // SAFETY: a plain call with a flags value; it has no pointers and no preconditions.
    unsafe {
        SetThreadExecutionState(flags);
    }
}

#[cfg(not(windows))]
pub fn set_system_awake(_awake: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_asks_only_when_the_wish_changes() {
        let awake = KeepAwake::default();
        let mut calls: Vec<bool> = Vec::new();
        assert!(
            !awake.update(false, |w| calls.push(w)),
            "nothing to withdraw yet"
        );
        assert!(awake.update(true, |w| calls.push(w)));
        assert!(
            !awake.update(true, |w| calls.push(w)),
            "already held: no second request"
        );
        assert!(awake.is_held());
        assert!(awake.update(false, |w| calls.push(w)));
        assert!(!awake.update(false, |w| calls.push(w)));
        assert!(!awake.is_held());
        assert_eq!(calls, vec![true, false]);
    }
}
