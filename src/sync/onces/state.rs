/// `OnceState` is used to indicate whether the `Once` has been called or not.
///
/// # Variants
///
/// * `NotCalled` - The `Once` has not been called.
/// * `Called` - The `Once` has been called.
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum OnceState {
    NotCalled = 0,
    Called = 1,
}

impl OnceState {
    /// Returns the `OnceState` as an `isize`.
    pub const fn not_called() -> isize {
        0
    }

    /// Returns the `OnceState` as an `isize`.
    pub const fn called() -> isize {
        1
    }
}

impl Into<isize> for OnceState {
    fn into(self) -> isize {
        self as isize
    }
}

impl From<isize> for OnceState {
    fn from(state: isize) -> Self {
        match state {
            0 => OnceState::NotCalled,
            1 => OnceState::Called,
            _ => panic!("Invalid once state. It can be 0 (NotCalled) or 1 (Called)"),
        }
    }
}