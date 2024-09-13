#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum OnceState {
    NotCalled = 0,
    Called = 1,
}

impl OnceState {
    pub const fn not_called() -> isize {
        0
    }

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

pub struct LocalOnce {
    state: OnceState,
}

impl LocalOnce {
    pub const fn new() -> LocalOnce {
        LocalOnce {
            state: OnceState::NotCalled,
        }
    }

    #[inline(always)]
    pub fn call_once<F: FnOnce()>(&mut self, f: F) -> Result<(), ()> {
        if self.state == OnceState::NotCalled {
            self.state = OnceState::Called;
            f();
            Ok(())
        } else {
            Err(())
        }
    }

    #[inline(always)]
    pub fn call_once_force<F: FnOnce(&OnceState)>(&mut self, f: F) -> Result<(), ()> {
        if self.state == OnceState::NotCalled {
            self.state = OnceState::Called;
            f(&self.state);
            Ok(())
        } else {
            Err(())
        }
    }

    #[inline(always)]
    pub fn state(&self) -> OnceState {
        self.state
    }

    #[inline(always)]
    pub fn is_called(&self) -> bool {
        self.state == OnceState::Called
    }
}

unsafe impl Sync for LocalOnce {}
impl !Send for LocalOnce {}

#[cfg(test)]
mod tests {
    use super::*;

    #[orengine_macros::test]
    fn test_local_once() {
        let mut once = LocalOnce::new();
        assert_eq!(once.state(), OnceState::NotCalled);
        assert!(!once.is_called());

        assert_eq!(once.call_once(|| ()), Ok(()));
        assert_eq!(once.state(), OnceState::Called);

        assert!(once.is_called());
        assert_eq!(once.call_once(|| ()), Err(()));
    }
}
