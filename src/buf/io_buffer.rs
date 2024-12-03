// TODO docs
pub trait IOBuffer: AsRef<[u8]> {
    #[inline(always)]
    fn is_fixed(&self) -> bool {
        false
    }
}

pub trait IOBufferMut: IOBuffer + AsMut<[u8]> {}
