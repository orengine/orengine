use crate::runtime::task::task_data::TaskData;
use crate::runtime::{Locality, Task};
use ahash::AHashMap;
use std::cell::UnsafeCell;
use std::future::Future;
use std::mem::size_of;

/// A pool of tasks.
pub struct TaskPool {
    /// Key is a size.
    storage: AHashMap<usize, Vec<*mut ()>>,
}

thread_local! {
    /// A thread-local task pool. So it is lockless.
    pub(crate) static TASK_POOL: UnsafeCell<Option<TaskPool>> = UnsafeCell::new(None);
}

/// Returns the thread-local task pool wrapped in an [`Option`].
#[inline(always)]
pub fn get_task_pool_ref() -> &'static mut Option<TaskPool> {
    TASK_POOL.with(|task_pool| unsafe { &mut *task_pool.get() })
}

/// Returns `&'static mut TaskPool` of the current thread.
///
/// # Safety
///
/// [`TASK_POOL`](TASK_POOL) must be initialized.
#[inline(always)]
pub fn task_pool() -> &'static mut TaskPool {
    #[cfg(debug_assertions)]
    {
        get_task_pool_ref().as_mut().expect(crate::BUG_MESSAGE)
    }

    #[cfg(not(debug_assertions))]
    unsafe {
        get_task_pool_ref().as_mut().unwrap_unchecked()
    }
}

impl TaskPool {
    /// Initializes a new `TaskPool` in the current thread if it is not initialized.
    pub fn init() {
        if get_task_pool_ref().is_none() {
            *get_task_pool_ref() = Some(TaskPool {
                storage: AHashMap::new(),
            });
        }
    }

    /// Returns a [`Task`] with the given future.
    #[inline(always)]
    pub fn acquire<F: Future<Output = ()>>(&mut self, future: F, locality: Locality) -> Task {
        let size = size_of::<F>();
        #[cfg(debug_assertions)]
        let executor_id = if cfg!(test) {
            usize::MAX
        } else {
            crate::local_executor().id()
        };

        let pool = self.storage.entry(size).or_insert_with(|| Vec::new());
        if let Some(slot_ptr) = pool.pop() {
            let future_ptr: *mut F = unsafe { &mut *(slot_ptr as *mut F) };
            unsafe {
                future_ptr.write(future);
            }

            Task {
                data: TaskData::new(future_ptr as *mut _, locality),
                #[cfg(debug_assertions)]
                executor_id,
                #[cfg(debug_assertions)]
                ref_count: std::sync::Arc::new(()),
            }
        } else {
            let future_ptr: *mut F = unsafe { &mut *(Box::into_raw(Box::new(future))) as *mut _ };
            Task {
                data: TaskData::new(future_ptr as *mut _, locality),
                #[cfg(debug_assertions)]
                executor_id,
                #[cfg(debug_assertions)]
                ref_count: std::sync::Arc::new(()),
            }
        }
    }

    /// Puts a task into the pool.
    #[inline(always)]
    pub fn put(&mut self, ptr: *mut dyn Future<Output = ()>) {
        let size = size_of_val(unsafe { &*ptr });
        if let Some(pool) = self.storage.get_mut(&size) {
            pool.push(ptr as *mut ());
            return;
        }

        // A task that have been allocated in another thread ended up here

        self.storage.insert(size, vec![ptr as *mut ()]);
    }
}
