use std::io::Result;
use std::mem;
use crate::runtime::task::Task;

pub(crate) const UNINIT_RESULT: Result<usize> = Ok(1 << 32 - 1);

pub(crate) struct IoRequest {
    ret: Result<usize>,
    task: Task
}

impl IoRequest {
    #[inline(always)]
    pub(crate) fn new(task: Task) -> Self {
        Self {
            ret: UNINIT_RESULT,
            task
        }
    }

    #[inline(always)]
    pub(crate) fn set_ret(&mut self, ret: Result<usize>) {
        self.ret = ret;
    }

    #[inline(always)]
    pub(crate) fn ret(&mut self) -> Result<usize> {
        mem::replace(&mut self.ret, UNINIT_RESULT)
    }

    #[inline(always)]
    pub(crate) fn task(&self) -> Task {
        self.task
    }
}