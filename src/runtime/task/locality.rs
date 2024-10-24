/// In systems with 64-bit pointers, the high bit is reserved for the task-locality flag
/// caused by the fact that `*mut dyn` can be safely cast to `i128`.
///
/// `IS_LOCAL_SHIFT` is the shift amount for the task-locality flag.
#[cfg(any(target_pointer_width = "64"))]
pub(crate) const IS_LOCAL_SHIFT: i128 = 127;
/// In systems with 64-bit pointers, the high bit is reserved for the task-locality flag
/// caused by the fact that `*mut dyn` can be safely cast to `i128`.
///
/// `TASK_MASK` is the mask for the task associated with the current tagged ptr.
#[cfg(any(target_pointer_width = "64"))]
pub(crate) const TASK_MASK: i128 = !(1 << IS_LOCAL_SHIFT);

/// In systems with 64-bit pointers, the high bit is reserved for the task-locality flag
/// caused by the fact that `*mut dyn` can be safely cast to `i128`.
///
/// `IS_LOCAL_MASK` is the mask for the task-locality flag associated with the current tagged ptr.
#[cfg(any(target_pointer_width = "64"))]
pub(crate) const IS_LOCAL_MASK: i128 = 1 << IS_LOCAL_SHIFT;

/// `Locality` is a struct that represents whether a task is [`local`](Locality::local)
/// or [`global`](Locality::global).
///
/// # The difference between global and local tasks
///
/// Read it in [`crate::Executor`].
#[derive(Clone, Copy)]
pub struct Locality {
    #[cfg(not(target_pointer_width = "64"))]
    pub(crate) value: bool,
    #[cfg(any(target_pointer_width = "64"))]
    pub(crate) value: i128,
}

impl Locality {
    /// Creates a new `Locality` with a `local` value.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`crate::Executor`].
    #[inline(always)]
    pub fn local() -> Self {
        #[cfg(not(target_pointer_width = "64"))]
        {
            Self { value: true }
        }

        #[cfg(any(target_pointer_width = "64"))]
        Self {
            value: IS_LOCAL_MASK,
        }
    }

    /// Creates a new `Locality` with a `global` value.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`crate::Executor`].
    #[inline(always)]
    pub fn global() -> Self {
        #[cfg(not(target_pointer_width = "64"))]
        {
            Self { value: false }
        }

        #[cfg(any(target_pointer_width = "64"))]
        Self { value: 0 }
    }

    /// Returns whether the `Locality` is `local`.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`crate::Executor`].
    #[inline(always)]
    pub fn is_local(&self) -> bool {
        #[cfg(not(target_pointer_width = "64"))]
        {
            self.value
        }

        #[cfg(any(target_pointer_width = "64"))]
        {
            self.value > 0
        }
    }
}
