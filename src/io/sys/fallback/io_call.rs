use crate::io::sys::fallback::operations;
use crate::io::sys::{self, os_sockaddr, MessageRecvHeader, OsMessageHeader, RawSocket};
#[cfg(feature = "fallback_thread_pool")]
use crate::io::sys::{OsOpenOptions, OsPathPtr, RawFile};

use std::time::Instant;
use std::{io, ptr};

/// `IoCall` represents a type of I/O call and its arguments.
pub(crate) enum IoCall {
    #[cfg(feature = "fallback_thread_pool")]
    Socket(socket2::Domain, socket2::Type, socket2::Protocol),
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
    PollRecvWithDeadline(RawSocket, *mut Instant),
    PollSend(RawSocket),
    PollSendWithDeadline(RawSocket, *mut Instant),
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
    #[cfg(feature = "fallback_thread_pool")]
    Shutdown(RawSocket, std::net::Shutdown),
    #[cfg(feature = "fallback_thread_pool")]
    Open(OsPathPtr, *const OsOpenOptions),
    #[cfg(feature = "fallback_thread_pool")]
    Fallocate,
    #[cfg(feature = "fallback_thread_pool")]
    FAllSync(RawFile),
    #[cfg(feature = "fallback_thread_pool")]
    FDataSync(RawFile),
    #[cfg(feature = "fallback_thread_pool")]
    Read(RawFile, *mut u8, u32),
    #[cfg(feature = "fallback_thread_pool")]
    PRead(RawFile, *mut u8, u32, u64),
    #[cfg(feature = "fallback_thread_pool")]
    Write(RawFile, *const u8, u32),
    #[cfg(feature = "fallback_thread_pool")]
    PWrite(RawFile, *const u8, u32, u64),
    #[cfg(feature = "fallback_thread_pool")]
    CloseFile(RawFile),
    #[cfg(feature = "fallback_thread_pool")]
    CloseSocket(RawSocket),
    #[cfg(feature = "fallback_thread_pool")]
    Rename(OsPathPtr, OsPathPtr),
    #[cfg(feature = "fallback_thread_pool")]
    CreateDir(OsPathPtr, u32),
    #[cfg(feature = "fallback_thread_pool")]
    RemoveDir(OsPathPtr),
    #[cfg(feature = "fallback_thread_pool")]
    RemoveFile(OsPathPtr),
}

impl IoCall {
    /// Performs the I/O call represented by this `IoCall`.
    pub(crate) fn do_io_work(&self) -> io::Result<usize> {
        match unsafe { ptr::read(self) } {
            #[cfg(feature = "fallback_thread_pool")]
            Self::Socket(domain, ty, protocol) => operations::socket_op(domain, ty, protocol),

            Self::Accept(raw_listener, sockaddr_ptr, sockaddr_len_ptr) => {
                operations::accept_op(raw_listener, sockaddr_ptr, sockaddr_len_ptr)
            }

            Self::AcceptWithDeadline(raw_listener, sockaddr_ptr, sockaddr_len_ptr, _deadline) => {
                operations::accept_op(raw_listener, sockaddr_ptr, sockaddr_len_ptr)
            }

            Self::Connect(raw_socket, addr_ptr, addr_len) => {
                operations::connect_op(raw_socket, addr_ptr, addr_len)
            }

            Self::ConnectWithDeadline(raw_socket, addr_ptr, addr_len, _deadline) => {
                operations::connect_op(raw_socket, addr_ptr, addr_len)
            }

            Self::PollRecv(..)
            | Self::PollSend(..)
            | Self::PollRecvWithDeadline(..)
            | Self::PollSendWithDeadline(..) => unreachable!(),

            Self::Recv(raw_socket, buf_ptr, buf_len) => {
                operations::recv_op(raw_socket, buf_ptr, buf_len)
            }

            Self::RecvWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                operations::recv_op(raw_socket, buf_ptr, buf_len)
            }

            Self::RecvFrom(raw_socket, header_ptr) => {
                operations::recv_from_op(raw_socket, header_ptr)
            }

            Self::RecvFromWithDeadline(raw_socket, header_ptr, _deadline) => {
                operations::recv_from_op(raw_socket, header_ptr)
            }

            Self::Send(raw_socket, buf_ptr, buf_len) => {
                operations::send_op(raw_socket, buf_ptr, buf_len)
            }

            Self::SendWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                operations::send_op(raw_socket, buf_ptr, buf_len)
            }

            Self::SendTo(raw_socket, header_ptr) => operations::send_to_op(raw_socket, header_ptr),

            Self::SendToWithDeadline(raw_socket, header_ptr, _deadline) => {
                operations::send_to_op(raw_socket, header_ptr)
            }

            Self::Peek(raw_socket, buf_ptr, buf_len) => {
                operations::peek_op(raw_socket, buf_ptr, buf_len)
            }

            Self::PeekWithDeadline(raw_socket, buf_ptr, buf_len, _deadline) => {
                operations::peek_op(raw_socket, buf_ptr, buf_len)
            }

            Self::PeekFrom(raw_socket, header_ptr) => {
                operations::peek_from_op(raw_socket, header_ptr)
            }

            Self::PeekFromWithDeadline(raw_socket, header_ptr, _deadline) => {
                operations::peek_from_op(raw_socket, header_ptr)
            }

            #[cfg(feature = "fallback_thread_pool")]
            Self::Shutdown(raw_socket, how) => operations::shutdown_op(raw_socket, how),

            #[cfg(feature = "fallback_thread_pool")]
            Self::Open(path_ptr, options) => operations::open_op(path_ptr, options),

            #[cfg(feature = "fallback_thread_pool")]
            Self::Fallocate => Ok(0),

            #[cfg(feature = "fallback_thread_pool")]
            Self::FAllSync(file) => operations::fsync_op(file),

            #[cfg(feature = "fallback_thread_pool")]
            Self::FDataSync(file) => operations::fsync_data_op(file),

            #[cfg(feature = "fallback_thread_pool")]
            Self::Read(file, buf_ptr, buf_len) => operations::read_op(file, buf_ptr, buf_len),

            #[cfg(feature = "fallback_thread_pool")]
            Self::PRead(file, buf_ptr, buf_len, offset) => {
                operations::read_at_op(file, offset, buf_ptr, buf_len)
            }

            #[cfg(feature = "fallback_thread_pool")]
            Self::Write(file, buf_ptr, buf_len) => operations::write_op(file, buf_ptr, buf_len),

            #[cfg(feature = "fallback_thread_pool")]
            Self::PWrite(file, buf_ptr, buf_len, offset) => {
                operations::write_at_op(file, offset, buf_ptr, buf_len)
            }

            #[cfg(feature = "fallback_thread_pool")]
            Self::CloseFile(file) => {
                operations::close_file_op(file);

                Ok(0)
            }

            #[cfg(feature = "fallback_thread_pool")]
            Self::CloseSocket(socket) => {
                operations::close_socket_op(socket);

                Ok(0)
            }

            #[cfg(feature = "fallback_thread_pool")]
            Self::Rename(from, to) => operations::rename_op(from, to),

            #[cfg(feature = "fallback_thread_pool")]
            Self::CreateDir(path_ptr, mode) => operations::mkdir_op(path_ptr, mode),

            #[cfg(feature = "fallback_thread_pool")]
            Self::RemoveDir(path_ptr) => operations::rmdir_op(path_ptr),

            #[cfg(feature = "fallback_thread_pool")]
            Self::RemoveFile(path_ptr) => operations::unlink_op(path_ptr),
        }
    }

    /// Checks if the I/O call represented by this `IoCall` is read-pollable.
    pub(crate) const fn is_recv_pollable(&self) -> bool {
        matches!(
            self,
            Self::PollRecv(_)
                | Self::PollRecvWithDeadline(_, _)
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
                | Self::PollSendWithDeadline(_, _)
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
                Self::PollRecvWithDeadline(_, deadline) => Some(&mut **deadline),
                Self::PollSendWithDeadline(_, deadline) => Some(&mut **deadline),
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
    #[allow(
        clippy::unnecessary_wraps,
        reason = "With `fallback_thread_pool` feature it can return None."
    )]
    pub(crate) const fn raw_socket(&self) -> Option<RawSocket> {
        #[allow(clippy::match_same_arms, reason = "It is more readable this way.")]
        match self {
            Self::Accept(raw_socket, _, _) => Some(*raw_socket),
            Self::AcceptWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            Self::Connect(raw_socket, _, _) => Some(*raw_socket),
            Self::ConnectWithDeadline(raw_socket, _, _, _) => Some(*raw_socket),
            Self::PollRecv(raw_socket) => Some(*raw_socket),
            Self::PollRecvWithDeadline(raw_socket, _) => Some(*raw_socket),
            Self::PollSend(raw_socket) => Some(*raw_socket),
            Self::PollSendWithDeadline(raw_socket, _) => Some(*raw_socket),
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
            #[cfg(feature = "fallback_thread_pool")]
            _ => None,
        }
    }

    /// Returns whether the I/O call must be retried after polling.
    pub(crate) const fn must_retry(&self) -> bool {
        match self {
            Self::Connect(..)
            | Self::ConnectWithDeadline(..)
            | Self::PollRecv(..)
            | Self::PollRecvWithDeadline(..)
            | Self::PollSend(..)
            | Self::PollSendWithDeadline(..) => false,

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

            #[cfg(feature = "fallback_thread_pool")]
            _ => unreachable!(),
        }
    }
}

unsafe impl Send for IoCall {}
