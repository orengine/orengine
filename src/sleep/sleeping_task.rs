use std::time::Instant;
use crate::runtime::task::Task;

/// `SleepingTask` is a wrapper of a task that contains `time to sleep`
/// which is used to wake the task after some time.
pub(crate) struct SleepingTask {
    time_to_wake: Instant,
    task: Task
}

impl SleepingTask {
    /// Creates new [`SleepingTask`].
    #[inline(always)]
    pub(crate) fn new(time_to_wake: Instant, task: Task) -> Self {
        Self {
            time_to_wake,
            task
        }
    }

    /// Returns the associated task.
    #[inline(always)]
    pub(crate) fn task(self) -> Task {
        self.task
    }

    /// Returns the time to wake.
    #[inline(always)]
    pub(crate) fn time_to_wake(&self) -> Instant {
        self.time_to_wake
    }
}

impl PartialEq for SleepingTask {
    fn eq(&self, other: &Self) -> bool {
        self.time_to_wake == other.time_to_wake
    }
}

impl PartialOrd for SleepingTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.time_to_wake.partial_cmp(&other.time_to_wake)
    }
}

impl Eq for SleepingTask {}

impl Ord for SleepingTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time_to_wake.cmp(&other.time_to_wake)
    }
}