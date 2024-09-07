use crate::io::IoWorkerConfig;
use crate::messages::BUG;
use crate::utils::SpinLock;

struct ConfigStats {
    number_of_executors_with_enabled_io_worker_and_work_sharing: usize,
    number_of_executors_with_enabled_thread_pool_and_work_sharing: usize,
    number_of_executors_with_work_sharing_and_without_io_worker: usize,
    number_of_executors_with_work_sharing_and_without_thread_pool: usize
}

impl ConfigStats {
    const fn new() -> Self {
        Self {
            number_of_executors_with_enabled_io_worker_and_work_sharing: 0,
            number_of_executors_with_enabled_thread_pool_and_work_sharing: 0,
            number_of_executors_with_work_sharing_and_without_io_worker: 0,
            number_of_executors_with_work_sharing_and_without_thread_pool: 0
        }
    }
}

static GLOBAL_CONFIG_STATS: SpinLock<ConfigStats> = SpinLock::new(ConfigStats::new());

pub const DEFAULT_BUF_LEN: usize = 4096;

#[derive(Clone)]
pub(crate) struct ValidConfig {
    pub(crate) buffer_len: usize,
    pub(crate) io_worker_config: Option<IoWorkerConfig>,
    pub(crate) is_thread_pool_enabled: bool,
    pub(crate) is_work_sharing_enabled: bool
}

impl Drop for ValidConfig {
    fn drop(&mut self) {
        if self.is_work_sharing_enabled {
            let mut guard;
            if self.io_worker_config.is_some() {
                guard = Some(GLOBAL_CONFIG_STATS.lock());
                let global_config_stats = guard.as_mut().expect(BUG);

                global_config_stats.number_of_executors_with_enabled_io_worker_and_work_sharing -= 1;
            } else {
                guard = Some(GLOBAL_CONFIG_STATS.lock());
                let global_config_stats = guard.as_mut().expect(BUG);

                global_config_stats.number_of_executors_with_work_sharing_and_without_io_worker -= 1;
            }

            if self.is_thread_pool_enabled {
                if guard.is_none() {
                    guard = Some(GLOBAL_CONFIG_STATS.lock());
                }

                let global_config_stats = guard.as_mut().expect(BUG);
                global_config_stats.number_of_executors_with_enabled_thread_pool_and_work_sharing -= 1;
            } else {
                if guard.is_none() {
                    guard = Some(GLOBAL_CONFIG_STATS.lock());
                }

                let global_config_stats = guard.as_mut().expect(BUG);
                global_config_stats.number_of_executors_with_work_sharing_and_without_thread_pool -= 1;
            }
        }
    }
}

#[derive(Clone)]
pub struct Config {
    buffer_len: usize,
    io_worker_config: Option<IoWorkerConfig>,
    is_thread_pool_enabled: bool,
    is_work_sharing_enabled: bool
}

impl Config {
    pub const fn default() -> Self {
        Self {
            buffer_len: DEFAULT_BUF_LEN,
            io_worker_config: Some(IoWorkerConfig::default()),
            is_thread_pool_enabled: true,
            is_work_sharing_enabled: true
        }
    }

    pub const fn new() -> Self {
        Self::default()
    }

    pub const fn buffer_len(&self) -> usize {
        self.buffer_len
    }

    pub const fn set_buffer_len(mut self, buf_len: usize) -> Self {
        self.buffer_len = buf_len;

        self
    }

    pub const fn io_worker_config(&self) -> Option<IoWorkerConfig> {
        self.io_worker_config
    }

    pub const fn set_io_worker_config(
        mut self,
        io_worker_config: Option<IoWorkerConfig>
    ) -> Result<Self, &'static str> {
        match io_worker_config {
            Some(io_worker_config) => {
                if let Err(err) = io_worker_config.validate() {
                    return Err(err);
                }

                self.io_worker_config = Some(io_worker_config);
            },
            None => {
                self.io_worker_config = None;
            }
        }

        Ok(self)
    }

    pub const fn disable_io_worker(mut self) -> Self {
        self.io_worker_config = None;

        self
    }

    pub const fn is_thread_pool_enabled(&self) -> bool {
        self.is_thread_pool_enabled
    }

    pub const fn set_thread_pool_enabled(mut self, is_thread_pool_enabled: bool) -> Self {
        self.is_thread_pool_enabled = is_thread_pool_enabled;

        self
    }

    pub const fn is_work_sharing_enabled(&self) -> bool {
        self.is_work_sharing_enabled
    }

    pub const fn set_work_sharing_enabled(mut self, is_work_sharing_enabled: bool) -> Self {
        self.is_work_sharing_enabled = is_work_sharing_enabled;

        self
    }

    pub(crate) fn validate(self) -> ValidConfig {
        if self.is_work_sharing_enabled {
            let mut global_config_stats = GLOBAL_CONFIG_STATS.lock();

            match self.io_worker_config.is_some() {
                true => {
                    if global_config_stats.number_of_executors_with_work_sharing_and_without_io_worker != 0 {
                        panic!(
                            "An attempt to create an executor with work sharing and with an \
                            IO worker has failed because another executor was created with \
                            work sharing enabled and without an IO worker enabled. \
                            This is unacceptable because an executor who does not have an \
                            IO worker cannot take on a task that requires an IO worker."
                        );
                    }

                    global_config_stats.number_of_executors_with_enabled_io_worker_and_work_sharing += 1;
                },
                false => {
                    if !global_config_stats.number_of_executors_with_enabled_io_worker_and_work_sharing != 0 {
                        panic!(
                            "An attempt to create an executor with work sharing and without an \
                            IO worker has failed because another executor was created with \
                            an IO worker and work sharing enabled. \
                            This is unacceptable because an executor who does not have an \
                            IO worker cannot take on a task that requires an IO worker."
                        );
                    }

                    global_config_stats.number_of_executors_with_work_sharing_and_without_io_worker += 1;
                }
            }

            match self.is_thread_pool_enabled {
                true => {
                    if global_config_stats.number_of_executors_with_work_sharing_and_without_thread_pool != 0 {
                        panic!(
                            "An attempt to create an executor with work sharing and with a \
                            thread pool enabled has failed because another executor was created with \
                            work sharing enabled and without a thread pool enabled. \
                            This is unacceptable because an executor who does not have a \
                            thread pool cannot take on a task that requires a thread pool."
                        );
                    }

                    global_config_stats.number_of_executors_with_enabled_thread_pool_and_work_sharing += 1;
                },
                false => {
                    if !global_config_stats.number_of_executors_with_enabled_thread_pool_and_work_sharing != 0 {
                        panic!(
                            "An attempt to create an executor with work sharing and without a \
                            thread pool enabled has failed because another executor was created with \
                            both a thread pool and work sharing enabled. \
                            This is unacceptable because an executor who does not have a \
                            thread pool cannot take on a task that requires a thread pool."
                        );
                    }

                    global_config_stats.number_of_executors_with_work_sharing_and_without_thread_pool += 1;
                }
            }
        }

        ValidConfig {
            buffer_len: self.buffer_len,
            io_worker_config: self.io_worker_config,
            is_thread_pool_enabled: self.is_thread_pool_enabled,
            is_work_sharing_enabled: self.is_work_sharing_enabled
        }
    }
}

impl From<&ValidConfig> for Config {
    fn from(config: &ValidConfig) -> Self {
        Config {
            buffer_len: config.buffer_len,
            io_worker_config: config.io_worker_config,
            is_thread_pool_enabled: config.is_thread_pool_enabled,
            is_work_sharing_enabled: config.is_work_sharing_enabled
        }
    }
}

#[cfg(test)]
mod tests {
    use std::intrinsics::black_box;
    use super::*;

    #[test_macro::test]
    fn test_default_config() {
        let config = Config::new().validate();
        assert_eq!(config.buffer_len, DEFAULT_BUF_LEN);
        assert!(config.io_worker_config.is_some());
        assert!(config.is_thread_pool_enabled);
        assert!(config.is_work_sharing_enabled);
    }

    #[test_macro::test]
    fn test_config() {
        let config = Config::new()
            .set_buffer_len(1024)
            .set_io_worker_config(None).unwrap()
            .set_thread_pool_enabled(false)
            .set_work_sharing_enabled(false);

        let config = config.validate();
        assert_eq!(config.buffer_len, 1024);
        assert!(config.io_worker_config.is_none());
        assert!(!config.is_thread_pool_enabled);
        assert!(!config.is_work_sharing_enabled);
    }

    // 4 cases for panic
    // 1 - first config with io worker and work sharing, next with work sharing and without io worker
    // 2 - first config with work sharing and without io worker, next with io worker and work sharing
    // 3 - first config with work sharing and without thread pool, next with thread pool and work sharing
    // 4 - first config with thread pool and work sharing, next with work sharing and without thread pool

    #[test_macro::test]
    #[should_panic]
    fn test_first_case_panic() {
        // with io worker and work sharing
        let first_config = Config::new().validate();
        let second_config = Config::new()
            .set_io_worker_config(None).unwrap()
            .set_work_sharing_enabled(true)
            .validate();

        black_box(first_config);
        black_box(second_config);
    }

    #[test_macro::test]
    #[should_panic]
    fn test_second_case_panic() {
        // with work sharing and without io worker
        let first_config = Config::new()
            .set_io_worker_config(None).unwrap()
            .set_work_sharing_enabled(true)
            .validate();
        let second_config = Config::new().validate();

        black_box(first_config);
        black_box(second_config);
    }

    #[test_macro::test]
    #[should_panic]
    fn test_third_case_panic() {
        // with work sharing and without thread pool
        let first_config = Config::new()
            .set_thread_pool_enabled(false)
            .set_work_sharing_enabled(true)
            .validate();
        let second_config = Config::new().validate();

        black_box(first_config);
        black_box(second_config);
    }

    #[test_macro::test]
    #[should_panic]
    fn test_fourth_case_panic() {
        // with thread pool and work sharing
        let first_config = Config::new().validate();
        let second_config = Config::new()
            .set_thread_pool_enabled(false)
            .set_work_sharing_enabled(true)
            .validate();

        black_box(first_config);
        black_box(second_config);
    }
}