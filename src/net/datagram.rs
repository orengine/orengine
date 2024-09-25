use std::net::{Ipv4Addr, Ipv6Addr};
use crate::io::{AsyncConnectDatagram, AsyncPeekFrom, AsyncRecvFrom, AsyncSendTo, AsyncBind};
use crate::net::connected_datagram::ConnectedDatagram;
use crate::net::Socket;

pub trait Datagram:
Socket + AsyncConnectDatagram<Self::ConnectedDatagram> + AsyncRecvFrom +
AsyncPeekFrom + AsyncSendTo + AsyncBind {
    type ConnectedDatagram: ConnectedDatagram;

    #[inline(always)]
    fn set_broadcast(&self, broadcast: bool) -> std::io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_broadcast(broadcast)
    }

    #[inline(always)]
    fn broadcast(&self) -> std::io::Result<bool> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.broadcast()
    }

    #[inline(always)]
    fn set_multicast_loop_v4(&self, multicast_loop_v4: bool) -> std::io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_multicast_loop_v4(multicast_loop_v4)
    }

    #[inline(always)]
    fn multicast_loop_v4(&self) -> std::io::Result<bool> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.multicast_loop_v4()
    }

    #[inline(always)]
    fn set_multicast_ttl_v4(&self, multicast_ttl_v4: u32) -> std::io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_multicast_ttl_v4(multicast_ttl_v4)
    }

    #[inline(always)]
    fn multicast_ttl_v4(&self) -> std::io::Result<u32> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.multicast_ttl_v4()
    }

    #[inline(always)]
    fn set_multicast_loop_v6(&self, multicast_loop_v6: bool) -> std::io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_multicast_loop_v6(multicast_loop_v6)
    }

    #[inline(always)]
    fn multicast_loop_v6(&self) -> std::io::Result<bool> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.multicast_loop_v6()
    }

    /// Executes an operation of the `IP_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// address of the local interface with which the system should join the
    /// multicast group. If it's equal to `INADDR_ANY` then an appropriate
    /// interface is chosen by the system.
    #[inline(always)]
    fn join_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr) -> std::io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.join_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// index of the interface to join/leave (or 0 to indicate any interface).
    #[inline(always)]
    fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> std::io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.join_multicast_v6(multiaddr, interface)
    }

    /// Executes an operation of the `IP_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`Socket::join_multicast_v4`].
    #[inline(always)]
    fn leave_multicast_v4(&self, multiaddr: &Ipv4Addr, interface: &Ipv4Addr) -> std::io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.leave_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`Socket::join_multicast_v6`].
    #[inline(always)]
    fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> std::io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.leave_multicast_v6(multiaddr, interface)
    }
}