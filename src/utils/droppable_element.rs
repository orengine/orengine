use std::sync::Arc;
use crate::utils::SpinLock;

pub(crate) struct DroppableElement {
    pub(crate) value: usize,
    pub(crate) drop_in: Arc<SpinLock<Vec<usize>>>
}

impl DroppableElement {
    pub(crate) fn new(value: usize, drop_in: Arc<SpinLock<Vec<usize>>>) -> Self {
        Self { value, drop_in }
    }
}

impl Drop for DroppableElement {
    fn drop(&mut self) {
        self.drop_in.lock().push(self.value)
    }
}