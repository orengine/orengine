use crate::io::io_request_data::IoRequestDataPtr;
use crate::io::sys::fallback::with_thread_pool::io_call::IoCall;
use crate::io::sys::RawSocket;
use mio::{Events, Interest, Poll, Token};
use std::cell::UnsafeCell;
use std::{io, ptr};

/// `MioPoller` is a wrapper around `mio::Poll` that is used to poll for events and poll-timeouts.
pub(crate) struct MioPoller {
    poll: Poll,
    events: UnsafeCell<Events>,
    request_slots: Vec<*mut (IoCall, IoRequestDataPtr)>,
}

impl MioPoller {
    /// Creates a new `MioPoller` instance.
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            poll: Poll::new()?,
            events: UnsafeCell::new(Events::with_capacity(128)),
            request_slots: Vec::new(),
        })
    }

    /// Allocates a request's slot or gets it from the pool, writes the request to the slot and returns the pointer.
    fn write_request_and_get_ptr(
        &mut self,
        request: (IoCall, IoRequestDataPtr),
    ) -> *mut (IoCall, IoRequestDataPtr) {
        if let Some(slot) = self.request_slots.pop() {
            unsafe { ptr::write(slot, request) };
            slot
        } else {
            Box::into_raw(Box::new(request))
        }
    }

    /// Releases a request's slot and returns an associated request via reading.
    fn release_request_slot(
        &mut self,
        request_ptr: *mut (IoCall, IoRequestDataPtr),
    ) -> (IoCall, IoRequestDataPtr) {
        self.request_slots.push(request_ptr);

        unsafe { ptr::read(request_ptr) }
    }

    /// Registers a new interest (read/write) for a given socket and associates it with a payload.
    ///
    /// Returns a slot pointer that is used to deregister.
    pub(crate) fn register(
        &mut self,
        interest: Interest,
        request: (IoCall, IoRequestDataPtr),
    ) -> *mut (IoCall, IoRequestDataPtr) {
        let raw_socket = request.0.raw_socket().unwrap();
        let request_ptr = self.write_request_and_get_ptr(request);
        let registry = self.poll.registry();

        #[cfg(unix)]
        {
            registry
                .register(
                    &mut mio::unix::SourceFd(&raw_socket),
                    Token(request_ptr as usize),
                    interest,
                )
                .unwrap();
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::FromRawSocket;

            let mut mio_stream = unsafe { mio::net::TcpStream::from_raw_socket(raw_socket) }; // It can be not only stream

            registry
                .register(&mut mio_stream, Token(request_ptr as usize), interest)
                .unwrap();

            std::mem::forget(mio_stream);
        }

        request_ptr
    }

    /// Deregister a socket from the poller. Slot will be released.
    pub(crate) fn deregister(
        &mut self,
        raw_socket: RawSocket,
        slot: *mut (IoCall, IoRequestDataPtr),
    ) -> io::Result<()> {
        self.release_request_slot(slot);

        self.deregister_(raw_socket)
    }

    /// Deregister a socket from the poller.
    fn deregister_(&mut self, raw_socket: RawSocket) -> io::Result<()> {
        // Here not slot's leaks, because it called after slot's release

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

    /// Polls for events and invokes the provided callback for each event.
    pub(crate) fn poll(
        &mut self,
        timeout: Option<std::time::Duration>,
        requests: &mut Vec<Result<(IoCall, IoRequestDataPtr), IoRequestDataPtr>>,
    ) -> io::Result<()> {
        let events = unsafe { &mut *self.events.get() };

        self.poll.poll(events, timeout)?;

        for event in events.iter() {
            let io_request_ptr = event.token().0 as *mut (IoCall, IoRequestDataPtr);
            let io_request = self.release_request_slot(io_request_ptr);

            self.deregister_(io_request.0.raw_socket().unwrap())?;

            requests.push(
                if event.is_error() || (!event.is_readable() && !event.is_writable()) {
                    Err(io_request.1)
                } else {
                    Ok(io_request)
                },
            );
        }
        Ok(())
    }
}
