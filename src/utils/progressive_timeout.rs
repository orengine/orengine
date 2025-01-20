// TODO
pub(crate) struct ProgressiveTimeout<const START_DUR_MICROS: u64, const END_DUR_MICROS: u64> {
    current_duration_ms: u64,
}

impl<const START_DUR_MICROS: u64, const END_DUR_MICROS: u64>
    ProgressiveTimeout<START_DUR_MICROS, END_DUR_MICROS>
{
    pub(crate) fn new() -> Self {
        Self {
            current_duration_ms: (START_DUR_MICROS >> 1).max(1),
        }
    }

    pub(crate) fn timeout(&mut self) -> std::time::Duration {
        if self.current_duration_ms < END_DUR_MICROS {
            self.current_duration_ms <<= 1;
        }

        std::time::Duration::from_micros(self.current_duration_ms)
    }

    pub(crate) fn timeout_with_shift(&mut self, shift: u64) -> std::time::Duration {
        if self.current_duration_ms < END_DUR_MICROS {
            self.current_duration_ms <<= 1;
        }

        std::time::Duration::from_micros(self.current_duration_ms << shift)
    }

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
