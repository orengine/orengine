use std::io::Result;
use std::net::{SocketAddr, ToSocketAddrs};
use socket2::SockRef;
use crate::each_addr;
use crate::io::sys::{RawFd, BorrowedFd, FromRawFd};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ReusePort {
    Disabled,
    Default,
    CPU
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BindConfig {
    pub backlog_size: isize,
    pub only_v6: bool,
    pub reuse_address: bool,
    pub reuse_port: ReusePort
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

    pub fn reuse_port(mut self, reuse_port: ReusePort) -> Self {
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
            reuse_port: ReusePort::Default
        }
    }
}

pub trait AsyncBind: Sized + FromRawFd {
    async fn new_socket(addr: &SocketAddr) -> Result<RawFd>;
    fn bind_and_listen_if_needed(
        sock_ref: SockRef,
        addr: SocketAddr,
        config: &BindConfig
    ) -> Result<()>;

    async fn bind_with_config<A: ToSocketAddrs>(addrs: A, config: &BindConfig) -> Result<Self> {
        each_addr!(&addrs, async move |addr| {
            let fd = Self::new_socket(&addr).await?;
            let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
            let socket_ref = socket2::SockRef::from(&borrowed_fd);

            if config.only_v6 {
                socket_ref.set_only_v6(true)?;
            }

            if config.reuse_address {
                socket_ref.set_reuse_address(true)?;
            }

            match config.reuse_port {
                ReusePort::Disabled => { Self::bind_and_listen_if_needed(socket_ref, addr, config)?; }
                ReusePort::Default => {
                    socket_ref.set_reuse_port(true)?;
                    Self::bind_and_listen_if_needed(socket_ref, addr, config)?;
                }
                ReusePort::CPU => {
                    socket_ref.set_reuse_port(true)?;
                    Self::bind_and_listen_if_needed(socket_ref, addr, config)?;

                    if cfg!(target_os = "linux") {
                        use nix::libc::{self, BPF_LD, BPF_W, BPF_ABS, BPF_RET, SKF_AD_OFF, SKF_AD_CPU};

                        // [
                        //     {BPF_LD | BPF_W | BPF_ABS, 0, 0, SKF_AD_OFF + SKF_AD_CPU},
                        //     {BPF_RET | BPF_A, 0, 0, 0}
                        // ]
                        let mut code = [
                            libc::sock_filter{
                                code: (BPF_LD | BPF_W | BPF_ABS) as _,
                                jt: 0,
                                jf: 0,
                                k: (SKF_AD_OFF + SKF_AD_CPU) as _
                            },
                            libc::sock_filter{
                                code: BPF_RET as _,
                                jt: 0,
                                jf: 0,
                                k: 0
                            }
                        ];
                        let p = libc::sock_fprog{
                            len: 2,
                            filter: code.as_mut_ptr()
                        };

                        let res = unsafe {
                            libc::setsockopt(
                                fd as _,
                                libc::SOL_SOCKET,
                                libc::SO_ATTACH_REUSEPORT_CBPF,
                                &p as *const _ as _,
                                size_of::<libc::sock_fprog>() as _
                            )
                        };
                        if res < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                    }
                }
            }

            Ok(unsafe { Self::from_raw_fd(fd) })
        })
    }

    #[inline(always)]
    async fn bind<A: ToSocketAddrs>(addrs: A) -> Result<Self> {
        Self::bind_with_config(addrs, &BindConfig::default()).await
    }
}

