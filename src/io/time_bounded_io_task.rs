use crate::io::io_request_data::IoRequestData;
#[cfg(not(target_os = "linux"))]
use crate::io::io_request_data::IoRequestDataPtr;
#[cfg(not(target_os = "linux"))]
use crate::io::sys::fallback::with_thread_pool::io_call::IoCall;
use std::borrow::Borrow;
use std::time::Instant;

/// [`TimeBoundedIoTask`] contains the deadline and user data for cancelling the task.
#[derive(Clone)]
pub(crate) struct TimeBoundedIoTask {
    deadline: Instant,
    /// User data is used to cancel the task if needed.
    user_data: u64,
    #[cfg(not(target_os = "linux"))]
    raw_socket: crate::io::sys::RawSocket,
    #[cfg(not(target_os = "linux"))]
    slot_ptr: *mut (IoCall, IoRequestDataPtr),
}

impl TimeBoundedIoTask {
    /// Creates a new [`TimeBoundedIoTask`]
    #[inline(always)]
    #[cfg(target_os = "linux")]
    pub(crate) fn new(io_request_data: &IoRequestData, deadline: Instant) -> Self {
        Self {
            deadline,
            user_data: std::ptr::from_ref(io_request_data) as u64,
        }
    }

    /// Creates a new [`TimeBoundedIoTask`]
    #[inline(always)]
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn new(
        io_request_data: &IoRequestData,
        deadline: Instant,
        raw_socket: crate::io::sys::RawSocket,
        slot_ptr: *mut (IoCall, IoRequestDataPtr),
    ) -> Self {
        Self {
            deadline,
            user_data: std::ptr::from_ref(io_request_data) as u64,
            raw_socket,
            slot_ptr,
        }
    }

    /// Returns the user data.
    #[inline(always)]
    pub(crate) fn user_data(&self) -> u64 {
        self.user_data
    }

    /// Returns the deadline.
    #[inline(always)]
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns the raw socket.
    #[inline(always)]
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn raw_socket(&self) -> crate::io::sys::RawSocket {
        self.raw_socket
    }

    /// Returns the slot pointer.
    #[inline(always)]
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn slot_ptr(&self) -> *mut (IoCall, IoRequestDataPtr) {
        self.slot_ptr
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
        Some(self.cmp(other))
    }
}

impl Eq for TimeBoundedIoTask {}

impl Ord for TimeBoundedIoTask {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.deadline.cmp(&other.deadline)
    }
}

impl Borrow<Instant> for TimeBoundedIoTask {
    #[inline(always)]
    fn borrow(&self) -> &Instant {
        &self.deadline
    }
}
