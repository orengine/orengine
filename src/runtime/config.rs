use crate::io::IoWorkerConfig;
use crate::utils::SpinLock;
use crate::BUG_MESSAGE;
use std::mem::discriminant;

/// A global config of state of the all runtime.
/// It is used to prevent unsafe behavior in the runtime.
///
/// For example, when the task that uses an IO worker is
/// shared with the [`Executor`](crate::runtime::executor::Executor), that has no IO worker.
struct ConfigStats {
    number_of_executors_with_enabled_io_worker_and_work_sharing: usize,
    number_of_executors_with_enabled_thread_pool_and_work_sharing: usize,
    number_of_executors_with_work_sharing_and_without_io_worker: usize,
    number_of_executors_with_work_sharing_and_without_thread_pool: usize,
}

impl ConfigStats {
    /// Create a new config stats.
    const fn new() -> Self {
        Self {
            number_of_executors_with_enabled_io_worker_and_work_sharing: 0,
            number_of_executors_with_enabled_thread_pool_and_work_sharing: 0,
            number_of_executors_with_work_sharing_and_without_io_worker: 0,
            number_of_executors_with_work_sharing_and_without_thread_pool: 0,
        }
    }
}

/// A global config of state of the all runtime.
static GLOBAL_CONFIG_STATS: SpinLock<ConfigStats> = SpinLock::new(ConfigStats::new());

/// The default [`buffers`](crate::buf::Buffer) capacity.
pub const DEFAULT_BUF_CAP: usize = 4096;

/// Config that can be used to create an Executor, because it is valid.
#[derive(Clone)]
pub(crate) struct ValidConfig {
    pub(crate) buffer_cap: usize,
    pub(crate) io_worker_config: Option<IoWorkerConfig>,
    pub(crate) number_of_thread_workers: usize,
    /// If it is `usize::MAX`, it means that work sharing is disabled.
    pub(crate) work_sharing_level: usize,
}

impl ValidConfig {
    /// Returns whether the IO worker is enabled.
    pub const fn is_work_sharing_enabled(&self) -> bool {
        self.work_sharing_level != usize::MAX
    }

    /// Returns whether the thread pool is enabled.
    pub const fn is_thread_pool_enabled(&self) -> bool {
        self.number_of_thread_workers != 0
    }
}

impl Drop for ValidConfig {
    fn drop(&mut self) {
        if self.work_sharing_level != usize::MAX {
            let mut guard;
            if self.io_worker_config.is_some() {
                guard = Some(GLOBAL_CONFIG_STATS.lock());
                let global_config_stats = guard.as_mut().expect(BUG_MESSAGE);

                global_config_stats.number_of_executors_with_enabled_io_worker_and_work_sharing -=
                    1;
            } else {
                guard = Some(GLOBAL_CONFIG_STATS.lock());
                let global_config_stats = guard.as_mut().expect(BUG_MESSAGE);

                global_config_stats.number_of_executors_with_work_sharing_and_without_io_worker -=
                    1;
            }

            if self.is_thread_pool_enabled() {
                if guard.is_none() {
                    guard = Some(GLOBAL_CONFIG_STATS.lock());
                }

                let global_config_stats = guard.as_mut().expect(BUG_MESSAGE);
                global_config_stats
                    .number_of_executors_with_enabled_thread_pool_and_work_sharing -= 1;
            } else {
                if guard.is_none() {
                    guard = Some(GLOBAL_CONFIG_STATS.lock());
                }

                let global_config_stats = guard.as_mut().expect(BUG_MESSAGE);
                global_config_stats
                    .number_of_executors_with_work_sharing_and_without_thread_pool -= 1;
            }
        }
    }
}

/// `Config` is a configuration struct used for controlling various parameters
/// related to buffers, I/O workers, thread workers, and work-sharing behavior.
///
/// # Fields
/// - `buffer_cap`: The size of the [`buffers`](crate::buf::Buffer).
///
/// - `io_worker_config`: An optional configuration for I/O workers. If none is provided,
/// the IO worker will be disabled.
///
/// - `number_of_thread_workers`: The number of thread workers to spawn. If zero is provided,
/// the thread pool will be disabled.
///
/// - `work_sharing_level`: The level of work sharing between threads. It is responsible for
/// how many tasks the [`Executor`](crate::runtime::executor::Executor) can hold before assigning
/// them to the shared queue.
/// If [`usize::MAX`] is provided, work sharing will be disabled.
#[derive(Clone, Copy)]
pub struct Config {
    /// The size of the [`buffers`](crate::buf::Buffer).
    buffer_cap: usize,
    /// An optional configuration for I/O workers. If none is provided,
    /// the IO worker will be disabled.
    io_worker_config: Option<IoWorkerConfig>,
    /// The number of thread workers to spawn. If zero is provided,
    /// the thread pool will be disabled.
    number_of_thread_workers: usize,
    /// The level of work sharing between threads. It is responsible for
    /// how many tasks the [`Executor`](crate::runtime::executor::Executor) can hold before assigning
    /// them to the shared queue.
    /// If [`usize::MAX`] is provided, work sharing will be disabled.
    work_sharing_level: usize,
}

impl Config {
    /// Returns a default [`Config`].
    pub const fn default() -> Self {
        Self {
            buffer_cap: DEFAULT_BUF_CAP,
            io_worker_config: Some(IoWorkerConfig::default()),
            number_of_thread_workers: 1,
            work_sharing_level: 7,
        }
    }

    /// Returns the capacity of the [`buffers`](crate::buf::Buffer).
    pub const fn buffer_cap(&self) -> usize {
        self.buffer_cap
    }

    /// Sets the capacity of the [`buffers`](crate::buf::Buffer).
    pub const fn set_buffer_cap(mut self, buf_cap: usize) -> Self {
        self.buffer_cap = buf_cap;

        self
    }

    /// Returns the optional configuration for I/O workers. If none is returned,
    /// the IO worker is disabled.
    pub const fn io_worker_config(&self) -> Option<IoWorkerConfig> {
        self.io_worker_config
    }

    /// Sets the optional configuration for I/O workers. If none is provided,
    /// the IO worker will be disabled.
    pub const fn set_io_worker_config(
        mut self,
        io_worker_config: Option<IoWorkerConfig>,
    ) -> Result<Self, &'static str> {
        match io_worker_config {
            Some(io_worker_config) => {
                if let Err(err) = io_worker_config.validate() {
                    return Err(err);
                }

                self.io_worker_config = Some(io_worker_config);
            }
            None => {
                self.io_worker_config = None;
            }
        }

        Ok(self)
    }

    /// Disables the IO worker.
    pub const fn disable_io_worker(mut self) -> Self {
        self.io_worker_config = None;

        self
    }

    /// Returns the number of thread workers to spawn. If zero is returned,
    /// the thread pool is disabled.
    pub const fn number_of_thread_workers(&self) -> usize {
        self.number_of_thread_workers
    }

    /// Returns whether the thread pool is enabled.
    pub const fn is_thread_pool_enabled(&self) -> bool {
        self.number_of_thread_workers != 0
    }

    /// Sets the number of thread workers to spawn. If zero is provided,
    /// the thread pool will be disabled.
    pub const fn set_numbers_of_thread_workers(mut self, number_of_thread_workers: usize) -> Self {
        self.number_of_thread_workers = number_of_thread_workers;

        self
    }

    /// Returns whether the work sharing is enabled.
    pub const fn is_work_sharing_enabled(&self) -> bool {
        self.work_sharing_level != usize::MAX
    }

    /// Enables the work sharing.
    pub const fn enable_work_sharing(mut self) -> Self {
        if self.work_sharing_level == usize::MAX {
            self.work_sharing_level = 7;
        }

        self
    }

    /// Disables the work sharing.
    pub const fn disable_work_sharing(mut self) -> Self {
        self.work_sharing_level = usize::MAX;

        self
    }

    /// Sets the level of work sharing between threads. It is responsible for
    /// how many tasks the [`Executor`](crate::runtime::executor::Executor) can hold before assigning
    /// them to the shared queue.
    /// If [`usize::MAX`] is provided, work sharing will be disabled.
    pub const fn set_work_sharing_level(mut self, work_sharing_level: usize) -> Self {
        self.work_sharing_level = work_sharing_level;

        self
    }

    /// Validates the configuration.
    pub(crate) fn validate(self) -> ValidConfig {
        if self.work_sharing_level != usize::MAX {
            let mut global_config_stats = GLOBAL_CONFIG_STATS.lock();

            match self.io_worker_config.is_some() {
                true => {
                    if global_config_stats
                        .number_of_executors_with_work_sharing_and_without_io_worker
                        != 0
                    {
                        panic!(
                            "An attempt to create an Executor with work sharing and with an \
                            IO worker has failed because another Executor was created with \
                            work sharing enabled and without an IO worker enabled. \
                            This is unacceptable because an Executor who does not have an \
                            IO worker cannot take on a task that requires an IO worker."
                        );
                    }

                    global_config_stats
                        .number_of_executors_with_enabled_io_worker_and_work_sharing += 1;
                }
                false => {
                    if global_config_stats
                        .number_of_executors_with_enabled_io_worker_and_work_sharing
                        != 0
                    {
                        panic!(
                            "An attempt to create an Executor with work sharing and without an \
                            IO worker has failed because another Executor was created with \
                            an IO worker and work sharing enabled. \
                            This is unacceptable because an Executor who does not have an \
                            IO worker cannot take on a task that requires an IO worker."
                        );
                    }

                    global_config_stats
                        .number_of_executors_with_work_sharing_and_without_io_worker += 1;
                }
            }

            match self.is_thread_pool_enabled() {
                true => {
                    if global_config_stats
                        .number_of_executors_with_work_sharing_and_without_thread_pool
                        != 0
                    {
                        panic!(
                            "An attempt to create an Executor with work sharing and with a \
                            thread pool enabled has failed because another Executor was created with \
                            work sharing enabled and without a thread pool enabled. \
                            This is unacceptable because an Executor who does not have a \
                            thread pool cannot take on a task that requires a thread pool."
                        );
                    }

                    global_config_stats
                        .number_of_executors_with_enabled_thread_pool_and_work_sharing += 1;
                }
                false => {
                    if global_config_stats
                        .number_of_executors_with_enabled_thread_pool_and_work_sharing
                        != 0
                    {
                        panic!(
                            "An attempt to create an Executor with work sharing and without a \
                            thread pool enabled has failed because another Executor was created with \
                            both a thread pool and work sharing enabled. \
                            This is unacceptable because an Executor who does not have a \
                            thread pool cannot take on a task that requires a thread pool."
                        );
                    }

                    global_config_stats
                        .number_of_executors_with_work_sharing_and_without_thread_pool += 1;
                }
            }
        }

        ValidConfig {
            buffer_cap: self.buffer_cap,
            io_worker_config: self.io_worker_config,
            number_of_thread_workers: self.number_of_thread_workers,
            work_sharing_level: self.work_sharing_level,
        }
    }
}

impl From<&ValidConfig> for Config {
    fn from(config: &ValidConfig) -> Self {
        Config {
            buffer_cap: config.buffer_cap,
            io_worker_config: config.io_worker_config,
            number_of_thread_workers: config.number_of_thread_workers,
            work_sharing_level: config.work_sharing_level,
        }
    }
}

impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        self.buffer_cap == other.buffer_cap
            && discriminant(&self.io_worker_config) == discriminant(&other.io_worker_config)
            && self.number_of_thread_workers == other.number_of_thread_workers
            && self.work_sharing_level == other.work_sharing_level
    }
}

impl Eq for Config {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn get_lock() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|e| {
            LOCK.clear_poison();
            e.into_inner()
        })
    }

    #[orengine_macros::test_local]
    fn test_default_config() {
        let lock = get_lock();
        let config = Config::default().validate();
        assert_eq!(config.buffer_cap, DEFAULT_BUF_CAP);
        assert!(config.io_worker_config.is_some());
        assert!(config.is_thread_pool_enabled());
        assert_ne!(config.work_sharing_level, usize::MAX);
        drop(lock);
    }

    #[orengine_macros::test_local]
    fn test_config() {
        let lock = get_lock();
        let config = Config::default()
            .set_buffer_cap(1024)
            .set_io_worker_config(None)
            .unwrap()
            .set_numbers_of_thread_workers(0)
            .disable_work_sharing();

        let config = config.validate();
        assert_eq!(config.buffer_cap, 1024);
        assert!(config.io_worker_config.is_none());
        assert!(!config.is_thread_pool_enabled());
        assert_eq!(config.work_sharing_level, usize::MAX);
        assert!(!config.is_work_sharing_enabled());

        drop(lock);
    }

    // 4 cases for panic
    // 1 - first config with io worker and task, next with work sharing and without io worker
    // 2 - first config with work sharing and without io worker, next with io worker and work sharing
    // 3 - first config with work sharing and without thread pool, next with thread pool and work sharing
    // 4 - first config with thread pool and work sharing, next with work sharing and without thread pool
    #[orengine_macros::test_local]
    #[should_panic]
    fn test_config_first_case_panic() {
        let lock = get_lock();
        // with io worker and work sharing
        let _first_config = Config::default().validate();
        let _second_config = Config::default()
            .set_io_worker_config(None)
            .unwrap()
            .enable_work_sharing()
            .validate();

        drop(lock);
    }

    #[orengine_macros::test_local]
    #[should_panic]
    fn test_config_second_case_panic() {
        let lock = get_lock();
        // with work sharing and without io worker
        let _first_config = Config::default()
            .set_io_worker_config(None)
            .unwrap()
            .enable_work_sharing()
            .validate();
        let _second_config = Config::default().validate();

        drop(lock);
    }

    #[orengine_macros::test_local]
    #[should_panic]
    fn test_config_third_case_panic() {
        let lock = get_lock();
        // with work sharing and without thread pool
        let _first_config = Config::default()
            .set_numbers_of_thread_workers(0)
            .enable_work_sharing()
            .validate();
        let _second_config = Config::default().validate();

        drop(lock);
    }

    #[orengine_macros::test_local]
    #[should_panic]
    fn test_config_fourth_case_panic() {
        let lock = get_lock();
        // with thread pool and work sharing
        let _first_config = Config::default().validate();
        let _second_config = Config::default()
            .set_numbers_of_thread_workers(0)
            .enable_work_sharing()
            .validate();

        drop(lock);
    }
}
