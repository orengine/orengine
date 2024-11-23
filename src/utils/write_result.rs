/// shortcut for `unsafe { $ptr.write(Ok($res)) }`
#[macro_export]
macro_rules! write_ok {
    ($ptr:expr, $res:expr) => {
        unsafe { $ptr.write(Ok($res)) }
    };
}

/// shortcut for `unsafe { $ptr.write(Err($err)) }`
#[macro_export]
macro_rules! write_err {
    ($ptr:expr, $err:expr) => {
        unsafe { $ptr.write(Err($err)) }
    };
}
