use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed};

use crossbeam::utils::CachePadded;

use crate::sync::local::once::OnceState;

pub struct Once {
    state: CachePadded<AtomicIsize>,
}

impl Once {
    pub const fn new() -> Once {
        Once {
            state: CachePadded::new(AtomicIsize::new(OnceState::not_called())),
        }
    }

    #[inline(always)]
    pub fn call_once<F: FnOnce()>(&self, f: F) -> Result<(), ()> {
        if self
            .state
            .compare_exchange(
                OnceState::NotCalled.into(),
                OnceState::Called.into(),
                Acquire,
                Relaxed,
            )
            .is_ok()
        {
            f();
            Ok(())
        } else {
            Err(())
        }
    }

    #[inline(always)]
    pub fn call_once_force<F: FnOnce(&AtomicIsize)>(&self, f: F) -> Result<(), ()> {
        if self
            .state
            .compare_exchange(
                OnceState::NotCalled.into(),
                OnceState::Called.into(),
                Acquire,
                Relaxed,
            )
            .is_ok()
        {
            f(&self.state);
            Ok(())
        } else {
            Err(())
        }
    }

    #[inline(always)]
    pub fn state(&self) -> OnceState {
        OnceState::from(self.state.load(Acquire))
    }

    #[inline(always)]
    pub fn was_called(&self) -> bool {
        self.state() == OnceState::Called
    }
}

unsafe impl Sync for Once {}
unsafe impl Send for Once {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering::SeqCst;
    use std::sync::Arc;
    use std::thread;
    use crate::{Executor};
    use crate::sync::WaitGroup;

    use super::*;

    #[orengine_macros::test_global]
    fn test_local_once() {
        let a = Arc::new(AtomicBool::new(false));
        let wg = Arc::new(WaitGroup::new());
        let once = Arc::new(Once::new());
        assert_eq!(once.state(), OnceState::NotCalled);
        assert!(!once.was_called());

        for _ in 0..10 {
            let a = a.clone();
            let wg = wg.clone();
            wg.add(1);
            let once = once.clone();
            thread::spawn(move || {
                Executor::init().run_with_global_future(async move {
                    let _ = once.call_once(|| {
                        assert!(!a.load(SeqCst));
                        a.store(true, SeqCst);
                    });
                    wg.done();
                });
            });
        }

        let _ = wg.wait().await;
        assert!(once.was_called());
        assert_eq!(once.call_once(|| ()), Err(()));
    }
}
