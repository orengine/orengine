/// The result of [`send_task_to`].
///
/// It can be [`SendTaskResult::Ok`] or [`SendTaskResult::ExecutorIsNotRegistered`].
///
/// [`send_task_to`]: crate::runtime::interaction_between_executors::Interactor::send_task_to
pub enum SendTaskResult {
    /// The task was sent successfully.
    Ok,
    /// The executor with the given id is not registered.
    ExecutorIsNotRegistered,
}

impl SendTaskResult {
    /// Returns `true` if the result is [`Self::Ok`].
    #[inline]
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    /// If the result is [`Self::ExecutorIsNotRegistered`],
    /// panics with the given message.
    ///
    /// # Panics
    ///
    /// Panics if the result is [`Self::ExecutorIsNotRegistered`].
    pub fn expect(self, msg: &str) {
        assert!(self.is_ok(), "{}", msg);
    }

    /// If the result is [`Self::ExecutorIsNotRegistered`],
    /// calls the given function.
    pub fn or_else(self, f: impl FnOnce() -> Self) {
        if !self.is_ok() {
            f();
        }
    }
}
