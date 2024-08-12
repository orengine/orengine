// TODO maybe async via worker pool?

use std::io::{Error, ErrorKind, Result};
use std::net::{ToSocketAddrs};
use crate::net::creators_of_sockets::new_unix_socket_datagram;
use crate::io::sys::FromRawFd;
use crate::io::AsPath;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BindConfig {
    pub backlog_size: isize,
    pub only_v6: bool,
    pub reuse_address: bool,
    pub reuse_port: bool,
}

impl BindConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn backlog_size(mut self, backlog_size: isize) -> Self {
        self.backlog_size = backlog_size;
        self
    }

    pub fn only_v6(mut self, only_v6: bool) -> Self {
        self.only_v6 = only_v6;
        self
    }

    pub fn reuse_address(mut self, reuse_address: bool) -> Self {
        self.reuse_address = reuse_address;
        self
    }

    pub fn reuse_port(mut self, reuse_port: bool) -> Self {
        self.reuse_port = reuse_port;
        self
    }
}

impl Default for BindConfig {
    fn default() -> Self {
        Self {
            backlog_size: 1024,
            only_v6: false,
            reuse_address: true,
            reuse_port: true,
        }
    }
}

pub trait AsyncBind: Sized {
    async fn bind_with_config<A: ToSocketAddrs>(addrs: A, config: &BindConfig) -> Result<Self>;

    #[inline(always)]
    async fn bind<A: ToSocketAddrs>(addrs: A) -> Result<Self> {
        Self::bind_with_config(addrs, &BindConfig::default()).await
    }
}

pub trait AsyncBindUnix: Sized + FromRawFd {
    #[inline(always)]
    async fn unbound() -> Result<Self> {
        Ok(unsafe {
            Self::from_raw_fd(new_unix_socket_datagram().await?)
        })
    }

    async fn bind_with_backlog_size<P: AsPath>(path: P, backlog_size: isize) -> Result<Self>;

    #[inline(always)]
    async fn bind<P: AsPath>(path: P) -> Result<Self> {
        Self::bind_with_backlog_size(path, -1).await
    }

    async fn bind_addr_with_backlog_size(addr: std::os::unix::net::SocketAddr, backlog_size: isize) -> Result<Self> {
        match addr.as_pathname() {
            Some(path) => Self::bind_with_backlog_size(path, backlog_size).await,
            None => Err(Error::new(ErrorKind::InvalidInput, "Invalid unix socket address")),
        }
    }

    #[inline(always)]
    async fn bind_addr(addr: std::os::unix::net::SocketAddr) -> Result<Self> {
        Self::bind_addr_with_backlog_size(addr, -1).await
    }
}

