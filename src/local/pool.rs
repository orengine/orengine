/// This macro creates a new thread local pool.
/// Because it is thread local it is lockless.
///
/// You can use this pool for reusing allocated in heap memory.
///
/// # Guard and releasing
///
/// Generated pool has a method `acquire()` that returns a guard for the pool.
/// It implements [`Deref`](core::ops::Deref) and [`DerefMut`](core::ops::DerefMut).
///
/// When the guard is dropped the value is released back to the pool.
///
/// If you want to get value from guard you can use the guard's method `into_inner`.
///
/// # Arguments
///
/// - vis: Visibility specifier for the generated structs and methods (e.g., pub, pub(self)).
///
/// - pool_thread_static_name: Name of the thread-local static pool.
///
/// - pool_struct_name: Name of the pool struct.
///
/// - value_type: Type of values stored in the pool.
///
/// - guard_name: Name of the guard struct that manages values acquired from the pool.
///
/// - new: A block that defines how to create new instances of the value type when the pool is empty.
///
/// # Example
///
/// ```no_run
/// use std::ops::Deref;
/// use orengine::new_local_pool;
///
/// new_local_pool! {
///     pub,
///     MY_THREAD_LOCAL_POOL,
///     MyPool,
///     String,
///     MyGuard,
///     { String::with_capacity(1024) }
/// }
///
/// fn main() {
///     let mut guard = MyPool::acquire();
///
///     assert_eq!(guard.capacity(), 1024);
///     // do some work
///
///     guard.clear(); // clear the value after use
///     // guard is dropped and released back to the pool
/// }
/// ```
#[macro_export]
macro_rules! new_local_pool {
    (
        $vis: vis,
        $pool_thread_static_name: ident,
        $pool_struct_name: ident,
        $value_type: ty,
        $guard_name: ident,
        $new: block
    ) => {
        /// The guard struct is responsible for managing the lifecycle of an object
        /// acquired from the pool.
        ///
        /// It implements the [`Deref`](core::ops::Deref) and [`DerefMut`](core::ops::DerefMut)
        /// traits, providing access to the underlying object, and automatically
        /// releases the object back to the pool when it is dropped.
        ///
        /// You can get the value from the guard using the `into_inner` method.
        ///
        /// # Example
        ///
        /// ```no_run
        /// use std::ops::Deref;
        /// use orengine::new_local_pool;
        ///
        /// new_local_pool! {
        ///     pub,
        ///     MY_THREAD_LOCAL_POOL,
        ///     MyPool,
        ///     String,
        ///     MyGuard,
        ///     { String::with_capacity(1024) }
        /// }
        ///
        /// fn main() {
        ///     let mut guard = MyPool::acquire();
        ///
        ///     assert_eq!(*guard.deref().capacity(), 1024);
        ///     guard.push_str("Hello, world!");
        ///     assert_eq!(*guard, "Hello, world!");
        ///
        ///     guard.clear(); // clear the value after use
        ///     // guard is dropped and released back to the pool
        /// }
        /// ```
        $vis struct $guard_name {
            value: std::mem::ManuallyDrop<$value_type>
        }

        impl $guard_name {
            /// Returns the value from the guard, consuming the guard.
            #[allow(dead_code)]
            #[inline(always)]
            $vis fn into_inner(mut self) -> $value_type {
                let value = unsafe { std::mem::ManuallyDrop::take(&mut self.value) };
                std::mem::forget(self);
                value
            }
        }

        impl core::ops::Deref for $guard_name {
            type Target = $value_type;

            fn deref(&self) -> &$value_type {
                &self.value
            }
        }

        impl core::ops::DerefMut for $guard_name {
            fn deref_mut(&mut self) -> &mut $value_type {
                &mut self.value
            }
        }

        impl Drop for $guard_name {
            fn drop(&mut self) {
                $pool_thread_static_name.with(|pool_cell| {
                    let pool = unsafe { &mut *pool_cell.get() };
                    let value = unsafe { std::mem::ManuallyDrop::take(&mut self.value) };
                    pool.storage.push(value);
                });
            }
        }

        /// This is a thread local pool.
        /// Because it is thread local it is lockless.
        ///
        /// You can use this pool for reusing allocated in heap memory.
        ///
        /// # Guard and releasing
        ///
        /// The pool has a method `acquire()` that returns a guard for the pool.
        /// It implements [`Deref`](core::ops::Deref) and [`DerefMut`](core::ops::DerefMut).
        ///
        /// When the guard is dropped the value is released back to the pool.
        ///
        /// If you want to get value from guard you can use the guard's method `into_inner`.
        ///
        /// # Example
        ///
        /// ```no_run
        /// use std::ops::Deref;
        /// use orengine::new_local_pool;
        ///
        /// new_local_pool! {
        ///     pub,
        ///     MY_THREAD_LOCAL_POOL,
        ///     MyPool,
        ///     String,
        ///     MyGuard,
        ///     { String::with_capacity(1024) }
        /// }
        ///
        /// fn main() {
        ///     let mut guard = MyPool::acquire();
        ///
        ///     assert_eq!(guard.capacity(), 1024);
        ///     // do some work
        ///
        ///     guard.clear(); // clear the value after use
        ///     // guard is dropped and released back to the pool
        /// }
        /// ```
        $vis struct $pool_struct_name {
            storage: Vec<$value_type>
        }

        thread_local! {
            static $pool_thread_static_name: std::cell::UnsafeCell<$pool_struct_name> = std::cell::UnsafeCell::new(
                $pool_struct_name {
                    storage: Vec::new()
                }
            );
        }

        impl $pool_struct_name {
            /// Acquires a guard for the pool.
            ///
            /// # Guard
            ///
            /// The guard implements [`Deref`](core::ops::Deref)
            /// and [`DerefMut`](core::ops::DerefMut).
            ///
            /// When the guard is dropped the value is released back to the pool.
            ///
            /// If you want to get value from guard you can use the guard's method `into_inner`.
            #[allow(dead_code)]
            #[inline(always)]
            $vis fn acquire() -> $guard_name {
                $pool_thread_static_name.with(|pool_cell| -> $guard_name {
                    let pool = unsafe { &mut *pool_cell.get() };

                    if let Some(value) = pool.storage.pop() {
                        $guard_name {
                            value: std::mem::ManuallyDrop::new(value)
                        }
                    } else {
                        $guard_name {
                            value: std::mem::ManuallyDrop::new($new)
                        }
                    }
                })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate as orengine;
    use std::ops::Deref;

    new_local_pool! {
        pub(self),
        LOCAL_TEST_POOL,
        TestPool,
        usize,
        TestGuard,
        { 0 }
    }

    #[orengine_macros::test_local]
    fn test_new_local_pool() {
        let mut guard = TestPool::acquire();
        assert_eq!(*guard.deref(), 0);

        *guard = 1;
        assert_eq!(*guard.deref(), 1);
        drop(guard);

        let guard = TestPool::acquire();
        assert_eq!(*guard.deref(), 1);

        let guard2 = TestPool::acquire();
        assert_eq!(guard2.into_inner(), 0);
    }
}
