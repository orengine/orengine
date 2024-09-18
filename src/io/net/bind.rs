use std::io::Result;
use std::net::{SocketAddr, ToSocketAddrs};

use socket2::SockRef;

use crate::each_addr;
use crate::io::sys::{BorrowedFd, FromRawFd, RawFd};
use crate::net::{BindConfig, ReusePort};

pub trait AsyncBind: Sized + FromRawFd {
    async fn new_socket(addr: &SocketAddr) -> Result<RawFd>;
    fn bind_and_listen_if_needed(
        sock_ref: SockRef,
        addr: SocketAddr,
        config: &BindConfig,
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
                ReusePort::Disabled => {
                    Self::bind_and_listen_if_needed(socket_ref, addr, config)?;
                }
                ReusePort::Default => {
                    socket_ref.set_reuse_port(true)?;
                    Self::bind_and_listen_if_needed(socket_ref, addr, config)?;
                }
                ReusePort::CPU => {
                    socket_ref.set_reuse_port(true)?;
                    Self::bind_and_listen_if_needed(socket_ref, addr, config)?;

                    if cfg!(target_os = "linux") {
                        use nix::libc::{
                            self, __u32, BPF_ABS, BPF_LD, BPF_RET, BPF_W, SKF_AD_CPU, SKF_AD_OFF,
                        };
                        const BPF_A: __u32 = 0x10;

                        // [
                        //     {BPF_LD | BPF_W | BPF_ABS, 0, 0, SKF_AD_OFF + SKF_AD_CPU},
                        //     {BPF_RET | BPF_A, 0, 0, 0}
                        // ]
                        let mut code = [
                            libc::sock_filter {
                                code: (BPF_LD | BPF_W | BPF_ABS) as _,
                                jt: 0,
                                jf: 0,
                                k: (SKF_AD_OFF + SKF_AD_CPU) as _,
                            },
                            libc::sock_filter {
                                code: (BPF_RET | BPF_A) as _,
                                jt: 0,
                                jf: 0,
                                k: 0,
                            },
                        ];
                        let p = libc::sock_fprog {
                            len: 2,
                            filter: code.as_mut_ptr(),
                        };

                        let res = unsafe {
                            libc::setsockopt(
                                fd as _,
                                libc::SOL_SOCKET,
                                libc::SO_ATTACH_REUSEPORT_CBPF,
                                &p as *const _ as _,
                                size_of::<libc::sock_fprog>() as _,
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
