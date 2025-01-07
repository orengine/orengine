use crate::utils::Ptr;
use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

/// `BorrowingState` used to represent the borrowing state of a resource in with `debug_assertions`.
///
/// This enum is enabled only when debug assertions are active (i.e., in non-release builds)
/// due to the use of the `#[cfg(debug_assertions)]` attribute.
///
/// # Variants
///
/// - [`None`](BorrowingState::None): Represents the state where no borrowing is
///   currently taking place. The resource is free to be borrowed in any mode.
///
/// - [`Shared(usize)`](BorrowingState::Shared): Represents the state where the resource is being
///   borrowed in a shared manner. The usize field tracks the count of shared borrows
///   currently in effect.
///
/// - [`Exclusive`](`BorrowingState::Exclusive`): Represents the state where the resource is being
///   borrowed exclusively. No other borrows (shared or exclusive) are allowed while in this state.
///
/// # Purpose
///
/// [`BorrowingState`] is intended for use with `debug_assertions` to aid in verifying or enforcing
/// borrowing rules at runtime.
/// It can help identify invalid access patterns or ensure proper borrowing
/// behavior during development.
#[cfg(debug_assertions)]
enum BorrowingState {
    /// Represents the state where no borrowing is currently taking place. The resource is free
    /// to be borrowed in any mode.
    None,
    /// Represents the state where the resource is being borrowed in a shared manner.
    /// The `usize` field tracks the count of shared borrows currently in effect.
    Shared(usize),
    /// Represents the state where the resource is being borrowed exclusively.
    /// No other borrows (shared or exclusive) are allowed while in this state.
    Exclusive,
}

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
    #[cfg(debug_assertions)]
    borrowing_state: BorrowingState,
    no_send_marker: std::marker::PhantomData<*const ()>,
}

/// A reference to a value stored in a [`Local`] struct.
///
/// With `debug_assertions` this struct provides additional checks to ensure that the user does not
/// create shared and exclusive references at the same time.
///
/// Else, it is simply a reference to the value stored in the [`Local`] struct.
pub struct LocalRef<'borrow, T> {
    #[cfg(debug_assertions)]
    inner: Ptr<Inner<T>>,
    #[cfg(debug_assertions)]
    borrow_pd: std::marker::PhantomData<&'borrow ()>,
    #[cfg(not(debug_assertions))]
    shared_reference: &'borrow T,
    // !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'borrow, T> LocalRef<'borrow, T> {
    /// Creates a new [`LocalRef`] from a reference to a [`Local`] struct with `'borrow` lifetime.
    fn new(local: &'borrow Local<T>) -> Self {
        Self {
            #[cfg(debug_assertions)]
            inner: local.inner,
            #[cfg(debug_assertions)]
            borrow_pd: std::marker::PhantomData,
            #[cfg(not(debug_assertions))]
            shared_reference: unsafe { &local.inner.as_ref().data },
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<T> Deref for LocalRef<'_, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        #[cfg(debug_assertions)]
        unsafe {
            &self.inner.as_ref().data
        }

        #[cfg(not(debug_assertions))]
        self.shared_reference
    }
}

impl<T: Display> Display for LocalRef<'_, T> {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(f)
    }
}

impl<T> Drop for LocalRef<'_, T> {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let inner = unsafe { self.inner.as_mut() };
            if let BorrowingState::Shared(n) = inner.borrowing_state {
                if n == 1 {
                    inner.borrowing_state = BorrowingState::None;
                } else {
                    inner.borrowing_state = BorrowingState::Shared(n - 1);
                }
            } else {
                panic!("{}", crate::BUG_MESSAGE)
            }
        }
    }
}

/// A mutable reference to a value stored in a [`Local`] struct.
///
/// With `debug_assertions` this struct provides additional checks to ensure that the user does not
/// create shared and exclusive references or multiple mutable references at the same time.
///
/// In `release` mode it is simply a mutable reference to the value stored in the [`Local`] struct.
pub struct LocalRefMut<'borrow, T> {
    #[cfg(debug_assertions)]
    inner: Ptr<Inner<T>>,
    #[cfg(debug_assertions)]
    borrow_pd: std::marker::PhantomData<&'borrow ()>,
    #[cfg(not(debug_assertions))]
    exclusive_reference: &'borrow mut T,
    // !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'borrow, T> LocalRefMut<'borrow, T> {
    /// Creates a new [`LocalRefMut`] from a reference to a [`Local`] struct
    /// with `'borrow` lifetime.
    fn new(local: &'borrow Local<T>) -> Self {
        Self {
            #[cfg(debug_assertions)]
            inner: local.inner,
            #[cfg(debug_assertions)]
            borrow_pd: std::marker::PhantomData,
            #[cfg(not(debug_assertions))]
            exclusive_reference: unsafe { &mut local.inner.as_mut().data },
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<T> Deref for LocalRefMut<'_, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        #[cfg(debug_assertions)]
        unsafe {
            &self.inner.as_ref().data
        }

        #[cfg(not(debug_assertions))]
        self.exclusive_reference
    }
}

impl<T> DerefMut for LocalRefMut<'_, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        #[cfg(debug_assertions)]
        unsafe {
            &mut self.inner.as_mut().data
        }

        #[cfg(not(debug_assertions))]
        self.exclusive_reference
    }
}

impl<T: Display> Display for LocalRefMut<'_, T> {
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(f)
    }
}

impl<T> Drop for LocalRefMut<'_, T> {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let inner = unsafe { self.inner.as_mut() };
            if matches!(inner.borrowing_state, BorrowingState::Exclusive) {
                inner.borrowing_state = BorrowingState::None;
            } else {
                panic!("{}", crate::BUG_MESSAGE)
            }
        }
    }
}

/// [`Local`] is a custom smart pointer-like structure designed to manage `T` in a single-threaded,
/// non-concurrent context.
///
/// This design is aligned with the shared-nothing concurrency model, which avoids shared memory
/// between threads to reduce the complexity of synchronization.
///
/// # What does concurrent context mean in single-threaded execution?
///
/// ## Invalid code (with single-threaded concurrency):
///
/// ```rust
/// use orengine::Local;
///
/// # struct Data {}
/// # async fn dump_data(counter: &Data) {}
/// # fn is_valid_data(counter: &Data) -> bool { true }
/// # async fn send_data(counter: &Data) {}
///
/// async fn write_and_dump_data_and_return_was_sent(counter: Local<Data>) -> bool {
///     let mut local_ref = counter.borrow_mut();
///     if is_valid_data(&*local_ref) {
///         dump_data(&*local_ref).await; // Here the executor can execute other task, that change the data to invalid
///         send_data(&*local_ref).await;
///
///         return true;
///     }
///
///     false
/// }
/// ```
///
/// # Safety
///
/// - `Local` is used in one thread;
/// - Before every `await` operation, [`LocalRef`] and [`LocalRefMut`] need to be dropped, if
///   other task can mutate the value and.
///
/// It is checked with `debug_assertions`. If you cannot guarantee compliance with the above rules
/// you should use [`LocalMutex`](crate::sync::LocalMutex) or
/// [`LocalRWLock`](crate::sync::LocalRWLock) instead. Local synchronization-primitives have
/// almost the same performance as `Local` or `Rc<RefCell>` and can be used in single-threaded
/// concurrency context.  
///
/// # Shared-Nothing and [`spawn_local`](crate::Executor::spawn_local)
///
/// In Rust, when working with local, task-specific data, you may want to leverage
/// shared-nothing concurrency by isolating data to specific threads.
/// Using the `Local` struct with [`spawn_local`](crate::runtime::Executor::spawn_local)
/// (which ensures that a task runs in a local thread) allows you to maintain this isolation.
/// Since `Local` cannot be transferred to another thread, it enforces the
/// shared-nothing principle, where each every thread holds and manages its own data independently.
///
/// # Example
///
/// ```no_run
/// use orengine::local::Local;
/// use orengine::Executor;
/// use orengine::local_executor;
///
/// Executor::init().run_with_local_future(async {
///     let local_data = Local::new(42);
///
///     println!("The value is: {}", local_data);
///
///     *local_data.borrow_mut() += 1;
///
///     println!("Updated value: {}", local_data);
///
///     // Cloning (increments the internal counter)
///     let cloned_data = local_data.clone();
///     local_executor().spawn_local(async move {
///         assert_eq!(*cloned_data.borrow(), 43);
///     });
/// });
/// ```
pub struct Local<T> {
    inner: Ptr<Inner<T>>,
    #[cfg(debug_assertions)]
    parent_executor_id: usize,
    // !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

/// With `debug_assertions` checks whether the parent executor ID matches the executor ID of the current thread.
macro_rules! debug_check_parent_executor_id {
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
        Self {
            inner: Ptr::new(Inner {
                data,
                counter: 1,
                #[cfg(debug_assertions)]
                borrowing_state: BorrowingState::None,
                no_send_marker: std::marker::PhantomData,
            }),
            #[cfg(debug_assertions)]
            parent_executor_id: crate::local_executor().id(),
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Increments the internal reference counter.
    /// This is called when a new reference to the [`Local`] is created
    /// (e.g., when `clone()` is called).
    #[inline(always)]
    fn inc_counter(&self) {
        debug_check_parent_executor_id!(self);

        unsafe {
            self.inner.as_mut().counter += 1;
        }
    }

    /// Decrements the internal reference counter. This is called when a reference is dropped
    /// (e.g., when the [`Local`] is dropped).
    /// If the reference count reaches zero, the data is cleaned up.
    #[inline(always)]
    fn dec_counter(&self) -> usize {
        debug_check_parent_executor_id!(self);

        let reference = unsafe { self.inner.as_mut() };
        reference.counter -= 1;
        reference.counter
    }

    #[inline(always)]
    /// Returns [`LocalRef`] that allows shared access to the data.
    ///
    /// Read more in [`LocalRef`].
    ///
    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    pub fn borrow(&self) -> LocalRef<T> {
        debug_check_parent_executor_id!(self);

        #[cfg(debug_assertions)]
        unsafe {
            match self.inner.as_ref().borrowing_state {
                BorrowingState::None => {
                    self.inner.as_mut().borrowing_state = BorrowingState::Shared(1);
                }
                BorrowingState::Shared(n) => {
                    self.inner.as_mut().borrowing_state = BorrowingState::Shared(n + 1);
                }
                BorrowingState::Exclusive => panic!(
                    "Local is already borrowed as mutably, use LocalMutex instead. It is almost as \
                    fast as RefCell, and it is safe to use in concurrent single-threaded contexts."
                ),
            }
        }

        LocalRef::new(self)
    }

    #[inline(always)]
    /// Returns [`LocalRefMut`] that allows mutable access to the data.
    ///
    /// Read more in [`LocalRefMut`].
    ///
    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    pub fn borrow_mut(&self) -> LocalRefMut<T> {
        debug_check_parent_executor_id!(self);

        #[cfg(debug_assertions)]
        unsafe {
            match self.inner.as_ref().borrowing_state {
                BorrowingState::None => {
                    self.inner.as_mut().borrowing_state = BorrowingState::Exclusive;
                }
                BorrowingState::Shared(_) => panic!(
                    "Local is already borrowed as shared, use LocalMutex instead. It is almost as \
                    fast as RefCell, and it is safe to use in concurrent single-threaded contexts."
                ),
                BorrowingState::Exclusive => panic!(
                    "Local is already borrowed as mutably, use LocalMutex instead. It is almost as \
                    fast as RefCell, and it is safe to use in concurrent single-threaded contexts."
                ),
            }
        }

        LocalRefMut::new(self)
    }
}

impl<T: Default> Default for Local<T> {
    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Debug> Debug for Local<T> {
    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        debug_check_parent_executor_id!(self);

        self.borrow().fmt(f)
    }
}

impl<T: Display> Display for Local<T> {
    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        debug_check_parent_executor_id!(self);

        self.borrow().fmt(f)
    }
}

impl<T: PartialEq> PartialEq for Local<T> {
    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        debug_check_parent_executor_id!(self);

        *self.borrow() == *other.borrow()
    }
}

impl<T: Eq> Eq for Local<T> {}

impl<T: PartialOrd> PartialOrd for Local<T> {
    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in either `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        debug_check_parent_executor_id!(self);

        self.borrow().partial_cmp(&*other.borrow())
    }

    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in either `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn lt(&self, other: &Self) -> bool {
        *self.borrow() < *other.borrow()
    }

    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in either `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn le(&self, other: &Self) -> bool {
        *self.borrow() <= *other.borrow()
    }

    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in either `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn gt(&self, other: &Self) -> bool {
        *self.borrow() > *other.borrow()
    }

    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in either `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    #[inline(always)]
    fn ge(&self, other: &Self) -> bool {
        *self.borrow() >= *other.borrow()
    }
}

impl<T: Ord> Ord for Local<T> {
    /// # Panics
    ///
    /// Panics with `debug_assertions` if the value in either `Local` is currently mutably borrowed or if
    /// `Local` has been moved to another thread.
    fn cmp(&self, other: &Self) -> Ordering {
        debug_check_parent_executor_id!(self);

        self.borrow().cmp(&*other.borrow())
    }
}

impl<T> From<T> for Local<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> Clone for Local<T> {
    fn clone(&self) -> Self {
        self.inc_counter();

        Self {
            inner: self.inner,
            #[cfg(debug_assertions)]
            parent_executor_id: self.parent_executor_id,
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<T> Drop for Local<T> {
    fn drop(&mut self) {
        debug_check_parent_executor_id!(self);

        if self.dec_counter() == 0 {
            unsafe {
                self.inner.drop_and_deallocate();
            }
        }
    }
}

/// ```compile_fail
/// use orengine::Local;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// let value = Local::new(42);
/// check_send(value);
/// ```
///
/// ```compile_fail
/// use orengine::Local;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// let value = Local::new(42);
/// check_send(value.borrow());
/// ```
///
/// ```compile_fail
/// use orengine::Local;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// let value = Local::new(42);
/// check_send(value.borrow_mut());
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_local() {}

unsafe impl<T> Sync for Local<T> {}
