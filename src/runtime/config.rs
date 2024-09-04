pub const DEFAULT_BUF_LEN: usize = 4096;

pub struct Config {
    pub buffer_len: usize,
    pub number_of_entries: u32
}

impl Config {
    pub const fn default() -> Self {
        Self {
            buffer_len: DEFAULT_BUF_LEN,
            number_of_entries: 1024
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_buf_len(&mut self, buf_len: usize) {
        self.buffer_len = buf_len;
    }

    pub fn set_number_of_entries(&mut self, number_of_entries: u32) {
        self.number_of_entries = number_of_entries;
    }
}