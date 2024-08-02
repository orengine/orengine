#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BindConfig {
    pub backlog_size: usize,
    pub reuse_address: bool,
    pub reuse_port: bool,
}

impl BindConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn backlog_size(mut self, backlog_size: usize) -> Self {
        self.backlog_size = backlog_size;
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
            reuse_address: true,
            reuse_port: true,
        }
    }
}
