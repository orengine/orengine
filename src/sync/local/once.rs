#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum OnceState {
    NotCalled = 0,
    Called = 1
}

pub struct LocalOnce {
    state: OnceState
}

impl LocalOnce {
    pub const fn new() -> LocalOnce {
        LocalOnce {
            state: OnceState::NotCalled
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
    pub fn call_once_force<F: FnOnce(&mut OnceState)>(&mut self, f: F) -> Result<(), ()> {
        if self.state == OnceState::NotCalled {
            self.state = OnceState::Called;
            f(&mut self.state);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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