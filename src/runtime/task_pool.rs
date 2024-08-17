use std::future::Future;
use std::mem;
use std::mem::MaybeUninit;
use ahash::AHashMap;


pub(crate) struct TaskPool {
    storage: AHashMap<usize, Vec<*mut ()>> // key is a size
}

#[thread_local]
pub(crate) static mut TASK_POOL: MaybeUninit<TaskPool> = MaybeUninit::uninit();

#[inline(always)]
pub(crate) fn task_pool() -> &'static mut TaskPool {
    unsafe {TASK_POOL.assume_init_mut()}
}

impl TaskPool {
    pub(crate) fn init() {
        unsafe {
            TASK_POOL = MaybeUninit::new(TaskPool {
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
        let pool = unsafe { self.storage.get_mut(&size).unwrap_unchecked() };
        pool.push(ptr as *mut ());
    }
}