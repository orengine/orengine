use std::cell::UnsafeCell;

thread_local! {
    static END: UnsafeCell<bool> = UnsafeCell::new(false);
}

#[inline(always)]
pub(crate) fn was_ended() -> bool {
    unsafe {
        END.with(|end| {
            *end.get()
        })
    }
}

pub(crate) fn set_was_ended(was_ended: bool) {
    unsafe {
        END.with(|end| {
            *end.get() = was_ended;
        })
    }
}

#[inline(always)]
pub fn end_local_thread() {
    unsafe {
        END.with(|end| {
            *end.get() = true;
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::{local_executor};
    use crate::{Executor, yield_now};
    use super::*;

    #[test]
    fn test_end_local_thread() {
        Executor::init();

        local_executor().spawn_local(async {
            let i = 0;
            assert!(!was_ended());
            end_local_thread();
            assert!(was_ended());
        });

        local_executor().run();
    }
}