/// `SendCallState` is a state machine for [`WaitSend`] or [`WaitLocalSend`].
/// This is used to improve performance.
///
/// [`WaitSend`]: crate::sync::channels::WaitSend
/// [`WaitLocalSend`]: crate::sync::channels::WaitLocalSend
pub(super) enum SendCallState {
    /// Default state.
    ///
    /// # Lock note
    ///
    /// It has no lock now.
    FirstCall,
    /// Receiver writes the value associated with this task
    ///
    /// # Lock note
    ///
    /// It has no lock now.
    WokenToReturnReady,
    /// This task was enqueued, now it is woken by close, and it has no lock now.
    WokenByClose,
}

/// `RecvCallState` is a state machine for [`WaitRecv`] or [`WaitLocalRecv`].
/// This is used to improve performance.
///
/// [`WaitRecv`]: crate::sync::channels::WaitRecv
/// [`WaitLocalRecv`]: crate::sync::channels::WaitLocalRecv
pub(super) enum RecvCallState {
    /// Default state.
    ///
    /// # Lock note
    ///
    /// It has no lock now.
    FirstCall,
    /// This task was enqueued, now it is woken for return [`Poll::Ready`],
    /// because a [`WaitSend`] or [`WaitLocalSend`] has written to the slot already.
    ///
    /// # Lock note
    ///
    /// And it has no lock now.
    ///
    /// [`WaitSend`]: crate::sync::channels::WaitSend
    /// [`WaitLocalSend`]: crate::sync::channels::WaitLocalSend
    WokenToReturnReady,
    /// This task was enqueued, now it is woken by close.
    ///
    /// # Lock note
    ///
    /// It has no lock now.
    WokenByClose,
}
