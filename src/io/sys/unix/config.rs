/// Configuration for `io-uring worker`.
///
/// # Fields
///
/// - `number_of_entries`: number of entries in `io_uring`. Must be greater than 0.
///   Every entry is 64 bytes, but `io-uring worker` can't process more requests at a one time
///   than the number of entries.
#[derive(Clone, Copy)]
pub struct IOUringConfig {
    /// Number of entries in `io_uring`. Must be greater than 0. Every entry is 64 bytes, but
    /// `io-uring worker` can't process more requests at a one time than the number of entries.
    pub number_of_entries: u32,
}

impl IOUringConfig {
    /// Creates new `IOUringConfig` with default values.
    pub const fn default() -> Self {
        Self {
            number_of_entries: 256,
        }
    }

    /// Checks if [`IOUringConfig`] is valid.
    ///
    /// # Errors
    ///
    /// - [`IOUringConfig.number_of_entries`](#field.number_of_entries) must be greater than 0.
    pub const fn validate(self) -> Result<(), &'static str> {
        if self.number_of_entries > 0 {
            Ok(())
        } else {
            Err("io_uring: number_of_entries must be greater than 0")
        }
    }
}
