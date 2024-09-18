#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ReusePort {
    Disabled,
    Default,
    CPU,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BindConfig {
    pub backlog_size: isize,
    pub only_v6: bool,
    pub reuse_address: bool,
    pub reuse_port: ReusePort,
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
            reuse_port: ReusePort::Default,
        }
    }
}
