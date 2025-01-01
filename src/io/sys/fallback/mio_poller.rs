use crate::io::io_request_data::IoRequestData;
use crate::io::sys::RawSocket;
use crate::io::time_bounded_io_task::TimeBoundedIoTask;
use mio::event::Source;
use mio::{Events, Interest, Poll, Token};
use std::collections::BTreeSet;
use std::time::Instant;
use std::{io, ptr};

/// `MioPoller` is a wrapper around `mio::Poll` that is used to poll for events and poll-timeouts.
pub(crate) struct MioPoller {
    poll: Poll,
    events: Events,
    time_bounded_io_task_queue: BTreeSet<TimeBoundedIoTask>,
}

impl MioPoller {
    /// Creates a new `MioPoller` instance.
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            poll: Poll::new()?,
            events: Events::with_capacity(128),
            time_bounded_io_task_queue: BTreeSet::new(),
        })
    }

    /// Registers a new interest (read/write) for a given socket and associates it with a payload.
    pub(crate) fn register(
        &mut self,
        raw_socket: RawSocket,
        interest: Interest,
        request_ptr: *mut IoRequestData,
    ) -> io::Result<()> {
        let registry = self.poll.registry();

        #[cfg(unix)]
        {
            registry.register(
                &mut mio::unix::SourceFd(&raw_socket),
                Token(request_ptr as usize),
                interest,
            )
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::FromRawSocket;

            let mut mio_stream = unsafe { mio::net::TcpStream::from_raw_socket(raw_socket) }; // It can be not only stream
            registry.register(&mut mio_stream, Token(request_ptr as usize), interest)?;
            std::mem::forget(mio_stream);

            Ok(())
        }
    }

    /// Deregister a socket from the poller.
    pub(crate) fn deregister(&mut self, raw_socket: RawSocket) -> io::Result<()> {
        let registry = self.poll.registry();

        #[cfg(unix)]
        {
            registry.deregister(&mut mio::unix::SourceFd(&raw_socket))
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::FromRawSocket;

            let mut mio_stream = unsafe { mio::net::TcpStream::from_raw_socket(raw_socket) }; // It can be not only stream
            registry.deregister(&mut mio_stream)?;
            std::mem::forget(mio_stream);

            Ok(())
        }
    }

    /// Checks for timed out requests and removes them from the queue.
    ///
    /// It saves the timed out requests to the provided vector.
    pub(crate) fn check_deadlines(&mut self, timed_out_requests: &mut Vec<IoRequestData>) {
        if self.time_bounded_io_task_queue.is_empty() {
            return;
        }

        let now = Instant::now();

        while let Some(time_bounded_io_task) = self.time_bounded_io_task_queue.pop_first() {
            if time_bounded_io_task.deadline() <= now {
                self.deregister(time_bounded_io_task.raw_socket()).unwrap();
                let io_request_ptr = time_bounded_io_task.user_data() as *mut IoRequestData;
                timed_out_requests.push(unsafe { ptr::read(io_request_ptr) });
            } else {
                self.time_bounded_io_task_queue.insert(time_bounded_io_task);

                break;
            }
        }
    }

    /// Polls for events and invokes the provided callback for each event.
    pub(crate) fn poll<F>(
        &mut self,
        timeout: Option<std::time::Duration>,
        requests: &mut Vec<Result<IoRequestData, IoRequestData>>,
    ) -> io::Result<()> {
        self.poll.poll(&mut self.events, timeout)?;

        for event in &self.events {
            let io_request_ptr = event.token().0 as *mut IoRequestData;
            let io_request = unsafe { &mut *io_request_ptr };

            self.deregister(io_request.raw_socket())?;

            requests.push(
                if event.is_error() || (!event.is_readable() && !event.is_writable()) {
                    Err(unsafe { ptr::read(io_request) })
                } else {
                    Ok(unsafe { ptr::read(io_request) })
                },
            );
        }
        Ok(())
    }
}
