#[macro_export]
macro_rules! each_addr {
    ($addrs: expr, $f: expr) => {
        {
            let addrs = match std::net::ToSocketAddrs::to_socket_addrs($addrs) {
                Ok(addrs) => addrs,
                Err(e) => return Err(e),
            };

            let mut last_err = None;
            for addr in addrs {
                match $f(addr).await {
                    Ok(res) => return Ok(res),
                    Err(error) => last_err = Some(error),
                }
            }

            Err(last_err.unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "could not resolve to any addresses")
            }))
        }
    }
}

#[macro_export]
macro_rules! each_addr_sync {
    ($addrs: expr, $f: expr) => {
        {
            let addrs = match std::net::ToSocketAddrs::to_socket_addrs($addrs) {
                Ok(addrs) => addrs,
                Err(e) => return Err(e),
            };

            let mut last_err = None;
            for addr in addrs {
                match $f(addr) {
                    Ok(res) => return Ok(res),
                    Err(error) => last_err = Some(error),
                }
            }

            Err(last_err.unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "could not resolve to any addresses")
            }))
        }
    }
}