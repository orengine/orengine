use crate::utils::SpinLock;
use std::sync::Arc;

/// `DroppableElement` writes `value` into `drop_in` on [`Drop`].
///
/// # Usage
///
/// It is used to test [`Drop`] implementations.
pub(crate) struct DroppableElement {
    pub(crate) value: usize,
    pub(crate) drop_in: Arc<SpinLock<Vec<usize>>>,
}

impl DroppableElement {
    /// Creates a new `DroppableElement`.
    pub(crate) fn new(value: usize, drop_in: Arc<SpinLock<Vec<usize>>>) -> Self {
        Self { value, drop_in }
    }
}

impl Drop for DroppableElement {
    fn drop(&mut self) {
        self.drop_in.lock().push(self.value);
    }
}
