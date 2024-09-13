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
        $vis struct $guard_name {
            value: std::mem::ManuallyDrop<$value_type>
        }

        impl $guard_name {
            #[inline(always)]
            $vis fn into_inner(self) -> $value_type {
                std::mem::ManuallyDrop::into_inner(self.value)
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
    use std::ops::Deref;
    new_local_pool! {
        pub(self),
        LOCAL_TEST_POOL,
        TestPool,
        usize,
        TestGuard,
        { 0 }
    }

    #[orengine_macros::test]
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
