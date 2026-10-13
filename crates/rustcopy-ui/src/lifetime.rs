//! When the console's process ends (14 Ott 2026).
//!
//! It used to end only through the tray icon's "Esci": closing the window hid it and the process stayed, so
//! an idle console lingered for days in the notification area and held `rustcopy-gui.exe` open, which is what
//! blocked an upgrade. The rule now is the one a person expects:
//!
//! * closing the window **while nothing runs** closes the program;
//! * closing it **while a copy runs** (or while "Esegui tutte" has tasks left) only hides it: the copy
//!   goes on and the icon stays, because stopping it behind a click on "x" would lose the work;
//! * a window that was hidden that way and has nothing left to do closes the program by itself a few
//!   seconds after the copy ended, so a finished copy does not leave an idle process behind either.
//!
//! The decisions are here, pure and tested; `main.rs` only asks them.

use std::time::Duration;

/// How long a hidden, idle console waits before it ends by itself: long enough for the end-of-copy
/// notification to be handed to Windows, short enough not to be a lingering process.
pub const HIDDEN_IDLE_GRACE: Duration = Duration::from_secs(10);

/// Whether the console is doing something that closing the window must not end.
pub fn is_busy(copy_running: bool, tasks_waiting: bool) -> bool {
    copy_running || tasks_waiting
}

/// What closing the window does: `true` ends the program, `false` only hides the window.
pub fn quit_on_close(busy: bool) -> bool {
    !busy
}

/// Whether a console that is **not** showing its window should end now: nothing is running or waiting and
/// it has been that way for the grace period. `hidden_for` is how long it has been hidden and idle (`None`
/// when it has not been, or is not hidden).
pub fn quit_when_hidden_idle(
    window_visible: bool,
    busy: bool,
    hidden_for: Option<Duration>,
) -> bool {
    !window_visible && !busy && hidden_for.is_some_and(|waited| waited >= HIDDEN_IDLE_GRACE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_or_waiting_tasks_make_it_busy() {
        assert!(!is_busy(false, false));
        assert!(is_busy(true, false));
        assert!(
            is_busy(false, true),
            "between two tasks of Esegui tutte it is still busy"
        );
        assert!(is_busy(true, true));
    }

    #[test]
    fn closing_the_window_ends_the_program_only_when_nothing_runs() {
        assert!(quit_on_close(false));
        assert!(
            !quit_on_close(true),
            "a copy goes on: the window only hides"
        );
    }

    #[test]
    fn a_hidden_idle_console_ends_after_the_grace_period_and_not_before() {
        let before = HIDDEN_IDLE_GRACE - Duration::from_secs(1);
        assert!(!quit_when_hidden_idle(false, false, Some(before)));
        assert!(quit_when_hidden_idle(false, false, Some(HIDDEN_IDLE_GRACE)));
        assert!(quit_when_hidden_idle(
            false,
            false,
            Some(HIDDEN_IDLE_GRACE * 3)
        ));
    }

    #[test]
    fn it_never_ends_while_it_is_shown_busy_or_has_not_been_idle() {
        let long = HIDDEN_IDLE_GRACE * 10;
        assert!(
            !quit_when_hidden_idle(true, false, Some(long)),
            "the person is looking at it"
        );
        assert!(
            !quit_when_hidden_idle(false, true, Some(long)),
            "a copy is running"
        );
        assert!(
            !quit_when_hidden_idle(false, false, None),
            "it has not been idle"
        );
    }
}
