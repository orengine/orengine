use std::future::Future;
use std::mem::size_of;
use ahash::AHashMap;
use crate::runtime::Task;

/// A pool of tasks.
pub struct TaskPool {
    /// Key is a size.
    storage: AHashMap<usize, Vec<*mut ()>>,
}

/// A thread-local task pool. So it is lockless.
#[thread_local]
pub static mut TASK_POOL: Option<TaskPool> = None;

/// Returns `&'static mut TaskPool` of the current thread.
///
/// # Safety
///
/// [`TASK_POOL`](TASK_POOL) must be initialized.
#[inline(always)]
pub fn task_pool() -> &'static mut TaskPool {
    #[cfg(debug_assertions)]
    unsafe {
        TASK_POOL.as_mut().expect(crate::messages::BUG)
    }

    #[cfg(not(debug_assertions))]
    unsafe {
        crate::runtime::task_pool::TASK_POOL
            .as_mut()
            .unwrap_unchecked()
    }
}

impl TaskPool {
    /// Initializes a new `TaskPool` in the current thread if it is not initialized.
    pub fn init() {
        unsafe {
            if TASK_POOL.is_none() {
                TASK_POOL = Some(TaskPool {
                    storage: AHashMap::new(),
                });
            }
        }
    }

    /// Returns a [`Task`] with the given future.
    #[inline(always)]
    pub fn acquire<F: Future<Output = ()>>(&mut self, future: F) -> Task {
        let size = size_of::<F>();

        let pool = self.storage.entry(size).or_insert_with(|| Vec::new());
        if let Some(slot_ptr) = pool.pop() {
            let future_ptr: *mut F = unsafe { &mut *(slot_ptr as *mut F) };
            // TODO *slot = future; // Maybe rewrite, not write
            unsafe {
                future_ptr.write(future);
            }
            Task {
                future_ptr: future_ptr as *mut _,
                #[cfg(debug_assertions)]
                executor_id: crate::local_executor().id(),
                #[cfg(debug_assertions)]
                is_local: true,
            }
        } else {
            let future_ptr: *mut F = unsafe { &mut *(Box::into_raw(Box::new(future))) as *mut _ };
            Task {
                future_ptr: future_ptr as *mut _,
                #[cfg(debug_assertions)]
                executor_id: crate::local_executor().id(),
                #[cfg(debug_assertions)]
                is_local: true,
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
