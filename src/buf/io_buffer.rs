/// Represents an immutable I/O buffer.
///
/// This trait is used to interact with types that provide read-only access
/// to a contiguous region of memory.
///
/// # Required Traits:
///
/// - [`AsRef<[u8]>`](AsRef): Enables a type to expose a reference to its inner `[u8]` buffer.
///
/// # About [`is_fixed()`](Self::is_fixed) and [`fixed_index()`](Self::fixed_index):
///
/// - Do not override these methods.
///
/// - They are inlined, so it is zero cost to use, if they are not overridden.
pub trait IOBuffer: AsRef<[u8]> {
    /// Returns length of the buffer as u32.
    #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
    #[inline(always)]
    fn len(&self) -> u32 {
        self.as_ref().len() as u32
    }

    /// Returns the index of the __fixed__ buffer or `u16::MAX`.
    #[inline(always)]
    fn fixed_index(&self) -> u16 {
        u16::MAX
    }

    /// Returns whether the buffer is __fixed__.
    ///
    /// It is inlined.
    #[inline(always)]
    fn is_fixed(&self) -> bool {
        false
    }
}

/// Represents a mutable I/O buffer.
///
/// This trait extends IOBuffer to provide read-write access to the underlying memory.
///
/// # Required Traits:
///
/// - [`IOBuffer`]: Provides immutable access to the buffer.
/// - [`AsMut<[u8]>`](AsMut): Enables a type to expose a mutable reference
///   to its inner `[u8]` buffer.
pub trait IOBufferMut: IOBuffer + AsMut<[u8]> {}

// region impl for std types

impl IOBuffer for Vec<u8> {}
impl IOBufferMut for Vec<u8> {}

impl IOBuffer for Box<[u8]> {}
impl IOBufferMut for Box<[u8]> {}

impl<const N: usize> IOBuffer for [u8; N] {}
impl<const N: usize> IOBufferMut for [u8; N] {}

impl IOBuffer for &[u8] {}

impl IOBuffer for &mut [u8] {}
impl IOBufferMut for &mut [u8] {}

// endregion
