use crate::io::sys::fallback::operations::{
    accept_op, close_file_op, close_socket_op, connect_op, fallocate_op, fsync_data_op, fsync_op,
    mkdir_op, open_op, peek_from_op, peek_op, read_at_op, read_op, recv_from_op, recv_op,
    rename_op, rmdir_op, send_op, send_to_op, shutdown_op, socket_op, unlink_op, write_at_op,
    write_op,
};
use crate::io::sys::{
    self, os_sockaddr, MessageRecvHeader, OsMessageHeader, OsOpenOptions, OsPathPtr, RawFile,
    RawSocket,
};
use socket2::{Domain, Protocol, Type};
use std::net::Shutdown;
use std::time::Instant;
use std::{io, ptr};

/// `IoCall` represents a type of I/O call and its arguments.
pub(crate) enum IoCall {
    Socket(Domain, Type, Protocol),
    Accept(RawSocket, *mut os_sockaddr, *mut sys::socklen_t),
    AcceptWithDeadline(
        RawSocket,
        *mut os_sockaddr,
        *mut sys::socklen_t,
        *mut Instant,
    ),
    Connect(RawSocket, *const os_sockaddr, sys::socklen_t),
    ConnectWithDeadline(RawSocket, *const os_sockaddr, sys::socklen_t, *mut Instant),
    PollRecv(RawSocket),
    PollSend(RawSocket),
    Recv(RawSocket, *mut u8, u32),
    RecvWithDeadline(RawSocket, *mut u8, u32, *mut Instant),
    RecvFrom(RawSocket, *mut MessageRecvHeader),
    RecvFromWithDeadline(RawSocket, *mut MessageRecvHeader, *mut Instant),
    Send(RawSocket, *const u8, u32),
    SendWithDeadline(RawSocket, *const u8, u32, *mut Instant),
    SendTo(RawSocket, *const OsMessageHeader),
    SendToWithDeadline(RawSocket, *const OsMessageHeader, *mut Instant),
    Peek(RawSocket, *mut u8, u32),
    PeekWithDeadline(RawSocket, *mut u8, u32, *mut Instant),
    PeekFrom(RawSocket, *mut MessageRecvHeader),
    PeekFromWithDeadline(RawSocket, *mut MessageRecvHeader, *mut Instant),
    Shutdown(RawSocket, Shutdown),
    Open(OsPathPtr, *const OsOpenOptions),
    Fallocate(RawFile, u64, u64, i32),
    FAllSync(RawFile),
    FDataSync(RawFile),
    Read(RawFile, *mut u8, u32),
    PRead(RawFile, *mut u8, u32, u64),
    Write(RawFile, *const u8, u32),
    PWrite(RawFile, *const u8, u32, u64),
    CloseFile(RawFile),
    CloseSocket(RawSocket),
    Rename(OsPathPtr, OsPathPtr),
    CreateDir(OsPathPtr, u32),
    RemoveDir(OsPathPtr),
    RemoveFile(OsPathPtr),
}

impl IoCall {
    /// Performs the I/O call represented by this `IoCall`.
    pub(crate) fn do_io_work(&self) -> io::Result<usize> {
        match unsafe { ptr::read(self) } {
            IoCall::Socket(domain, ty, protocol) => socket_op(domain, ty, protocol),

            IoCall::Accept(raw_listener, sockaddr_ptr, sockaddr_len_ptr) => {
                accept_op(raw_listener, sockaddr_ptr, sockaddr_len_ptr)
            }

            IoCall::AcceptWithDeadline(raw_listener, sockaddr_ptr, sockaddr_len_ptr, _deadline) => {
                accept_op(raw_listener, sockaddr_ptr, sockaddr_len_ptr)
            }

            IoCall::Connect(raw_socket, addr_ptr, addr_len) => {
                connect_op(raw_socket, addr_ptr, addr_len)
            }

            IoCall::ConnectWithDeadline(raw_socket, addr_ptr, addr_len, _deadline) => {
                connect_op(raw_socket, addr_ptr, addr_len)
            }

            IoCall::PollRecv(_) => unreachable!(),

            IoCall::PollSend(_) => unreachable!(),

            IoCall::Recv(raw_socket, buf_ptr, buf_len) => recv_op(raw_socket, buf_ptr, buf_len),

            IoCall::RecvWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                recv_op(raw_socket, buf_ptr, buf_len)
            }

            IoCall::RecvFrom(raw_socket, header_ptr) => recv_from_op(raw_socket, header_ptr),

            IoCall::RecvFromWithDeadline(raw_socket, header_ptr, _deadline) => {
                recv_from_op(raw_socket, header_ptr)
            }

            IoCall::Send(raw_socket, buf_ptr, buf_len) => send_op(raw_socket, buf_ptr, buf_len),

            IoCall::SendWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                send_op(raw_socket, buf_ptr, buf_len)
            }

            IoCall::SendTo(raw_socket, header_ptr) => send_to_op(raw_socket, header_ptr),

            IoCall::SendToWithDeadline(raw_socket, header_ptr, _deadline) => {
                send_to_op(raw_socket, header_ptr)
            }

            IoCall::Peek(raw_socket, buf_ptr, buf_len) => peek_op(raw_socket, buf_ptr, buf_len),

            IoCall::PeekWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                peek_op(raw_socket, buf_ptr, buf_len)
            }

            IoCall::PeekFrom(raw_socket, header_ptr) => peek_from_op(raw_socket, header_ptr),

            IoCall::PeekFromWithDeadline(raw_socket, header_ptr, _deadline) => {
                peek_from_op(raw_socket, header_ptr)
            }

            IoCall::Shutdown(raw_socket, how) => shutdown_op(raw_socket, how),

            IoCall::Open(path_ptr, options) => open_op(path_ptr, options),

            IoCall::Fallocate(file, offset, len, mode) => fallocate_op(file, offset, len, mode),

            IoCall::FAllSync(file) => fsync_op(file),

            IoCall::FDataSync(file) => fsync_data_op(file),

            IoCall::Read(file, buf_ptr, buf_len) => read_op(file, buf_ptr, buf_len),

            IoCall::PRead(file, buf_ptr, buf_len, offset) => {
                read_at_op(file, offset, buf_ptr, buf_len)
            }

            IoCall::Write(file, buf_ptr, buf_len) => write_op(file, buf_ptr, buf_len),

            IoCall::PWrite(file, buf_ptr, buf_len, offset) => {
                write_at_op(file, offset, buf_ptr, buf_len)
            }

            IoCall::CloseFile(file) => close_file_op(file),

            IoCall::CloseSocket(socket) => close_socket_op(socket),

            IoCall::Rename(from, to) => rename_op(from, to),

            IoCall::CreateDir(path_ptr, mode) => mkdir_op(path_ptr, mode),

            IoCall::RemoveDir(path_ptr) => rmdir_op(path_ptr),

            IoCall::RemoveFile(path_ptr) => unlink_op(path_ptr),
        }
    }

    /// Checks if the I/O call represented by this `IoCall` is pollable and readable.
    pub(crate) const fn is_recv_pollable(&self) -> bool {
        match self {
            IoCall::PollRecv(_) => true,
            IoCall::Accept(_, _, _) => true,
            IoCall::AcceptWithDeadline(_, _, _, _) => true,
            IoCall::Recv(_, _, _) => true,
            IoCall::RecvWithDeadline(_, _, _, _) => true,
            IoCall::RecvFrom(_, _) => true,
            IoCall::RecvFromWithDeadline(_, _, _) => true,
            IoCall::Peek(_, _, _) => true,
            IoCall::PeekWithDeadline(_, _, _, _) => true,
            IoCall::PeekFrom(_, _) => true,
            IoCall::PeekFromWithDeadline(_, _, _) => true,
            _ => false,
        }
    }

    /// Checks if the I/O call represented by this `IoCall` is pollable and writable.
    pub(crate) const fn is_send_pollable(&self) -> bool {
        match self {
            IoCall::Connect(_, _, _) => true,
            IoCall::ConnectWithDeadline(_, _, _, _) => true,
            IoCall::PollSend(_) => true,
            IoCall::Send(_, _, _) => true,
            IoCall::SendWithDeadline(_, _, _, _) => true,
            IoCall::SendTo(_, _) => true,
            IoCall::SendToWithDeadline(_, _, _) => true,
            _ => false,
        }
    }

    /// Returns a deadline of the I/O call represented by this `IoCall` if it has one.
    pub(crate) const fn deadline(&self) -> Option<&mut Instant> {
        unsafe {
            match self {
                IoCall::AcceptWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                IoCall::ConnectWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                IoCall::RecvWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                IoCall::RecvFromWithDeadline(_, _, deadline) => Some(&mut **deadline),
                IoCall::SendWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                IoCall::SendToWithDeadline(_, _, deadline) => Some(&mut **deadline),
                IoCall::PeekWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                IoCall::PeekFromWithDeadline(_, _, deadline) => Some(&mut **deadline),
                _ => None,
            }
        }
    }

    /// Returns an associated raw socket of the I/O call represented by this `IoCall` if it has one.
    pub(crate) const fn raw_socket(&self) -> Option<RawSocket> {
        match self {
            IoCall::Accept(raw_socket, _, _) => Some(*raw_socket),
            IoCall::AcceptWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            IoCall::Connect(raw_socket, _, _) => Some(*raw_socket),
            IoCall::ConnectWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            IoCall::PollRecv(raw_socket) => Some(*raw_socket),
            IoCall::PollSend(raw_socket) => Some(*raw_socket),
            IoCall::Recv(raw_socket, _, _) => Some(*raw_socket),
            IoCall::RecvWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            IoCall::RecvFrom(raw_socket, _) => Some(*raw_socket),
            IoCall::RecvFromWithDeadline(raw_socket, _, _) => Some(*raw_socket),
            IoCall::Send(raw_socket, _, _) => Some(*raw_socket),
            IoCall::SendWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            IoCall::SendTo(raw_socket, _) => Some(*raw_socket),
            IoCall::SendToWithDeadline(raw_socket, _, _) => Some(*raw_socket),
            IoCall::Peek(raw_socket, _, _) => Some(*raw_socket),
            IoCall::PeekWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            IoCall::PeekFrom(raw_socket, _) => Some(*raw_socket),
            IoCall::PeekFromWithDeadline(raw_socket, _, _) => Some(*raw_socket),
            _ => None,
        }
    }
}
