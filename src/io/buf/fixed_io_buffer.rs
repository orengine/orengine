use crate::utils::Sealed;

/// Represents an immutable __fixed__ I/O buffer.
pub trait FixedBuffer: Sealed {
    /// Returns a pointer to the buffer data.
    fn as_ptr(&self) -> *const u8;
    /// Returns a bytes slice of the buffer.
    fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.as_ptr(), self.len_u32() as _) }
    }
    /// Returns length of the buffer as u32.
    fn len_u32(&self) -> u32;
    /// Returns the index of the __fixed__ buffer or `u16::MAX`.
    fn fixed_index(&self) -> u16;
    /// Returns whether the buffer is __fixed__.
    fn is_fixed(&self) -> bool;
}

/// Represents an mutable __fixed__ I/O buffer.
pub trait FixedBufferMut: FixedBuffer {
    /// Returns a mutable pointer to the buffer data.
    fn as_mut_ptr(&mut self) -> *mut u8;
    /// Returns a mutable bytes slice of the buffer.
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len_u32() as _) }
    }
}
