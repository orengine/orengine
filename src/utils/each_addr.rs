use std::io;

/// `EachAddrRes` is used to return results from `each_addr`. 
/// 
/// When successful, it contains an `Ok(R)` variant.
/// 
/// It contains a last error in an `Err` variant.
/// 
/// By default, it contains `None`.
pub(crate) enum EachAddrRes<R> {
    Ok(R),
    Err(io::Error),
    None
}

/// `each_addr` is a macro that will iterate over the addresses in `addrs` and call `f` for each one
/// before first successful connection or the last address.
/// 
/// # The difference with [`each_addr_sync`]
/// 
/// `$f` will be called in an async context.
#[macro_export]
macro_rules! each_addr {
    ($addrs: expr, $f: expr) => {
        {
            use $crate::utils::each_addr::EachAddrRes;

            let addrs = match std::net::ToSocketAddrs::to_socket_addrs($addrs) {
                Ok(addrs) => addrs,
                Err(e) => return Err(e),
            };

            let mut res = EachAddrRes::None;
            for addr in addrs {
                match $f(addr).await {
                    Ok(connect_res) => {
                        res = EachAddrRes::Ok(connect_res);
                        break;
                    },
                    Err(error) => {
                        res = EachAddrRes::Err(error);
                    },
                }
            }

            match res {
                EachAddrRes::Ok(res) => std::io::Result::Ok(res),
                EachAddrRes::Err(err) => std::io::Result::Err(err),
                EachAddrRes::None => std::io::Result::Err(
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "could not resolve to any addresses")
                ),
            }
        }
    }
}

/// `each_addr_sync` is a macro that will iterate over the addresses in `addrs` and call
/// `f` for each one before first successful connection or the last address.
/// 
/// # The difference with [`each_addr`]
/// 
/// `$f` will be called in a sync context.
#[macro_export]
macro_rules! each_addr_sync {
    ($addrs: expr, $f: expr) => {
        {
            use $crate::utils::each_addr::EachAddrRes;

            let addrs = match std::net::ToSocketAddrs::to_socket_addrs($addrs) {
                Ok(addrs) => addrs,
                Err(e) => return Err(e),
            };

            let mut res = EachAddrRes::None;
            for addr in addrs {
                match $f(addr) {
                    Ok(connect_res) => {
                        res = EachAddrRes::Ok(connect_res);
                        break;
                    },
                    Err(error) => {
                        res = EachAddrRes::Err(error);
                    },
                }
            }

            match res {
                EachAddrRes::Ok(res) => std::io::Result::Ok(res),
                EachAddrRes::Err(err) => std::io::Result::Err(err),
                EachAddrRes::None => std::io::Result::Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput, "could not resolve to any addresses")
                ),
            }
        }
    }
}