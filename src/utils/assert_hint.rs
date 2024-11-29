/// Do the same as [`assert_unchecked`](std::hint::assert_unchecked), but instead of UB, it panics.
#[inline(always)]
pub(crate) fn assert_hint(cond: bool, debug_msg: &str) {
    #[cfg(debug_assertions)]
    {
        assert!(cond, "{}", debug_msg);
    }

    #[cfg(not(debug_assertions))]
    unsafe {
        std::hint::assert_unchecked(cond)
    };
}
