//! Where each job of a batch stands while a configuration with several jobs runs
//! (CATALOGO_COMPORTAMENTI_GUI.md L43): waiting, running, done. The numbers come from the progress
//! samples the command line already publishes (`batch_index`, 1-based); this only turns one number
//! into a row per job, so it is pure and unit-tested.
//!
//! It is drawn **only while the run is going**. When it ends, the window leaves for the copy's own
//! page, and what each job ended in lives where it always did: the job page, from each job's own
//! report. Nothing here guesses an outcome.

/// One job's place in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueState {
    Waiting,
    Running,
    Done,
}

impl QueueState {
    pub fn label(self) -> &'static str {
        match self {
            QueueState::Waiting => "in attesa",
            QueueState::Running => "in corso",
            QueueState::Done => "concluso",
        }
    }
}

/// The state of each of `total` jobs when the batch is on job `current` (1-based). Before the first
/// sample there is no `current`: the batch has begun, so the first job is the one running.
pub fn states(total: usize, current: Option<usize>) -> Vec<QueueState> {
    let running = current.unwrap_or(1).clamp(1, total.max(1));
    (1..=total)
        .map(|position| {
            if position < running {
                QueueState::Done
            } else if position == running {
                QueueState::Running
            } else {
                QueueState::Waiting
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use QueueState::{Done, Running, Waiting};

    #[test]
    fn jobs_before_the_current_one_are_done_and_the_ones_after_it_wait() {
        assert_eq!(states(4, Some(3)), vec![Done, Done, Running, Waiting]);
    }

    #[test]
    fn before_the_first_sample_the_first_job_is_the_one_running() {
        assert_eq!(states(3, None), vec![Running, Waiting, Waiting]);
    }

    #[test]
    fn an_index_outside_the_batch_never_invents_a_job() {
        assert_eq!(states(2, Some(9)), vec![Done, Running]);
        assert_eq!(states(2, Some(0)), vec![Running, Waiting]);
        assert!(states(0, Some(1)).is_empty());
    }

    #[test]
    fn the_labels_are_the_words_the_console_uses_elsewhere() {
        assert_eq!(Waiting.label(), "in attesa");
        assert_eq!(Running.label(), "in corso");
        assert_eq!(Done.label(), "concluso");
    }
}
