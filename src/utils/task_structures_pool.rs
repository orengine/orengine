macro_rules! create_control_cap_wrapper {
    ($name:ident, $acquire:expr, $guard_name:ident) => {
        pub struct $name {
            guard: $guard_name,
        }

        impl $name {
            fn new() -> Self {
                Self { guard: $acquire() }
            }
        }

        impl std::ops::Deref for $name {
            type Target = <$guard_name as std::ops::Deref>::Target;

            fn deref(&self) -> &Self::Target {
                &self.guard
            }
        }

        impl std::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.guard
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                if self.guard.len() < 128 {
                    // Drop guard to return the object to the pool.
                } else {
                    // Shrink the pool.
                    self.guard.shrink_to(2);
                }
            }
        }
    };
}

/// # Arguments
///
/// - vis: Visibility specifier for the generated structs and methods (e.g., pub, pub(self));
///
/// - `pool_thread_static_name`: Name of the thread-local static pool;
///
/// - `pool_struct_name`: Name of the pool struct;
///
/// - `value_type`: Type of values stored in the pool;
///
/// - `guard_name`: Name of the guard struct that manages values acquired from the pool;
///
/// - `new`: A block that defines how to create new instances of the value type when the pool is empty;
///
/// - `control_cap_wrapper_name`: Name of the control capacity wrapper struct;
///
/// - `get_control_cap_wrapper_fn_name`: Name of the function that returns the control capacity wrapper.
macro_rules! create_pool_of_objects_with_control_cap_wrapper {
    (
        $vis: vis,
        $pool_thread_static_name: ident,
        $pool_struct_name: ident,
        $value_type: ty,
        $guard_name: ident,
        $new: block,
        $control_cap_wrapper_name: ident,
        $get_control_cap_wrapper_fn_name: ident
    ) => {
        crate::new_local_pool! {
            $vis,
            $pool_thread_static_name,
            $pool_struct_name,
            $value_type,
            $guard_name,
            $new
        }

        create_control_cap_wrapper!($control_cap_wrapper_name, $pool_struct_name::acquire, $guard_name);

        /// Returns a control capacity wrapper for the pool.
        ///
        /// The wrapper shrinks the capacity of the pool to 2 if it is greater than 128.
        #[allow(clippy::needless_pub_self, reason = "Cannot write private code in macro else.")]
        $vis fn $get_control_cap_wrapper_fn_name() -> $control_cap_wrapper_name {
            $control_cap_wrapper_name::new()
        }
    };
}

create_pool_of_objects_with_control_cap_wrapper! {
    pub,
    TASK_VEC_LOCAL_POOL,
    TaskVecPool,
    Vec<crate::runtime::Task>,
    TaskVecPoolGuard,
    { Vec::with_capacity(2) },
    TaskVecFromPool,
    acquire_task_vec_from_pool
}

create_pool_of_objects_with_control_cap_wrapper! {
    pub,
    SYNC_TASK_LIST_LOCAL_POOL,
    SyncTaskListPool,
    crate::sync_task_queue::SyncTaskList,
    SyncTaskListPoolGuard,
    { crate::sync_task_queue::SyncTaskList::new() },
    SyncTaskListFromPool,
    acquire_sync_task_list_from_pool
}

pub(crate) use create_pool_of_objects_with_control_cap_wrapper;
