#![allow(dead_code)]
// TODO docs

use std::fmt::Debug;
use crate::utils::Ptr;

struct Inner<T> {
    counter: usize,
    data: T
}

pub struct Local<T> {
    inner: Ptr<Inner<T>>
}

impl<T> Local<T> {
    pub fn new(data: T) -> Self {
        Local {
            inner: Ptr::new(Inner { data, counter: 1 })
        }
    }
    
    #[inline(always)]
    fn inc_counter(&self) {
        unsafe { self.inner.as_mut().counter += 1; }
    }
    
    #[inline(always)]
    fn dec_counter(&self) -> usize {
        let reference = unsafe { self.inner.as_mut() };
        reference.counter -= 1;
        reference.counter
    }
    
    #[inline(always)]
    pub fn get<'local>(&self) -> &'local T {
        unsafe { &self.inner.as_ref().data }
    }
    
    #[inline(always)]
    pub fn get_mut<'local>(&self) -> &'local mut T {
        unsafe { &mut self.inner.as_mut().data }
    }
}

impl<T: Default> Default for Local<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Debug> Debug for Local<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}

impl<T> Clone for Local<T> {
    fn clone(&self) -> Self {
        self.inc_counter();
        Self {
            inner: self.inner
        }
    }
}

impl<T> Drop for Local<T> {
    fn drop(&mut self) {
        if self.dec_counter() == 0 {
            unsafe {
                self.inner.drop_and_deallocate();
            }
        }
    }
}

impl<T> !Send for Local<T> {}
unsafe impl<T> Sync for Local<T> {}