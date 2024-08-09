use std::ops::{Deref, DerefMut};

use crate::local::Local;

struct Inner<T, Creator: Fn() -> T> {
    storage: Vec<T>,
    vacant: Vec<usize>,
    creator: Creator,
}

pub struct LocalPoolGuard<T, Creator: Fn() -> T> {
    index: usize,
    inner: Local<Inner<T, Creator>>,
}

impl<T, Creator: Fn() -> T> Deref for LocalPoolGuard<T, Creator> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.inner.get().storage.get_unchecked(self.index) }
    }
}

impl<T, Creator: Fn() -> T> DerefMut for LocalPoolGuard<T, Creator> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.inner.get_mut().storage.get_unchecked_mut(self.index) }
    }
}

impl<T, Creator: Fn() -> T> Drop for LocalPoolGuard<T, Creator> {
    fn drop(&mut self) {
        self.inner.get_mut().vacant.push(self.index)
    }
}

pub struct LocalPool<T, Creator: Fn() -> T> {
    inner: Local<Inner<T, Creator>>,
}

impl<T, Creator: Fn() -> T> LocalPool<T, Creator> {
    pub fn new(creator: Creator) -> Self {
        Self {
            inner: Local::new(Inner {
                storage: Vec::new(),
                vacant: Vec::new(),
                creator,
            }),
        }
    }

    #[inline(always)]
    pub fn acquire(&self) -> LocalPoolGuard<T, Creator> {
        let inner_ref = self.inner.get_mut();

        if let Some(index) = inner_ref.vacant.pop() {
            LocalPoolGuard {
                index,
                inner: self.inner.clone(),
            }
        } else {
            let index = inner_ref.storage.len();
            inner_ref.storage.push((inner_ref.creator)());
            LocalPoolGuard {
                index,
                inner: self.inner.clone(),
            }
        }
    }
}
