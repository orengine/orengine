use std::future::Future;
use std::mem;
use ahash::AHashMap;
use crate::messages::BUG;


pub(crate) struct TaskPool {
    /// Key is a size.
    storage: AHashMap<usize, Vec<*mut ()>>
}

#[thread_local]
pub(crate) static mut TASK_POOL: Option<TaskPool> = None;

#[inline(always)]
// TODO rewrite LOCAL_EXECUTER and LOCAL_WORKER as it
pub(crate) fn task_pool() -> &'static mut TaskPool {
    #[cfg(debug_assertions)]
    unsafe { TASK_POOL.as_mut().expect(BUG) }

    #[cfg(not(debug_assertions))]
    unsafe { crate::runtime::task_pool::TASK_POOL.as_mut().unwrap_unchecked() }
}

impl TaskPool {
    pub(crate) fn init() {
        unsafe {
            TASK_POOL = Some(TaskPool {
                storage: AHashMap::new()
            });
        }
    }

    #[inline(always)]
    pub(crate) fn acquire<F: Future<Output=()> + 'static>(&mut self, future: F) -> *mut dyn Future<Output=()> {
        let size = mem::size_of::<F>();

        let pool = self.storage.entry(size).or_insert_with(|| Vec::new());
        if let Some(slot_ptr) = pool.pop() {
            let slot = unsafe {&mut *(slot_ptr as *mut F)};
            //*slot = future; // rewrite, not write
            unsafe { (slot as *mut F).write(future); }
            slot
        } else {
            Box::into_raw(Box::new(future))
        }
    }

    #[inline(always)]
    pub(crate) fn put(&mut self, ptr: *mut dyn Future<Output=()>) {
        let size = mem::size_of_val(unsafe { &*ptr });
        if let Some(pool) = self.storage.get_mut(&size) {
            pool.push(ptr as *mut ());
            return
        }

        // A task that have been allocated in another thread ended up here

        self.storage.insert(size, vec![ptr as *mut ()]);
    }
}