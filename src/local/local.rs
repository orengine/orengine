use crate::utils::Ptr;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

/// A private structure used within Local to hold a reference counter
/// and the actual data.
///
/// It ensures that the reference counting mechanism is tightly coupled with the data being stored.
struct Inner<T> {
    /// A simple reference counter, incremented when new references are created and decremented
    /// when references are dropped.
    counter: usize,
    /// The user-defined data that the [`Local`] struct manages.
    data: T,
}

/// [`Local`] is a custom smart pointer-like structure designed to manage `T` in a single-threaded,
/// non-shared memory environment.
/// This design is aligned with the shared-nothing concurrency model, which avoids shared memory
/// between threads to reduce the complexity of synchronization.
///
/// In this model, each thread has its own isolated memory,
/// ensuring that data is not concurrently accessed by multiple threads.
/// This provides simpler concurrency and eliminates
/// the need for complex locking or atomic operations.
///
/// [`Local`] is not [`Send`] by design, meaning it cannot be transferred across threads,
/// and it should only be used in single-threaded (local) contexts.
///
/// This makes [`Local`] ideal for managing thread-local data,
/// where the overhead of atomic reference counting or synchronization is unnecessary.
/// Instead, you get a simpler and more efficient implementation by using a raw pointer
/// with manual reference counting.
///
/// # Shared-Nothing and spawn_local
///
/// In Rust, when working with local, task-specific data, you may want to leverage
/// shared-nothing concurrency by isolating data to specific threads.
/// Using the [`Local`] struct with [`spawn_local`](crate::runtime::Executor::spawn_local)
/// (which ensures that a task runs in a local thread) allows you to maintain this isolation.
/// Since Local cannot be transferred to another thread, it enforces the shared-nothing principle,
/// where each every thread holds and manages its own data independently.
///
/// # Example
///
/// ```no_run
/// use orengine::local::Local;
/// use orengine::Executor;
/// use orengine::local_executor;
///
/// fn main() {
///     Executor::init().run_with_local_future(async {
///         let local_data = Local::new(42);
///
///         println!("The value is: {}", local_data);
///
///         *local_data.get_mut() += 1;
///
///         println!("Updated value: {}", local_data);
///
///         // Cloning (increments the internal counter)
///         let cloned_data = local_data.clone();
///         local_executor().spawn_local(async move {
///             assert_eq!(*cloned_data, 43);
///         });
///     });
/// }
/// ```
pub struct Local<T> {
    inner: Ptr<Inner<T>>,
    #[cfg(debug_assertions)]
    parent_executor_id: usize,
}

macro_rules! check_parent_executor_id {
    ($local:expr) => {
        #[cfg(debug_assertions)]
        {
            if $local.parent_executor_id != crate::local_executor().id() {
                panic!("{}", crate::BUG_MESSAGE);
            }
        }
    };
}

impl<T> Local<T> {
    /// Creates a new [`Local`] instance, initializing it with the provided data.
    ///
    /// # Panics
    ///
    /// If the [`Executor`](crate::Executor) is not initialized.
    ///
    /// Read [`MSG_LOCAL_EXECUTOR_IS_NOT_INIT`](crate::runtime::executor::MSG_LOCAL_EXECUTOR_IS_NOT_INIT)
    /// for more details.
    pub fn new(data: T) -> Self {
        Local {
            inner: Ptr::new(Inner { data, counter: 1 }),
            #[cfg(debug_assertions)]
            parent_executor_id: crate::local_executor().id(),
        }
    }

    /// Increments the internal reference counter.
    /// This is called when a new reference to the [`Local`] is created
    /// (e.g., when `clone()` is called).
    #[inline(always)]
    fn inc_counter(&self) {
        check_parent_executor_id!(self);

        unsafe {
            self.inner.as_mut().counter += 1;
        }
    }

    /// Decrements the internal reference counter. This is called when a reference is dropped
    /// (e.g., when the [`Local`] is dropped).
    /// If the reference count reaches zero, the data is cleaned up.
    #[inline(always)]
    fn dec_counter(&self) -> usize {
        check_parent_executor_id!(self);

        let reference = unsafe { self.inner.as_mut() };
        reference.counter -= 1;
        reference.counter
    }

    /// Returns a mutable reference to the inner data.
    #[inline(always)]
    pub fn get_mut(&self) -> &mut T {
        check_parent_executor_id!(self);

        unsafe { &mut self.inner.as_mut().data }
    }
}

impl<T: Default> Default for Local<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Debug> Debug for Local<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        check_parent_executor_id!(self);

        self.deref().fmt(f)
    }
}

impl<T> Deref for Local<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        check_parent_executor_id!(self);

        unsafe { &self.inner.as_ref().data }
    }
}

impl<T> DerefMut for Local<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

impl<T: Display> Display for Local<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        check_parent_executor_id!(self);

        self.deref().fmt(f)
    }
}

impl<T> Clone for Local<T> {
    fn clone(&self) -> Self {
        self.inc_counter();
        Self {
            inner: self.inner,
            #[cfg(debug_assertions)]
            parent_executor_id: self.parent_executor_id,
        }
    }
}

impl<T> Drop for Local<T> {
    fn drop(&mut self) {
        check_parent_executor_id!(self);

        if self.dec_counter() == 0 {
            unsafe {
                self.inner.drop_and_deallocate();
            }
        }
    }
}

impl<T> !Send for Local<T> {}
unsafe impl<T> Sync for Local<T> {}
