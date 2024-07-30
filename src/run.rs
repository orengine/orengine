use std::future::Future;
use crate::{Executor, utils};
use crate::runtime::local_executor;

pub fn run_on_all_cores<Fut: Future<Output=()> + 'static, F: 'static + Clone + Send + Fn() -> Fut>(creator: F) {
    let cores = utils::core::get_core_ids().unwrap();
    for i in 1..cores.len() {
        let core = cores[i];
        let creator = creator.clone();
        std::thread::Builder::new()
            // TODO worker with id in panic and check worker id code
            .name(format!("worker on core: {}", i))
            .spawn(move || {
                Executor::init_on_core(core);
                local_executor().spawn_local(creator());
                local_executor().run();
            }).expect("failed to create worker thread");
    }

    Executor::init_on_core(cores[0]);
    local_executor().spawn_local(creator());
    local_executor().run();
}