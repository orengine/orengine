// TODO docs

use crate::{stop_executor, Executor};
use std::future::Future;

pub struct ExecutorThreadSpawner {
    ids_of_executors: Vec<usize>,
}

macro_rules! generate_spawn_executor_function_body {
    ($spawner:expr, $method:expr, $fut:expr, $handle_result_fn:expr) => {
        let id = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cond_var = std::sync::Arc::new(std::sync::Condvar::new());
        let id_clone = id.clone();
        let cond_var_clone = cond_var.clone();
        std::thread::spawn(move || {
            let ex = Executor::init();
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
    pub(crate) fn new() -> Self {
        Self {
            ids_of_executors: Vec::new(),
        }
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
}

impl Drop for ExecutorThreadSpawner {
    fn drop(&mut self) {
        for id in self.ids_of_executors.drain(..) {
            stop_executor(id);
        }
    }
}
