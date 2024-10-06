use std::time::{Duration, Instant};

/// [`TimeBoundedIoTask`] contains the deadline and user data for cancelling the task.
#[derive(Clone)]
pub(crate) struct TimeBoundedIoTask {
    deadline: Instant,
    /// User data is used to cancel the task if needed.
    user_data: u64,
}

impl TimeBoundedIoTask {
    /// Creates a new [`TimeBoundedIoTask`]
    #[inline(always)]
    pub(crate) fn new(deadline: Instant, user_data: u64) -> Self {
        Self {
            deadline,
            user_data,
        }
    }

    /// Increments the deadline by 1 nanosecond
    #[inline(always)]
    pub(crate) fn inc_deadline(&mut self) {
        self.deadline = self.deadline + Duration::from_nanos(1);
    }

    /// Returns the user data.
    #[inline(always)]
    pub(crate) fn user_data(&self) -> u64 {
        self.user_data
    }

    /// Sets the user data.
    #[inline(always)]
    pub(crate) fn set_user_data(&mut self, user_data: u64) {
        self.user_data = user_data
    }

    /// Returns the deadline.
    #[inline(always)]
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
}

impl PartialEq for TimeBoundedIoTask {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}

impl PartialOrd for TimeBoundedIoTask {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.deadline.partial_cmp(&other.deadline)
    }
}

impl Eq for TimeBoundedIoTask {}

impl Ord for TimeBoundedIoTask {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.deadline.cmp(&other.deadline)
    }
}
