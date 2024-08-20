use std::cell::UnsafeCell;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;

static END: AtomicBool = AtomicBool::new(false);

#[inline(always)]
pub(crate) fn was_ended() -> bool {
    END.load(Relaxed)
}

pub(crate) fn set_was_ended(was_ended: bool) {
    END.store(was_ended, Relaxed);
}

#[inline(always)]
pub fn end_local_thread() {
    set_was_ended(true);
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