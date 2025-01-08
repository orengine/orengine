/// In systems with 64-bit pointers, the high bit is reserved for the task-locality flag
/// caused by the fact that `*mut dyn` can be safely cast to `i128`.
///
/// `IS_LOCAL_SHIFT` is the shift amount for the task-locality flag.
#[cfg(target_pointer_width = "64")]
pub(crate) const IS_LOCAL_SHIFT: i128 = 127;
/// In systems with 64-bit pointers, the high bit is reserved for the task-locality flag
/// caused by the fact that `*mut dyn` can be safely cast to `i128`.
///
/// `TASK_MASK` is the mask for the task associated with the current tagged ptr.
#[cfg(target_pointer_width = "64")]
pub(crate) const TASK_MASK: i128 = !(1 << IS_LOCAL_SHIFT);

/// In systems with 64-bit pointers, the high bit is reserved for the task-locality flag
/// caused by the fact that `*mut dyn` can be safely cast to `i128`.
///
/// `IS_LOCAL_MASK` is the mask for the task-locality flag associated with the current tagged ptr.
#[cfg(target_pointer_width = "64")]
pub(crate) const IS_LOCAL_MASK: i128 = 1 << IS_LOCAL_SHIFT;

/// `Locality` is a struct that represents whether a task is [`local`](Locality::local)
/// or [`shared`](Locality::shared).
///
/// # The difference between shared and local tasks
///
/// Read it in [`crate::Executor`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Locality {
    #[cfg(not(target_pointer_width = "64"))]
    pub(crate) value: bool,
    #[cfg(target_pointer_width = "64")]
    pub(crate) value: i128,
}

impl Locality {
    /// Creates a new `Locality` with a `local` value.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`crate::Executor`].
    #[inline(always)]
    pub fn local() -> Self {
        #[cfg(not(target_pointer_width = "64"))]
        {
            Self { value: true }
        }

        #[cfg(target_pointer_width = "64")]
        Self {
            value: IS_LOCAL_MASK,
        }
    }

    /// Creates a new `Locality` with a `shared` value.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`crate::Executor`].
    #[inline(always)]
    pub fn shared() -> Self {
        #[cfg(not(target_pointer_width = "64"))]
        {
            Self { value: false }
        }

        #[cfg(target_pointer_width = "64")]
        Self { value: 0 }
    }

    /// Returns whether the `Locality` is `local`.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`crate::Executor`].
    #[inline(always)]
    pub fn is_local(&self) -> bool {
        #[cfg(not(target_pointer_width = "64"))]
        {
            self.value
        }

        #[cfg(target_pointer_width = "64")]
        {
            self.value != 0
        }
    }
}
