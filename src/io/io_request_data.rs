use crate::runtime::task::Task;
use std::io::Result;
use std::{mem, ptr};

/// Default value of [`IoRequestData::ret`].
pub(crate) const UNINIT_RESULT: Result<usize> = Ok(1 << 32 - 1);

/// Data of io request. It contains a result and a task.
/// After the task is done, the result will be set and the task will be executed.
pub(crate) struct IoRequestData {
    ret: Result<usize>,
    task: Task,
    #[cfg(debug_assertions)]
    was_executed: bool,
}

impl IoRequestData {
    /// Returns a new [`IoRequestData`] with the given task.
    #[inline(always)]
    pub(crate) fn new(task: Task) -> Self {
        Self {
            ret: UNINIT_RESULT,
            task,
            #[cfg(debug_assertions)]
            was_executed: false,
        }
    }

    /// Checks whether an associated task has been read to execute. If yes, it panics.
    #[inline(always)]
    fn check_if_executed_in_debug(&mut self) {
        #[cfg(debug_assertions)]
        {
            if self.was_executed {
                panic!("{}", crate::bug_message::BUG_MESSAGE);
            }
            
            self.was_executed = true;
        }
    }

    /// Sets the result.
    #[inline(always)]
    pub(crate) fn set_ret(&mut self, ret: Result<usize>) {
        self.ret = ret;
    }

    /// Returns the result.
    #[inline(always)]
    pub(crate) fn ret(&mut self) -> Result<usize> {
        mem::replace(&mut self.ret, UNINIT_RESULT)
    }

    /// Returns an associated task.
    ///
    /// # Safety
    ///
    /// This method calls only once for one instance of [`IoRequestData`].
    #[inline(always)]
    pub(crate) unsafe fn task(&mut self) -> Task {
        self.check_if_executed_in_debug();

        unsafe { ptr::read(&self.task) }
    }
}
