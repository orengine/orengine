use std::io::Result;
use std::mem;
use crate::runtime::task::Task;

/// Default value of [`IoRequestData::ret`].
pub(crate) const UNINIT_RESULT: Result<usize> = Ok(1 << 32 - 1);

/// Data of io request. It contains a result and a task.
/// After the task is done, the result will be set and the task will be executed.
pub(crate) struct IoRequestData {
    ret: Result<usize>,
    task: Task
}

impl IoRequestData {
    /// Returns a new [`IoRequestData`] with the given task.
    #[inline(always)]
    pub(crate) fn new(task: Task) -> Self {
        Self {
            ret: UNINIT_RESULT,
            task
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

    /// Returns the task that must be executed.
    #[inline(always)]
    pub(crate) fn task(&self) -> Task {
        self.task
    }
}