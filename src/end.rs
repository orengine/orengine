use std::cell::UnsafeCell;
use std::sync::atomic::AtomicBool;

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

static GLOBAL_END: AtomicBool = AtomicBool::new(false);

#[inline(always)]
pub fn global_was_end() -> bool {
    GLOBAL_END.load(std::sync::atomic::Ordering::Relaxed)
}

#[inline(always)]
pub fn set_global_was_end(was_end: bool) {
    GLOBAL_END.store(was_end, std::sync::atomic::Ordering::Relaxed);
}

#[inline(always)]
pub fn end() {
    GLOBAL_END.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use crate::runtime::{local_executor};
    use crate::Executor;
    use super::*;

    #[test]
    fn test_end_local_thread() {
        Executor::init();

        local_executor().spawn_local(async {
            assert!(!was_ended());
            end_local_thread();
            assert!(was_ended());
        });

        local_executor().run();
    }
}