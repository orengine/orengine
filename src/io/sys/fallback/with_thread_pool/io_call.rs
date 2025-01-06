use crate::io::sys::fallback::operations::{
    accept_op, close_file_op, close_socket_op, connect_op, fsync_data_op, fsync_op, mkdir_op,
    open_op, peek_from_op, peek_op, read_at_op, read_op, recv_from_op, recv_op, rename_op,
    rmdir_op, send_op, send_to_op, shutdown_op, socket_op, unlink_op, write_at_op, write_op,
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
    Fallocate,
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
            Self::Socket(domain, ty, protocol) => socket_op(domain, ty, protocol),

            Self::Accept(raw_listener, sockaddr_ptr, sockaddr_len_ptr) => {
                accept_op(raw_listener, sockaddr_ptr, sockaddr_len_ptr)
            }

            Self::AcceptWithDeadline(raw_listener, sockaddr_ptr, sockaddr_len_ptr, _deadline) => {
                accept_op(raw_listener, sockaddr_ptr, sockaddr_len_ptr)
            }

            Self::Connect(raw_socket, addr_ptr, addr_len) => {
                connect_op(raw_socket, addr_ptr, addr_len)
            }

            Self::ConnectWithDeadline(raw_socket, addr_ptr, addr_len, _deadline) => {
                connect_op(raw_socket, addr_ptr, addr_len)
            }

            Self::PollRecv(_) | Self::PollSend(_) => unreachable!(),

            Self::Recv(raw_socket, buf_ptr, buf_len) => recv_op(raw_socket, buf_ptr, buf_len),

            Self::RecvWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                recv_op(raw_socket, buf_ptr, buf_len)
            }

            Self::RecvFrom(raw_socket, header_ptr) => recv_from_op(raw_socket, header_ptr),

            Self::RecvFromWithDeadline(raw_socket, header_ptr, _deadline) => {
                recv_from_op(raw_socket, header_ptr)
            }

            Self::Send(raw_socket, buf_ptr, buf_len) => send_op(raw_socket, buf_ptr, buf_len),

            Self::SendWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                send_op(raw_socket, buf_ptr, buf_len)
            }

            Self::SendTo(raw_socket, header_ptr) => send_to_op(raw_socket, header_ptr),

            Self::SendToWithDeadline(raw_socket, header_ptr, _deadline) => {
                send_to_op(raw_socket, header_ptr)
            }

            Self::Peek(raw_socket, buf_ptr, buf_len) => peek_op(raw_socket, buf_ptr, buf_len),

            Self::PeekWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                peek_op(raw_socket, buf_ptr, buf_len)
            }

            Self::PeekFrom(raw_socket, header_ptr) => peek_from_op(raw_socket, header_ptr),

            Self::PeekFromWithDeadline(raw_socket, header_ptr, _deadline) => {
                peek_from_op(raw_socket, header_ptr)
            }

            Self::Shutdown(raw_socket, how) => shutdown_op(raw_socket, how),

            Self::Open(path_ptr, options) => open_op(path_ptr, options),

            Self::Fallocate => Ok(0),

            Self::FAllSync(file) => fsync_op(file),

            Self::FDataSync(file) => fsync_data_op(file),

            Self::Read(file, buf_ptr, buf_len) => read_op(file, buf_ptr, buf_len),

            Self::PRead(file, buf_ptr, buf_len, offset) => {
                read_at_op(file, offset, buf_ptr, buf_len)
            }

            Self::Write(file, buf_ptr, buf_len) => write_op(file, buf_ptr, buf_len),

            Self::PWrite(file, buf_ptr, buf_len, offset) => {
                write_at_op(file, offset, buf_ptr, buf_len)
            }

            Self::CloseFile(file) => {
                close_file_op(file);
                Ok(0)
            }

            Self::CloseSocket(socket) => {
                close_socket_op(socket);
                Ok(0)
            }

            Self::Rename(from, to) => rename_op(from, to),

            Self::CreateDir(path_ptr, mode) => mkdir_op(path_ptr, mode),

            Self::RemoveDir(path_ptr) => rmdir_op(path_ptr),

            Self::RemoveFile(path_ptr) => unlink_op(path_ptr),
        }
    }

    /// Checks if the I/O call represented by this `IoCall` is read-pollable.
    pub(crate) const fn is_recv_pollable(&self) -> bool {
        matches!(
            self,
            Self::PollRecv(_)
                | Self::Accept(_, _, _)
                | Self::AcceptWithDeadline(_, _, _, _)
                | Self::Recv(_, _, _)
                | Self::RecvWithDeadline(_, _, _, _)
                | Self::RecvFrom(_, _)
                | Self::RecvFromWithDeadline(_, _, _)
                | Self::Peek(_, _, _)
                | Self::PeekWithDeadline(_, _, _, _)
                | Self::PeekFrom(_, _)
                | Self::PeekFromWithDeadline(_, _, _)
        )
    }

    /// Checks if the I/O call represented by this `IoCall` is write-pollable.
    pub(crate) const fn is_send_pollable(&self) -> bool {
        matches!(
            self,
            Self::PollSend(_)
                | Self::Send(_, _, _)
                | Self::SendWithDeadline(_, _, _, _)
                | Self::SendTo(_, _)
                | Self::SendToWithDeadline(_, _, _)
        )
    }

    /// Checks if the I/O call represented by this `IoCall` is both-pollable.
    pub(crate) const fn is_both_pollable(&self) -> bool {
        matches!(self, Self::Connect(..) | Self::ConnectWithDeadline(..))
    }

    /// Returns a mutable reference to a deadline of the I/O call represented by this `IoCall`
    /// if it has one.
    #[allow(clippy::mut_from_ref, reason = "False positive.")]
    pub(crate) const fn deadline_mut(&self) -> Option<&mut Instant> {
        unsafe {
            #[allow(clippy::match_same_arms, reason = "It is more readable this way.")]
            match self {
                Self::AcceptWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                Self::ConnectWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                Self::RecvWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                Self::RecvFromWithDeadline(_, _, deadline) => Some(&mut **deadline),
                Self::SendWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                Self::SendToWithDeadline(_, _, deadline) => Some(&mut **deadline),
                Self::PeekWithDeadline(_, _, _, deadline) => Some(&mut **deadline),
                Self::PeekFromWithDeadline(_, _, deadline) => Some(&mut **deadline),
                _ => None,
            }
        }
    }

    /// Returns a deadline of the I/O call represented by this `IoCall` if it has one.
    pub(crate) const fn deadline(&self) -> Option<Instant> {
        self.deadline_mut().copied()
    }

    /// Returns an associated raw socket of the I/O call represented by this `IoCall` if it has one.
    pub(crate) const fn raw_socket(&self) -> Option<RawSocket> {
        #[allow(clippy::match_same_arms, reason = "It is more readable this way.")]
        match self {
            Self::Accept(raw_socket, _, _) => Some(*raw_socket),
            Self::AcceptWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            Self::Connect(raw_socket, _, _) => Some(*raw_socket),
            Self::ConnectWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            Self::PollRecv(raw_socket) => Some(*raw_socket),
            Self::PollSend(raw_socket) => Some(*raw_socket),
            Self::Recv(raw_socket, _, _) => Some(*raw_socket),
            Self::RecvWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            Self::RecvFrom(raw_socket, _) => Some(*raw_socket),
            Self::RecvFromWithDeadline(raw_socket, _, _) => Some(*raw_socket),
            Self::Send(raw_socket, _, _) => Some(*raw_socket),
            Self::SendWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            Self::SendTo(raw_socket, _) => Some(*raw_socket),
            Self::SendToWithDeadline(raw_socket, _, _) => Some(*raw_socket),
            Self::Peek(raw_socket, _, _) => Some(*raw_socket),
            Self::PeekWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            Self::PeekFrom(raw_socket, _) => Some(*raw_socket),
            Self::PeekFromWithDeadline(raw_socket, _, _) => Some(*raw_socket),
            _ => None,
        }
    }

    /// Returns whether the I/O call must be retried after polling.
    pub(crate) const fn must_retry(&self) -> bool {
        match self {
            Self::Connect(..)
            | Self::ConnectWithDeadline(..)
            | Self::PollRecv(..)
            | Self::PollSend(..) => false,

            Self::Accept(..)
            | Self::AcceptWithDeadline(..)
            | Self::Recv(..)
            | Self::RecvWithDeadline(..)
            | Self::RecvFrom(..)
            | Self::RecvFromWithDeadline(..)
            | Self::Send(..)
            | Self::SendWithDeadline(..)
            | Self::SendTo(..)
            | Self::SendToWithDeadline(..)
            | Self::Peek(..)
            | Self::PeekWithDeadline(..)
            | Self::PeekFrom(..)
            | Self::PeekFromWithDeadline(..) => true,

            _ => unreachable!(),
        }
    }
}

unsafe impl Send for IoCall {}
