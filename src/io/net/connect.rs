use std::future::Future;
use std::io::{Error, ErrorKind, Result};
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;
use crate::each_addr;

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsRawFd, RawFd, IntoRawFd, FromRawFd};
use crate::io::worker::{local_worker, IoWorker};
use crate::io::AsPath;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Connect<'fut> {
    fd: RawFd,
    addr: &'fut SockAddr,
    io_request: Option<IoRequest>
}

impl<'fut> Connect<'fut> {
    pub fn new(fd: RawFd, addr: &'fut SockAddr) -> Self {
        Self {
            fd,
            addr,
            io_request: None
        }
    }
}

impl<'fut> Future for Connect<'fut> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_io_request!((
            worker.connect(
                this.fd,
                this.addr.as_ptr(),
                this.addr.len(),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ()
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct ConnectWithDeadline<'fut> {
    fd: RawFd,
    addr: &'fut SockAddr,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>
}

impl<'fut> ConnectWithDeadline<'fut> {
    pub fn new(fd: RawFd, addr: &'fut SockAddr, deadline: Instant) -> Self {
        Self {
            fd,
            addr,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None
        }
    }
}

impl<'fut> Future for ConnectWithDeadline<'fut> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        #[allow(unused)]
        let ret;

        poll_for_time_bounded_io_request!((
            worker.connect(
                this.fd,
                this.addr.as_ptr(),
                this.addr.len(),
                this.io_request.as_mut().unwrap_unchecked()
            ),
            ()
        ));
    }
}

pub trait AsyncConnectStream: Sized + AsRawFd {
    async fn new_ip4() -> Result<Self>;
    async fn new_ip6() -> Result<Self>;

    #[inline(always)]
    async fn new_for_addr(addr: &SocketAddr) -> Result<Self> {
        match addr {
            SocketAddr::V4(_) => Self::new_ip4().await,
            SocketAddr::V6(_) => Self::new_ip6().await,
        }
    }

    #[inline(always)]
    async fn connect<A: ToSocketAddrs>(addr: A) -> Result<Self> {
        each_addr!(
            &addr,
            async move |addr: SocketAddr| -> Result<Self> {
                let stream = Self::new_for_addr(&addr).await?;
                Connect::new(stream.as_raw_fd(), &SockAddr::from(addr)).await?;

                Ok(stream)
            }
        )
    }

    #[inline(always)]
    async fn connect_with_deadline<A: ToSocketAddrs>(addr: A, deadline: Instant) -> Result<Self> {
        each_addr!(
            &addr,
            async move |addr: SocketAddr| -> Result<Self> {
                let stream = Self::new_for_addr(&addr).await?;
                ConnectWithDeadline::new(stream.as_raw_fd(), &SockAddr::from(addr), deadline).await?;

                Ok(stream)
            }
        )
    }

    #[inline(always)]
    async fn connect_with_timeout<A: ToSocketAddrs>(addr: A, timeout: Duration) -> Result<Self> {
        Self::connect_with_deadline(addr, Instant::now() + timeout).await
    }
}

pub trait AsyncConnectDatagram<S: FromRawFd + Sized>: IntoRawFd + Sized {
    #[inline(always)]
    async fn connect<A: ToSocketAddrs>(self, addr: A) -> Result<S> {
        let new_datagram_socket_fd = self.into_raw_fd();
        each_addr!(
            &addr,
            async move |addr: SocketAddr| -> Result<S> {
                Connect::new(new_datagram_socket_fd, &SockAddr::from(addr)).await?;
                Ok(unsafe { S::from_raw_fd(new_datagram_socket_fd) })
            }
        )
    }

    #[inline(always)]
    async fn connect_with_deadline<A: ToSocketAddrs>(self, addr: A, deadline: Instant) -> Result<S> {
        let new_datagram_socket_fd = self.into_raw_fd();
        each_addr!(
            &addr,
            async move |addr: SocketAddr| -> Result<S> {
                ConnectWithDeadline::new(new_datagram_socket_fd, &SockAddr::from(addr), deadline).await?;
                Ok(unsafe { S::from_raw_fd(new_datagram_socket_fd) })
            }
        )
    }

    #[inline(always)]
    async fn connect_with_timeout<A: ToSocketAddrs>(self, addr: A, timeout: Duration) -> Result<S> {
        self.connect_with_deadline(addr, Instant::now() + timeout).await
    }
}

pub trait AsyncConnectStreamUnix: Sized + AsRawFd {
    fn unbound() -> Result<Self>;

    #[inline(always)]
    async fn connect<P: AsPath>(path: P) -> Result<Self> {
        let stream = Self::unbound()?;
        Connect::new(stream.as_raw_fd(), &SockAddr::unix(path)?).await?;

        Ok(stream)
    }

    #[inline(always)]
    async fn connect_with_deadline<P: AsPath>(path: P, deadline: Instant) -> Result<Self> {
        let stream = Self::unbound()?;
        ConnectWithDeadline::new(stream.as_raw_fd(), &SockAddr::unix(path)?, deadline).await?;

        Ok(stream)
    }

    #[inline(always)]
    async fn connect_with_timeout<P: AsPath>(path: P, timeout: Duration) -> Result<Self> {
        Self::connect_with_deadline(path, Instant::now() + timeout).await
    }

    #[inline(always)]
    async fn connect_addr(addr: &std::os::unix::net::SocketAddr) -> Result<Self> {
        let stream = Self::unbound()?;
        match addr.as_pathname() {
            Some(path) => {
                Connect::new(stream.as_raw_fd(), &SockAddr::unix(path)?).await?;
                Ok(stream)
            }
            None => Err(Error::new(ErrorKind::InvalidInput, "Invalid socket address")),
        }
    }

    #[inline(always)]
    async fn connect_addr_with_deadline(addr: &std::os::unix::net::SocketAddr, deadline: Instant) -> Result<Self> {
        let stream = Self::unbound()?;
        match addr.as_pathname() {
            Some(path) => {
                ConnectWithDeadline::new(stream.as_raw_fd(), &SockAddr::unix(path)?, deadline).await?;
                Ok(stream)
            }
            None => Err(Error::new(ErrorKind::InvalidInput, "Invalid socket address")),
        }
    }

    #[inline(always)]
    async fn connect_addr_with_timeout(addr: &std::os::unix::net::SocketAddr, timeout: Duration) -> Result<Self> {
        Self::connect_addr_with_deadline(addr, Instant::now() + timeout).await
    }
}

pub trait AsyncConnectDatagramUnix<S: FromRawFd + Sized>: IntoRawFd + Sized {
    #[inline(always)]
    async fn connect<P: AsPath>(self, path: P) -> Result<S> {
        let new_datagram_socket_fd = self.into_raw_fd();
        Connect::new(new_datagram_socket_fd, &SockAddr::unix(path)?).await?;

        Ok(unsafe { S::from_raw_fd(new_datagram_socket_fd) })
    }

    #[inline(always)]
    async fn connect_with_deadline<P: AsPath>(self, path: P, deadline: Instant) -> Result<S> {
        let new_datagram_socket_fd = self.into_raw_fd();
        ConnectWithDeadline::new(new_datagram_socket_fd, &SockAddr::unix(path)?, deadline).await?;

        Ok(unsafe { S::from_raw_fd(new_datagram_socket_fd) })
    }

    #[inline(always)]
    async fn connect_with_timeout<P: AsPath>(self, path: P, timeout: Duration) -> Result<S> {
        self.connect_with_deadline(path, Instant::now() + timeout).await
    }

    #[inline(always)]
    async fn connect_addr(self, addr: &std::os::unix::net::SocketAddr) -> Result<S> {
        let new_datagram_socket_fd = self.into_raw_fd();
        match addr.as_pathname() {
            Some(path) => {
                Connect::new(
                    new_datagram_socket_fd,
                    &SockAddr::unix(path)?
                ).await?;

                Ok(unsafe { S::from_raw_fd(new_datagram_socket_fd) })
            }
            None => Err(Error::new(ErrorKind::InvalidInput, "Invalid socket address")),
        }
    }

    #[inline(always)]
    async fn connect_addr_with_deadline(self, addr: &std::os::unix::net::SocketAddr, deadline: Instant) -> Result<S> {
        let new_datagram_socket_fd = self.into_raw_fd();
        match addr.as_pathname() {
            Some(path) => {
                ConnectWithDeadline::new(
                    new_datagram_socket_fd,
                    &SockAddr::unix(path)?,
                    deadline
                ).await?;

                Ok(unsafe { S::from_raw_fd(new_datagram_socket_fd) })
            }
            None => Err(Error::new(ErrorKind::InvalidInput, "Invalid socket address")),
        }
    }

    #[inline(always)]
    async fn connect_addr_with_timeout(self, addr: &std::os::unix::net::SocketAddr, timeout: Duration) -> Result<S> {
        self.connect_addr_with_deadline(addr, Instant::now() + timeout).await
    }
}