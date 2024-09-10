use std::intrinsics::likely;
use crate::runtime::Task;
use crate::utils::SpinLock;

pub(crate) struct SharedTaskList {
    list: SpinLock<Vec<Task>>
}

impl SharedTaskList {
    pub(crate)  const fn new() -> Self {
        Self {
            list: SpinLock::new(Vec::new())
        }
    }

    #[inline(always)]
    pub(crate)  fn push(&self, task: Task) {
        self.list.lock().push(task);
    }

    #[inline(always)]
    /// # Safety
    ///
    /// other_list must have at least `limit` reserved capacity
    pub(crate)  unsafe fn take_batch(&self, other_list: &mut Vec<Task>, limit: usize) {
        let mut guard = self.list.lock();

        if likely(guard.len() <= limit) {
            let src = guard.as_mut_ptr();
            let dst = unsafe { other_list.as_mut_ptr().add(other_list.len()) };

            unsafe {
                other_list.set_len(other_list.len() + guard.len());
                std::ptr::copy_nonoverlapping(src, dst, guard.len());
                guard.set_len(0);
            }
        } else {
            let src = unsafe { guard.as_mut_ptr().add(guard.len() - limit) };
            let dst = unsafe { other_list.as_mut_ptr().add(other_list.len()) };

            unsafe {
                other_list.set_len(other_list.len() + limit);
                std::ptr::copy_nonoverlapping(src, dst, limit);
                let len = guard.len() - limit;
                guard.set_len(len);
            }
        }
    }
}

unsafe impl Send for SharedTaskList {}
unsafe impl Sync for SharedTaskList {}