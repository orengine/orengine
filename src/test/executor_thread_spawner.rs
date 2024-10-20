// TODO r or docs

use crate::runtime::Config;
use crate::test::WORK_SHARING_TEST_LOCK;
use crate::{stop_executor, Executor};
use std::future::Future;

pub struct ExecutorThreadSpawner {
    ids_of_executors: Vec<usize>,
    config: Config,
}

macro_rules! generate_spawn_executor_function_body {
    ($spawner:expr, $method:expr, $fut:expr, $handle_result_fn:expr) => {
        let id = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cond_var = std::sync::Arc::new(std::sync::Condvar::new());
        let id_clone = id.clone();
        let cond_var_clone = cond_var.clone();
        let cfg = $spawner.config.clone();
        std::thread::spawn(move || {
            let ex = Executor::init_with_config(cfg);
            id_clone.lock().unwrap().replace(ex.id());
            cond_var_clone.notify_one();
            let res = $method(ex, $fut);
            $handle_result_fn(res);
        });

        let mut id_lock = id.lock().unwrap();
        while id_lock.is_none() {
            id_lock = cond_var.wait(id_lock).unwrap();
        }
        $spawner.ids_of_executors.push(id_lock.unwrap());
    };
}

impl ExecutorThreadSpawner {
    pub fn with_config(config: Config) -> Self {
        if config.is_work_sharing_enabled() {
            if WORK_SHARING_TEST_LOCK.try_lock().is_ok() {
                panic!(
                    "ExecutorThreadSpawner with work sharing used in \
                TestRunner without work sharing!"
                );
            }
        }

        Self {
            ids_of_executors: Vec::new(),
            config,
        }
    }

    pub fn default() -> Self {
        Self::with_config(Config::default().disable_work_sharing())
    }

    pub fn spawn_executor_and_block_on_local<Ret, Fut>(&mut self, fut: Fut)
    where
        Fut: Future<Output = Ret> + Send + 'static,
    {
        generate_spawn_executor_function_body!(
            self,
            Executor::run_and_block_on_local,
            fut,
            |res: Result<Ret, &'static str>| {
                res.expect("Spawned executor has panicked!");
            }
        );
    }

    pub fn spawn_executor_and_block_on_global<Ret, Fut>(&mut self, fut: Fut)
    where
        Fut: Future<Output = Ret> + Send + 'static,
    {
        generate_spawn_executor_function_body!(
            self,
            Executor::run_and_block_on_global,
            fut,
            |res: Result<Ret, &'static str>| {
                res.expect("Spawned executor has panicked!");
            }
        );
    }

    pub fn spawn_executor_and_spawn_local<Fut>(&mut self, fut: Fut)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        generate_spawn_executor_function_body!(
            self,
            Executor::run_with_local_future,
            fut,
            |_: ()| {}
        );
    }

    pub fn spawn_executor_and_spawn_global<Fut>(&mut self, fut: Fut)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        generate_spawn_executor_function_body!(
            self,
            Executor::run_with_global_future,
            fut,
            |_: ()| {}
        );
    }

    pub fn ids_of_executors(&self) -> &[usize] {
        &self.ids_of_executors
    }

    pub fn stop_spawned_executors(&mut self) {
        for id in self.ids_of_executors.drain(..) {
            stop_executor(id);
        }
    }
}

impl Drop for ExecutorThreadSpawner {
    fn drop(&mut self) {
        self.stop_spawned_executors();
    }
}
