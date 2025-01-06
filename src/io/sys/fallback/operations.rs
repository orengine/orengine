use crate::io::sys::{
    self, FromRawFile, FromRawSocket, IntoRawFile, IntoRawSocket, MessageRecvHeader,
    OsMessageHeader, OsOpenOptions, OsPathPtr, RawFile, RawSocket,
};
use positioned_io::{ReadAt, WriteAt};
use socket2::{Domain, Protocol, SockAddr, Type};
use std::io::{Read, Write};
use std::mem::MaybeUninit;
use std::net::Shutdown;
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
use std::{io, mem, ptr};

/// Performs provided `FnOnce` on a created from a provided [`RawSocket`]
/// [`socket2::Socket`](socket2::Socket), and forgets it.
fn with_socket<F: FnOnce(&mut socket2::Socket) -> io::Result<usize>>(
    raw_socket: RawSocket,
    f: F,
) -> io::Result<usize> {
    let mut socket = unsafe { socket2::Socket::from_raw_socket(raw_socket) };
    let res = f(&mut socket);

    mem::forget(socket);

    res
}

/// Performs provided `FnOnce` on a created from a provided [`RawFile`]
/// [`std::fs::File`](std::fs::File), and forgets it.
fn with_file<F: FnOnce(&mut std::fs::File) -> io::Result<usize>>(
    raw_file: RawFile,
    f: F,
) -> io::Result<usize> {
    let mut file = unsafe { std::fs::File::from_raw_file(raw_file) };
    let res = f(&mut file);

    mem::forget(file);

    res
}

/// Creates a new socket and returns its file descriptor.
#[allow(
    clippy::cast_possible_truncation,
    reason = "RawSocket never exceeds u32"
)]
pub(crate) fn socket_op(
    domain: Domain,
    socket_type: Type,
    protocol: Protocol,
) -> io::Result<usize> {
    socket2::Socket::new(domain, socket_type, Some(protocol)).map(|socket| {
        socket.set_nonblocking(true).unwrap();

        IntoRawSocket::into_raw_socket(socket) as usize
    })
}

/// Accepts a connection on the socket.
#[allow(
    clippy::cast_possible_truncation,
    reason = "RawSocket never exceeds u32"
)]
pub(crate) fn accept_op(
    raw_listener: RawSocket,
    sockaddr_ptr: *mut sys::os_sockaddr,
    sockaddr_len_ptr: *mut sys::socklen_t,
) -> io::Result<usize> {
    with_socket(raw_listener, |listener| {
        listener.accept().map(|(socket, addr)| {
            socket.set_nonblocking(true).unwrap();

            #[allow(
                clippy::cast_ptr_alignment,
                reason = "sys::os_sockaddr is aligned rightly"
            )]
            unsafe {
                ptr::copy_nonoverlapping(addr.as_ptr().cast::<sys::os_sockaddr>(), sockaddr_ptr, 1);
                ptr::write(sockaddr_len_ptr, addr.len());
            }

            IntoRawSocket::into_raw_socket(socket) as usize
        })
    })
}

/// Connects the socket to a remote address.
pub(crate) fn connect_op(
    raw_socket: RawSocket,
    addr_ptr: *const sys::os_sockaddr,
    addr_len: sys::socklen_t,
) -> io::Result<usize> {
    with_socket(raw_socket, |socket| {
        let addr = unsafe { SockAddr::new(ptr::read(addr_ptr.cast()), addr_len) };

        socket.connect(&addr).map(|()| 0)
    })
}

/// Receives data on the socket.
pub(crate) fn recv_op(raw_socket: RawSocket, buf_ptr: *mut u8, buf_len: u32) -> io::Result<usize> {
    with_socket(raw_socket, |socket| {
        let slice = unsafe {
            std::slice::from_raw_parts_mut(buf_ptr.cast::<MaybeUninit<u8>>(), buf_len as _)
        };
        socket.recv(slice)
    })
}

/// Receives data from the socket.
pub(crate) fn recv_from_op(
    raw_socket: RawSocket,
    header_ptr: *mut MessageRecvHeader,
) -> io::Result<usize> {
    with_socket(raw_socket, |socket| {
        let header = unsafe { &mut *header_ptr }.get_os_message_header();
        let slice = unsafe { &mut **header.0.cast::<*mut [MaybeUninit<u8>]>() };
        let addr_ptr = header.1;
        socket.recv_from(slice).map(|(n, sock_addr)| {
            unsafe { ptr::write(addr_ptr, sock_addr) };

            n
        })
    })
}

/// Sends data on the socket.
pub(crate) fn send_op(
    raw_socket: RawSocket,
    buf_ptr: *const u8,
    buf_len: u32,
) -> io::Result<usize> {
    with_socket(raw_socket, |socket| {
        let slice = unsafe { std::slice::from_raw_parts(buf_ptr.cast(), buf_len as _) };

        socket.send(slice)
    })
}

/// Sends data to the address.
pub(crate) fn send_to_op(
    raw_socket: RawSocket,
    header_ptr: *const OsMessageHeader,
) -> io::Result<usize> {
    with_socket(raw_socket, |socket| {
        let header = unsafe { &*header_ptr };
        let slice = unsafe { &**header.0 };

        socket.send_to(slice, unsafe { &*header.1 })
    })
}

/// Receives data from the socket without consuming it.
pub(crate) fn peek_op(raw_socket: RawSocket, buf_ptr: *mut u8, buf_len: u32) -> io::Result<usize> {
    with_socket(raw_socket, |socket| {
        let slice = unsafe { std::slice::from_raw_parts_mut(buf_ptr.cast(), buf_len as _) };

        socket.peek(slice)
    })
}

/// Receives data from the addr without consuming it.
pub(crate) fn peek_from_op(
    raw_socket: RawSocket,
    header_ptr: *mut MessageRecvHeader,
) -> io::Result<usize> {
    with_socket(raw_socket, |socket| {
        let header = unsafe { &mut *header_ptr }.get_os_message_header();
        let slice = unsafe { &mut **header.0.cast::<*mut [MaybeUninit<u8>]>() };
        let addr_ptr = header.1;

        socket.peek_from(slice).map(|(n, sock_addr)| {
            unsafe { ptr::write(addr_ptr, sock_addr) };

            n
        })
    })
}

/// Shuts down the socket.
pub(crate) fn shutdown_op(raw_socket: RawSocket, how: Shutdown) -> io::Result<usize> {
    with_socket(raw_socket, |socket| socket.shutdown(how).map(|()| 0))
}

/// Opens a file.
pub(crate) fn open_op(path_ptr: OsPathPtr, open_how: *const OsOpenOptions) -> io::Result<usize> {
    let open_how = unsafe { &*open_how };
    let path = unsafe { &*path_ptr };

    open_how
        .open(path)
        .map(|file| file.into_raw_file() as usize)
}

/// Syncs a file to disk.
pub(crate) fn fsync_op(raw_file: RawFile) -> io::Result<usize> {
    with_file(raw_file, |file| file.sync_all().map(|()| 0))
}

/// Syncs a file data to disk.
pub(crate) fn fsync_data_op(raw_file: RawFile) -> io::Result<usize> {
    with_file(raw_file, |file| file.sync_data().map(|()| 0))
}

/// Reads data from a file.
pub(crate) fn read_op(raw_file: RawFile, buf_ptr: *mut u8, buf_len: u32) -> io::Result<usize> {
    with_file(raw_file, |file| {
        let slice = unsafe { std::slice::from_raw_parts_mut(buf_ptr.cast(), buf_len as _) };

        file.read(slice)
    })
}

/// Reads data from a file with the given offset.
pub(crate) fn read_at_op(
    raw_file: RawFile,
    offset: u64,
    buf_ptr: *mut u8,
    buf_len: u32,
) -> io::Result<usize> {
    with_file(raw_file, |file| {
        let slice = unsafe { std::slice::from_raw_parts_mut(buf_ptr.cast(), buf_len as _) };

        file.read_at(offset, slice)
    })
}

/// Writes data to a file.
pub(crate) fn write_op(raw_file: RawFile, buf_ptr: *const u8, buf_len: u32) -> io::Result<usize> {
    with_file(raw_file, |file| {
        let slice = unsafe { std::slice::from_raw_parts(buf_ptr.cast(), buf_len as _) };

        file.write(slice)
    })
}

/// Writes data to a file with the given offset.
pub(crate) fn write_at_op(
    raw_file: RawFile,
    offset: u64,
    buf_ptr: *const u8,
    buf_len: u32,
) -> io::Result<usize> {
    with_file(raw_file, |file| {
        let slice = unsafe { std::slice::from_raw_parts(buf_ptr.cast(), buf_len as _) };

        file.write_at(offset, slice)
    })
}

/// Closes a file.
pub(crate) fn close_file_op(raw_file: RawFile) {
    drop(unsafe { std::fs::File::from_raw_file(raw_file) });
}

/// Closes a socket.
pub(crate) fn close_socket_op(raw_socket: RawSocket) {
    drop(unsafe { std::net::UdpSocket::from_raw_socket(raw_socket) });
}

/// Renames a file.
pub(crate) fn rename_op(old_path_ptr: OsPathPtr, new_path_ptr: OsPathPtr) -> io::Result<usize> {
    let old_path = unsafe { &*old_path_ptr };
    let new_path = unsafe { &*new_path_ptr };

    std::fs::rename(old_path, new_path).map(|()| 0)
}

/// Creates a directory.
#[allow(unused_variables, reason = "We use #[cfg] here.")]
pub(crate) fn mkdir_op(path_ptr: OsPathPtr, mode: u32) -> io::Result<usize> {
    let path = unsafe { &*path_ptr };
    let mut dir_builder = std::fs::DirBuilder::new();
    let dir_builder_ref = dir_builder.recursive(true);

    #[cfg(unix)]
    let dir_builder_ref = dir_builder_ref.mode(0o777);

    dir_builder_ref.create(path).map(|()| 0)
}

/// Removes a directory.
pub(crate) fn rmdir_op(path_ptr: OsPathPtr) -> io::Result<usize> {
    let path = unsafe { &*path_ptr };

    std::fs::remove_dir(path).map(|()| 0)
}

/// Removes a file.
pub(crate) fn unlink_op(path_ptr: OsPathPtr) -> io::Result<usize> {
    let path = unsafe { &*path_ptr };

    std::fs::remove_file(path).map(|()| 0)
}
