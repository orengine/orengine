/// Configuration for `fallback worker`.
///
/// # Fields
///
/// - `number_of_threads_per_executor`: number of threads in the pool per executor.
///    Must be greater than 0.
///    If feature `fallback_thread_pool` is disabled, this field is ignored.
#[derive(Clone, Copy)]
pub struct FallbackConfig {
    /// Number of threads in the pool per executor. Must be greater than 0.
    /// If feature `fallback_thread_pool` is disabled, this field is ignored.
    pub number_of_threads_per_executor: u16,
}

impl FallbackConfig {
    /// Creates new `FallbackConfig` with default values.
    pub const fn default() -> Self {
        Self {
            number_of_threads_per_executor: 4,
        }
    }

    /// Checks if [`FallbackConfig`] is valid.
    ///
    /// # Errors
    ///
    /// - [`FallbackConfig.number_of_entries`](#field.number_of_entries) must be greater than 0.
    pub const fn validate(self) -> Result<(), &'static str> {
        if self.number_of_threads_per_executor > 0 {
            Ok(())
        } else {
            Err("fallback: number_of_threads_per_executor must be greater than 0")
        }
    }
}
