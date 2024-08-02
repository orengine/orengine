use crate::io::sys::Fd;

pub struct Stream {
    pub(crate) fd: Fd,
}
