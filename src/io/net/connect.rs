use std::future::Future;
use std::io::Result;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use socket2::SockAddr;

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::Fd;
use crate::io::worker::{local_worker, IoWorker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Connect<S: From<Fd>> {
    fd: Fd,
    addr: SockAddr,
    io_request: Option<IoRequest>,
    phantom_data: PhantomData<S>,
}

impl<S: From<Fd>> Connect<S> {
    pub fn new(fd: Fd, addr: SocketAddr) -> Self {
        Self {
            fd,
            addr: SockAddr::from(addr),
            io_request: None,
            phantom_data: PhantomData,
        }
    }
}

impl<S: From<Fd>> Future for Connect<S> {
    type Output = Result<S>;

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
                this.io_request.as_ref().unwrap_unchecked()
            ),
            S::from(this.fd)
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct ConnectWithTimeout<S: From<Fd>> {
    fd: Fd,
    addr: SockAddr,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
    phantom_data: PhantomData<S>,
}

impl<S: From<Fd>> ConnectWithTimeout<S> {
    pub fn new(fd: Fd, addr: SocketAddr, deadline: Instant) -> Self {
        Self {
            fd,
            addr: SockAddr::from(addr),
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
            phantom_data: PhantomData,
        }
    }
}

impl<S: From<Fd>> Future for ConnectWithTimeout<S> {
    type Output = Result<S>;

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
                this.io_request.as_ref().unwrap_unchecked()
            ),
            S::from(this.fd)
        ));
    }
}

#[macro_export]
macro_rules! generate_connect_from_new_socket {
    ($new_socket: expr) => {
        #[inline(always)]
        pub async fn connect<A: ToSocketAddrs>(addrs: A) -> io::Result<Self> {
            each_addr!(&addrs, async move |addr: SocketAddr| -> io::Result<Self> {
                let socket = $new_socket(addr)?;
                Connect::new(socket.into_raw_fd(), addr).await
            })
        }

        #[inline(always)]
        pub async fn connect_with_deadline<A: ToSocketAddrs>(
            addrs: A,
            deadline: Instant,
        ) -> io::Result<Self> {
            each_addr!(
                &addrs,
                async move |addr: SocketAddr| -> io::Result<Stream> {
                    let socket = $new_socket(addr)?;
                    ConnectWithTimeout::new(socket.into_raw_fd(), addr, deadline).await
                }
            )
        }

        #[inline(always)]
        pub async fn connect_with_timeout<A: ToSocketAddrs>(
            addrs: A,
            timeout: Duration,
        ) -> io::Result<Self> {
            Self::connect_with_deadline(addrs, Instant::now() + timeout).await
        }
    };
}
