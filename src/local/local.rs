#![allow(dead_code)]
// TODO docs

// TODO run tests (tests had been written, but hadn't been run)

// TODO make it pub in mod.rs

use std::fmt::Debug;
use crate::utils::Ptr;

struct Inner<T> {
    counter: usize,
    data: T
}

pub struct Local<T> {
    inner: Ptr<Inner<T>>
}

impl<T> Local<T> {
    pub fn new(data: T) -> Self {
        Local {
            inner: Ptr::new(Inner { data, counter: 1 })
        }
    }
    
    #[inline(always)]
    fn inc_counter(&self) {
        unsafe { self.inner.as_mut().counter += 1; }
    }
    
    #[inline(always)]
    fn dec_counter(&self) -> usize {
        let reference = unsafe { self.inner.as_mut() };
        reference.counter -= 1;
        reference.counter
    }
    
    #[inline(always)]
    pub fn get<'a>(&self) -> &'a T {
        unsafe { &self.inner.as_ref().data }
    }
    
    #[inline(always)]
    pub fn get_mut<'a>(&self) -> &'a mut T {
        unsafe { &mut self.inner.as_mut().data }
    }
}

impl<T: Default> Default for Local<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Debug> Debug for Local<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}

impl<T> Clone for Local<T> {
    fn clone(&self) -> Self {
        self.inc_counter();
        Self {
            inner: self.inner
        }
    }
}

impl<T> Drop for Local<T> {
    fn drop(&mut self) {
        if self.dec_counter() == 0 {
            unsafe {
                self.inner.drop_and_deallocate();
            }
        }
    }
}

impl<T> !Send for Local<T> {}
unsafe impl<T> Sync for Local<T> {}
//
// #[cfg(test)]
// mod tests {
//     use proc::{test_local};
//     use super::*;
//
//     #[test]
//     #[should_panic(expected = "Cannot create local data a thread, that has not start Scheduler! Please call run_on_core on run_on_all_cores first.")]
//     fn test_new_panic_on_zero_worker_id() {
//         Local::new(5);
//     }
//
//     #[test_local(crate="crate")]
//     fn test_new_success() {
//         let local = Local::new(5);
//         assert_eq!(local.worker_id, 1);
//         let reference = unsafe { local.inner.as_ref() };
//         assert_eq!(reference.data, 5);
//         assert_eq!(reference.counter, 1);
//     }
//
//     // TODO after coroutine leak
//     // #[test]
//     // #[should_panic(expected = "Tried to inc_counter from another worker!")]
//     // fn test_inc_counter_panic_on_wrong_worker_id() {
//     //     let local = Local::new(5);
//     //     local.inc_counter();
//     // }
//
//     #[test_local(crate="crate")]
//     fn test_inc_counter() {
//         let local = Local::new(5);
//         local.inc_counter();
//         assert_eq!(unsafe { local.inner.as_ref().counter }, 2);
//     }
//
//     // TODO after coroutine leak
//     // #[test]
//     // #[should_panic(expected = "Tried to dec_counter from another worker!")]
//     // fn test_dec_counter_panic_on_wrong_worker_id() {
//     //     let local = Local::new(5);
//     //     local.dec_counter();
//     // }
//
//     #[test_local(crate="crate")]
//     // While dropping we try to dec counter. This code should panic, because we try to get 0 - 1.
//     #[should_panic(expected = "attempt to subtract with overflow")]
//     fn test_dec_counter() {
//         let local = Local::new(5);
//         assert_eq!(local.dec_counter(), 0);
//     }
//
//     #[test_local(crate="crate")]
//     fn test_check_worker_id() {
//         let local = Local::new(5);
//         assert!(local.check_worker_id());
//     }
//
//     // TODO after coroutine leak
//     // #[test]
//     // #[should_panic(expected = "Tried to get local data from another worker!")]
//     // fn test_get_panic_on_wrong_worker_id() {
//     //     let local = Local::new(5);
//     //     local.get();
//     // }
//
//     #[test_local(crate="crate")]
//     fn test_get_success() {
//         let local = Local::new(5);
//         assert_eq!(*local.get(), 5);
//     }
//
//     // TODO after coroutine leak
//     // #[test]
//     // #[should_panic(expected = "Tried to get_mut local data from another worker!")]
//     // fn test_get_mut_panic_on_wrong_worker_id() {
//     //     let local = Local::new(5);
//     //     local.get_mut();
//     // }
//
//     #[test_local(crate="crate")]
//     fn test_get_mut_success() {
//         let local = Local::new(5);
//         *local.get_mut() = 10;
//         assert_eq!(*local.get(), 10);
//     }
//
//     #[test_local(crate="crate")]
//     fn test_default() {
//         let local: Local<i32> = Local::default();
//         assert_eq!(*local.get(), 0);
//     }
//
//     #[test_local(crate="crate")]
//     fn test_debug() {
//         let local = Local::new(5);
//         assert_eq!(format!("{:?}", local), "5");
//     }
//
//     #[test_local(crate="crate")]
//     fn test_clone() {
//         let local = Local::new(5);
//         let local_clone = local.clone();
//         assert_eq!(unsafe { local.inner.as_ref().counter }, 2);
//         assert_eq!(local.worker_id, local_clone.worker_id);
//         assert_eq!(unsafe { local.inner.as_ref().data }, unsafe { local_clone.inner.as_ref().data });
//     }
//
//     #[test_local(crate="crate")]
//     #[should_panic(expected = "MustDrop was dropped")]
//     fn test_drop() {
//         struct MustDrop {
//             counter: u32
//         }
//
//         impl Drop for MustDrop {
//             fn drop(&mut self) {
//                 if self.counter != 0 {
//                     panic!("counter is not 0. So, probable, it was dropped before the loop below was finished. \
//                     In this test we expect the counter to be 0, to check that it was dropped only after all local clones were dropped.");
//                 }
//                 panic!("MustDrop was dropped");
//             }
//         }
//
//         let local = Local::new(MustDrop { counter: 5 });
//
//         for i in 0..5 {
//             let md = local.clone().get_mut();
//             assert_eq!(md.counter, 5 - i);
//             md.counter -= 1;
//         }
//     }
// }