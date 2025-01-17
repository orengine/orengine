use crate::net::addr::to_sock_addrs::ToSockAddrs;
use crate::net::addr::{FromSockAddr, IntoSockAddr};
use std::future::Future;
use std::io;

/// `EachAddrRes` is used to return results from [`each_addr`].
///
/// When successful, it contains an `Ok(R)` variant.
///
/// It contains a last error in an `Err` variant.
///
/// By default, it contains `None`.
pub(crate) enum EachAddrRes<R> {
    Ok(R),
    Err(io::Error),
    None,
}

/// Iterate over the addresses in `addrs` and call `f` for each one
/// before first successful result or the last address.
#[allow(
    clippy::future_not_send,
    reason = "It is not Send only when Res or Fur or F is not Send, it is right."
)]
pub(crate) async fn each_addr<Addr: FromSockAddr + IntoSockAddr, Res, Fut, F>(
    addrs: impl ToSockAddrs<Addr>,
    f: F,
) -> io::Result<Res>
where
    Fut: Future<Output = io::Result<Res>>,
    F: Fn(Addr) -> Fut,
{
    let addrs = match ToSockAddrs::to_sock_addrs(&addrs) {
        Ok(addrs) => addrs,
        Err(e) => return Err(e),
    };
    let mut res = EachAddrRes::None;
    for addr in addrs {
        match f(addr).await {
            Ok(connect_res) => {
                res = EachAddrRes::Ok(connect_res);
                break;
            }
            Err(error) => {
                res = EachAddrRes::Err(error);
            }
        }
    }
    match res {
        EachAddrRes::Ok(res) => Ok(res),
        EachAddrRes::Err(err) => Err(err),
        EachAddrRes::None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "could not resolve to any addresses",
        )),
    }
}
