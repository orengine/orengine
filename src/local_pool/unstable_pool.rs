#[macro_export]
macro_rules! new_local_pool_thread_local {
    (
        $vis: vis,
        $pool_thread_static_name: ident,
        $pool_struct_name: ident,
        $value_type: ty,
        $guard_name: ident,
        $new: block
    ) => {
        $vis struct $guard_name {
            index: usize
        }

        impl core::ops::Deref for $guard_name {
            type Target = $value_type;

            fn deref(&self) -> &$value_type {
                unsafe {
                    $pool_thread_static_name.storage.get_unchecked(self.index)
                }
            }
        }

        impl core::ops::DerefMut for $guard_name {
            fn deref_mut(&mut self) -> &mut $value_type {
                unsafe {
                    $pool_thread_static_name.storage.get_unchecked_mut(self.index)
                }
            }
        }

        impl Drop for $guard_name {
            fn drop(&mut self) {
                unsafe {
                    $pool_thread_static_name.vacant.push(self.index);
                }
            }
        }

        $vis struct $pool_struct_name {
            storage: Vec<$value_type>,
            vacant: Vec<usize>
        }

        #[thread_local]
        $vis static mut $pool_thread_static_name: $pool_struct_name = $pool_struct_name {
            storage: Vec::new(),
            vacant: Vec::new()
        };

        impl $pool_struct_name {
            #[inline(always)]
            $vis fn acquire() -> $guard_name {
                unsafe {
                    if let Some(index) = $pool_thread_static_name.vacant.pop() {
                        $guard_name {
                            index
                        }
                    } else {
                        let index = $pool_thread_static_name.storage.len();
                        $pool_thread_static_name.storage.push($new);
                        $guard_name {
                            index
                        }
                    }
                }
            }
        }
    };
}
