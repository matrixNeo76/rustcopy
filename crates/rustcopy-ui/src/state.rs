//! Interface state helpers that carry the console's hard-won behaviours (CATALOGO_COMPORTAMENTI_GUI.md
//! G07, E01, E03). Pure, no Slint types, so they are unit-tested without a window.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Discards stale asynchronous replies (E01).
///
/// Every read started from the interface takes a ticket with [`Generation::next`]; when the reply
/// arrives, only the newest ticket is allowed to update the screen. Cheap to clone: the counter is
/// shared, so a worker thread can hold its own handle.
#[derive(Clone, Default)]
pub struct Generation(Arc<AtomicU64>);

impl Generation {
    /// Starts a new request and invalidates every earlier one.
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// `true` when `ticket` is still the newest request.
    pub fn is_current(&self, ticket: u64) -> bool {
        self.0.load(Ordering::SeqCst) == ticket
    }
}

/// Something that can say whether it is still running (a child process, in practice).
pub trait Running {
    fn is_running(&mut self) -> bool;
}

/// Holds at most one active run (G07).
///
/// The "is something already running?" check and the assignment of the new run happen under **one**
/// borrow, so two quick clicks cannot both pass the check and the second cannot overwrite a child
/// that no window could stop any more.
pub struct RunSlot<T: Running> {
    inner: RefCell<Option<T>>,
}

impl<T: Running> Default for RunSlot<T> {
    fn default() -> Self {
        Self {
            inner: RefCell::new(None),
        }
    }
}

impl<T: Running> RunSlot<T> {
    /// Starts a run with `make` unless one is still running. A finished one is replaced.
    pub fn try_start(&self, make: impl FnOnce() -> Result<T, String>) -> Result<(), String> {
        let mut slot = self.inner.borrow_mut();
        if let Some(active) = slot.as_mut() {
            if active.is_running() {
                return Err("un backup è già in corso in questa finestra".to_string());
            }
        }
        *slot = Some(make()?);
        Ok(())
    }

    /// Runs `f` on the active run, if any.
    pub fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        self.inner.borrow_mut().as_mut().map(f)
    }

    /// Removes and returns the active run.
    pub fn take(&self) -> Option<T> {
        self.inner.borrow_mut().take()
    }
}

/// The progress bar's fraction while a run is active (E03): never 1.0.
///
/// Robocopy counts directory entries the inventory total does not, so `bytes_done` legitimately
/// passes `bytes_total` before the transfer ends; "100 %" is shown only when the run is over.
/// `None` (unknown total) maps to `-1.0`, which the interface renders as an indeterminate bar.
pub fn running_fraction(fraction: Option<f64>) -> f32 {
    fraction.map_or(-1.0, |f| f.clamp(0.0, 0.99) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        alive: bool,
    }
    impl Running for Fake {
        fn is_running(&mut self) -> bool {
            self.alive
        }
    }

    #[test]
    fn only_the_newest_ticket_is_current() {
        let generation = Generation::default();
        let first = generation.next();
        let second = generation.next();
        assert!(
            !generation.is_current(first),
            "an older reply must be discarded"
        );
        assert!(generation.is_current(second));
    }

    #[test]
    fn a_clone_shares_the_counter() {
        let generation = Generation::default();
        let worker = generation.clone();
        let ticket = worker.next();
        assert!(generation.is_current(ticket));
        generation.next();
        assert!(!worker.is_current(ticket));
    }

    #[test]
    fn a_second_start_is_refused_while_the_first_runs() {
        let slot: RunSlot<Fake> = RunSlot::default();
        slot.try_start(|| Ok(Fake { alive: true }))
            .expect("first start");
        let mut built = false;
        let second = slot.try_start(|| {
            built = true;
            Ok(Fake { alive: true })
        });
        assert!(second.is_err());
        assert!(!built, "the second run must not even be constructed");
    }

    #[test]
    fn a_finished_run_is_replaced() {
        let slot: RunSlot<Fake> = RunSlot::default();
        slot.try_start(|| Ok(Fake { alive: false }))
            .expect("first start");
        slot.try_start(|| Ok(Fake { alive: true }))
            .expect("a finished run does not block the next");
        assert_eq!(slot.with(|run| run.is_running()), Some(true));
    }

    #[test]
    fn a_failed_start_leaves_the_slot_untouched() {
        let slot: RunSlot<Fake> = RunSlot::default();
        slot.try_start(|| Ok(Fake { alive: false }))
            .expect("first start");
        assert!(slot.try_start(|| Err("boom".to_string())).is_err());
    }

    #[test]
    fn the_running_fraction_never_reaches_one() {
        assert_eq!(running_fraction(None), -1.0);
        assert_eq!(running_fraction(Some(0.5)), 0.5);
        assert_eq!(running_fraction(Some(1.0)), 0.99);
        assert_eq!(running_fraction(Some(7.0)), 0.99);
        assert_eq!(running_fraction(Some(-1.0)), 0.0);
    }
}
