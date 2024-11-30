use std::alloc::{alloc, dealloc, Layout};
use std::fmt::Debug;
use std::ptr;

/// A pointer wrapper.
pub struct Ptr<T> {
    ptr: *mut T,
}

impl<T> Ptr<T> {
    /// Create a new `Ptr` with the given value.
    #[inline(always)]
    pub fn new(value: T) -> Self {
        let ptr = unsafe { alloc(Layout::new::<T>()) }.cast::<T>();
        unsafe { ptr.write(value) };
        Self { ptr }
    }

    /// Create a null `Ptr`.
    #[inline(always)]
    pub fn null() -> Self {
        Self {
            ptr: ptr::null_mut(),
        }
    }

    /// Check if the pointer is null.
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.ptr.is_null()
    }

    /// Get the raw pointer.
    #[inline(always)]
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    /// Get a reference to the value.
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// # Panics
    ///
    /// If the pointer is null.
    #[inline(always)]
    pub unsafe fn as_ref<'pointer>(self) -> &'pointer T {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe { &*self.ptr }
    }

    /// Get a mutable reference to the value.
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// # Panics
    ///
    /// If the pointer is null.
    #[inline(always)]
    pub unsafe fn as_mut<'pointer>(self) -> &'pointer mut T {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe { &mut *self.ptr }
    }

    /// Returns the pointer as an u64.
    /// Use [`Ptr::from`](#method.from) to convert it back.
    #[inline(always)]
    pub fn as_u64(&self) -> u64 {
        self.ptr as u64
    }

    /// Drop the value by calling the destructor, but not deallocate memory.
    /// To deallocate memory, use [`Ptr::deallocate`](#method.deallocate).
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// # Panics
    ///
    /// If the pointer is null.  
    pub unsafe fn drop_in_place(self) {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe {
            ptr::drop_in_place(self.ptr);
        }
    }

    /// Drop the value without calling a destructor.
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// And be careful; be sure that you call [`drop_in_place`](Self::drop_in_place)
    /// if you need to drop the value.
    ///
    /// # Panics
    ///
    /// If the pointer is null.  
    #[inline(always)]
    pub unsafe fn deallocate(self) {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe {
            dealloc(self.ptr.cast::<u8>(), Layout::new::<T>());
        }
    }

    /// Drop the value by calling the destructor and then deallocate.
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// # Panics
    ///
    /// If the pointer is null.  
    #[inline(always)]
    pub unsafe fn drop_and_deallocate(self) {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe {
            ptr::drop_in_place(self.ptr);
            dealloc(self.ptr.cast::<u8>(), Layout::new::<T>());
        }
    }

    /// Return the value. It will not lead to the pointer value being dropped.
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// # Panics
    ///
    /// If the pointer is null.  
    #[inline(always)]
    pub unsafe fn read(self) -> T {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe { ptr::read(self.ptr) }
    }

    /// Set the value. Does not call drop the old value.
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// # Panics
    ///
    /// If the pointer is null.  
    ///
    /// #  Write with drop
    ///
    /// Call [`Ptr::write_with_drop`] instead.
    #[inline(always)]
    pub unsafe fn write(self, value: T) {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe { ptr::write(self.ptr, value) }
    }

    /// Set the value. Drops the old value.
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// # Panics
    ///
    /// If the pointer is null.  
    ///
    /// #  Write no drop
    ///
    /// Call [`Ptr::write`] instead.
    #[inline(always)]
    pub unsafe fn write_with_drop(self, value: T) {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe {
            ptr::drop_in_place(self.ptr);
            ptr::write(self.ptr, value);
        }
    }

    /// Replace the value, returning the old value.
    ///
    /// # Safety
    ///
    /// The pointer must not be null.
    ///
    /// # Panics
    ///
    /// If the pointer is null.  
    #[inline(always)]
    pub unsafe fn replace(self, value: T) -> T {
        assert!(!self.ptr.is_null(), "ptr is null");

        unsafe { ptr::replace(self.ptr, value) }
    }
}

impl<T> Clone for Ptr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Ptr<T> {}

unsafe impl<T: Send> Send for Ptr<T> {}
unsafe impl<T: Sync> Sync for Ptr<T> {}

impl<T> From<usize> for Ptr<T> {
    fn from(ptr: usize) -> Self {
        Self { ptr: ptr as *mut T }
    }
}

impl<T> From<u64> for Ptr<T> {
    fn from(ptr: u64) -> Self {
        Self { ptr: ptr as *mut T }
    }
}

impl<T> From<&mut T> for Ptr<T> {
    fn from(ptr: &mut T) -> Self {
        Self { ptr }
    }
}

impl<T: Debug> Debug for Ptr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { write!(f, "{:?}", self.as_ref()) }
    }
}

#[cfg(test)]
mod tests {
    use super::Ptr;
    use crate as orengine;

    struct MustDropIfCounterMoreThanOne {
        #[allow(dead_code)]
        counter: u32,
    }

    impl Drop for MustDropIfCounterMoreThanOne {
        fn drop(&mut self) {
            assert!(self.counter <= 1, "dropped");
        }
    }

    #[orengine::test::test_local]
    fn test_new() {
        let value = 10;
        let ptr = Ptr::new(value);
        unsafe {
            assert_eq!(*ptr.as_ref(), value);
            ptr.deallocate();
        }
    }

    #[orengine::test::test_local]
    fn test_null() {
        let ptr: Ptr<i32> = Ptr::null();
        assert!(ptr.is_null());
    }

    #[orengine::test::test_local]
    fn test_as_ptr() {
        let value = 20;
        let ptr = Ptr::new(value);
        let raw_ptr = ptr.as_ptr();
        unsafe {
            assert_eq!(*raw_ptr, value);
            ptr.deallocate();
        }
    }

    #[orengine::test::test_local]
    fn test_as_ref() {
        let value = 30;
        let ptr = Ptr::new(value);
        unsafe {
            let value_ref = ptr.as_ref();
            assert_eq!(*value_ref, value);
            ptr.deallocate();
        }
    }

    #[orengine::test::test_local]
    fn test_as_mut() {
        let value = 40;
        let ptr = Ptr::new(value);
        unsafe {
            let value_mut = ptr.as_mut();
            *value_mut = 50;
            assert_eq!(*ptr.as_ref(), 50);
            ptr.deallocate();
        }
    }

    #[orengine::test::test_local]
    #[should_panic(expected = "ptr is null")]
    fn test_as_ref_null() {
        let ptr: Ptr<i32> = Ptr::null();
        unsafe {
            let _ = ptr.as_ref();
        }
    }

    #[orengine::test::test_local]
    #[should_panic(expected = "ptr is null")]
    fn test_as_mut_null() {
        let ptr: Ptr<i32> = Ptr::null();
        unsafe {
            let _ = ptr.as_mut();
        }
    }

    #[orengine::test::test_local]
    fn test_as_u64() {
        let value = 60;
        let ptr = Ptr::new(value);
        let raw_u64 = ptr.as_u64();
        let converted_ptr: Ptr<i32> = Ptr::from(raw_u64);
        unsafe {
            assert_eq!(*converted_ptr.as_ref(), value);
            ptr.deallocate();
        }
    }

    #[orengine::test::test_local]
    #[should_panic(expected = "dropped")]
    fn test_drop_in_place() {
        let value = MustDropIfCounterMoreThanOne { counter: 5 };
        let ptr = Ptr::new(value);
        unsafe {
            ptr.drop_in_place();
        }
    }

    #[orengine::test::test_local]
    #[should_panic(expected = "dropped")]
    fn test_drop_and_deallocate() {
        let value = MustDropIfCounterMoreThanOne { counter: 5 };
        let ptr = Ptr::new(value);
        unsafe {
            ptr.drop_and_deallocate();
        }
    }

    #[orengine::test::test_local]
    fn test_read() {
        let value = 70;
        let ptr = Ptr::new(value);
        unsafe {
            assert_eq!(ptr.read(), value);
            ptr.deallocate();
        }
    }

    #[orengine::test::test_local]
    fn test_write() {
        let value = 80;
        let ptr = Ptr::new(value);
        unsafe {
            ptr.write(90);
            assert_eq!(*ptr.as_ref(), 90);
            ptr.deallocate();
        }
    }

    #[orengine::test::test_local]
    #[should_panic(expected = "dropped")]
    fn test_write_with_drop() {
        let value = MustDropIfCounterMoreThanOne { counter: 2 };
        let ptr = Ptr::new(value);
        unsafe {
            ptr.write_with_drop(MustDropIfCounterMoreThanOne { counter: 1 });
        }
    }

    #[orengine::test::test_local]
    fn test_replace() {
        let value = 100;
        let ptr = Ptr::new(value);
        unsafe {
            assert_eq!(ptr.replace(110), value);
            assert_eq!(*ptr.as_ref(), 110);
            ptr.deallocate();
        }
    }

    #[orengine::test::test_local]
    fn test_debug() {
        let value = 130;
        let ptr = Ptr::new(value);
        let debug_str = format!("{ptr:?}");
        assert_eq!(debug_str, "130");

        unsafe {
            ptr.deallocate();
        }
    }
}
