use std::io;

pub(crate) enum EachAddrRes<R> {
    Ok(R),
    Err(std::io::Error),
    None
}

#[macro_export]
macro_rules! each_addr {
    ($addrs: expr, $f: expr) => {
        {
            use crate::utils::each_addr::EachAddrRes;

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
                EachAddrRes::Ok(res) => io::Result::Ok(res),
                EachAddrRes::Err(err) => io::Result::Err(err),
                EachAddrRes::None => io::Result::Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "could not resolve to any addresses")),
            }
        }
    }
}

#[macro_export]
macro_rules! each_addr_sync {
    ($addrs: expr, $f: expr) => {
        {
            use crate::utils::each_addr::EachAddrRes;

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
                EachAddrRes::Ok(res) => io::Result::Ok(res),
                EachAddrRes::Err(err) => io::Result::Err(err),
                EachAddrRes::None => io::Result::Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "could not resolve to any addresses")),
            }
        }
    }
}