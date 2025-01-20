/// A utility to manage progressively increasing timeouts.
///
/// # Type Parameters
/// - `START_DUR_MICROS`: The initial timeout duration in microseconds.
/// - `END_DUR_MICROS`: The maximum timeout duration in microseconds.
pub(crate) struct ProgressiveTimeout<const START_DUR_MICROS: u64, const END_DUR_MICROS: u64> {
    current_duration_ms: u64,
}

impl<const START_DUR_MICROS: u64, const END_DUR_MICROS: u64>
    ProgressiveTimeout<START_DUR_MICROS, END_DUR_MICROS>
{
    /// Creates a new `ProgressiveTimeout`.
    ///
    /// Initializes `current_duration_ms` to half of
    /// `START_DUR_MICROS` (minimum value of 1 microsecond).
    pub(crate) fn new() -> Self {
        Self {
            current_duration_ms: (START_DUR_MICROS >> 1).max(1),
        }
    }

    /// Returns the current timeout duration and doubles it.
    ///
    /// Increases `current_duration_ms` by doubling its value, up to a maximum of `END_DUR_MICROS`.
    pub(crate) fn timeout(&mut self) -> std::time::Duration {
        if self.current_duration_ms < END_DUR_MICROS {
            self.current_duration_ms <<= 1;
        }

        std::time::Duration::from_micros(self.current_duration_ms)
    }

    /// Returns the current timeout duration with an additional shift and doubles it.
    ///
    /// Increases `current_duration_ms` by doubling its value, up to a maximum of `END_DUR_MICROS`,
    /// and applies a left bit shift by the given `shift` value to the resulting duration.
    ///
    /// # Parameters
    ///
    /// - `shift`: The number of bits to shift the duration left.
    pub(crate) fn timeout_with_shift(&mut self, shift: u64) -> std::time::Duration {
        if self.current_duration_ms < END_DUR_MICROS {
            self.current_duration_ms <<= 1;
        }

        std::time::Duration::from_micros(self.current_duration_ms << shift)
    }

    /// Resets the timeout duration.
    ///
    /// Resets `current_duration_ms` to half of `START_DUR_MICROS` (minimum value of 1 microsecond).
    pub(crate) fn reset(&mut self) {
        self.current_duration_ms = (START_DUR_MICROS >> 1).max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progressive_timeout() {
        let mut progressive_timeout = ProgressiveTimeout::<100, 1600>::new();

        assert_eq!(
            progressive_timeout.timeout(),
            std::time::Duration::from_micros(100)
        );
        assert_eq!(
            progressive_timeout.timeout(),
            std::time::Duration::from_micros(200)
        );
        assert_eq!(
            progressive_timeout.timeout(),
            std::time::Duration::from_micros(400)
        );
        assert_eq!(
            progressive_timeout.timeout(),
            std::time::Duration::from_micros(800)
        );
        assert_eq!(
            progressive_timeout.timeout(),
            std::time::Duration::from_micros(1600)
        );
        assert_eq!(
            progressive_timeout.timeout(),
            std::time::Duration::from_micros(1600)
        );

        progressive_timeout.reset();

        assert_eq!(
            progressive_timeout.timeout(),
            std::time::Duration::from_micros(100)
        );
        assert_eq!(
            progressive_timeout.timeout_with_shift(2),
            std::time::Duration::from_micros(800)
        );
        assert_eq!(
            progressive_timeout.timeout_with_shift(1),
            std::time::Duration::from_micros(800)
        );
        assert_eq!(
            progressive_timeout.timeout(),
            std::time::Duration::from_micros(800)
        );
    }
}
