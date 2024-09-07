// use std::panic;
//
// pub struct BlockingTask {
//
// }
//
// fn run_thread_worker(
//     receiver: flume::Receiver<dyn FnOnce()>,
//     first_neighbor_receiver: flume::Receiver<dyn FnOnce()>,
//     second_neighbor_receiver: flume::Receiver<dyn FnOnce()>,
// ) {
//     loop {
//         match receiver.recv() {
//             Ok(task) => {
//                 (task)();
//             }
//             Err(flume::RecvError::Disconnected) => return
//         }
//
//         loop {
//             match receiver.try_recv() {
//                 Ok(task) => {
//                     (task)();
//                     continue;
//                 }
//                 Err(flume::TryRecvError::Empty) => {
//                     // try to steal from neighbors
//                 }
//                 Err(flume::TryRecvError::Disconnected) => return
//             }
//
//             macro_rules! steal_from_neighbor {
//                 ($receiver:ident) => {
//                     loop {
//                         match $receiver.try_recv() {
//                             Ok(task) => {
//                                 (task)();
//                             }
//                             Err(flume::TryRecvError::Empty) => { break }
//                             Err(flume::TryRecvError::Disconnected) => return
//                         }
//                     }
//                 };
//             }
//
//             steal_from_neighbor!(first_neighbor_receiver);
//             steal_from_neighbor!(second_neighbor_receiver);
//
//             break;
//         }
//     }
// }
//
// pub struct ThreadPool {
//     pub(crate) senders: Vec<flume::Sender<dyn FnOnce()>>
// }
//
// impl ThreadPool {
//     pub(crate) fn new(number_of_workers: usize) -> Self {
//         if number_of_workers < 3 {
//             panic!("number_of_workers must be at least 3")
//         }
//
//         let mut senders = Vec::with_capacity(number_of_workers);
//         let mut receivers = Vec::with_capacity(number_of_workers);
//
//         for _ in 0..number_of_workers {
//             let (sender, receiver) = flume::unbounded();
//             senders.push(sender);
//             receivers.push(receiver);
//         }
//
//         for i in 0..number_of_workers {
//             let receiver = receivers[i].clone();
//             let first_neighbor_receiver = receivers[(i + 1) % number_of_workers].clone();
//             let second_neighbor_receiver = receivers[(i + 2) % number_of_workers].clone();
//
//             std::thread::spawn(move || {
//                 let res = panic::catch_unwind(move || {
//                     run_thread_worker(receiver, first_neighbor_receiver, second_neighbor_receiver);
//                 });
//                 if let Err(err) = res {
//                     panic!(
//                         "Worker thread (that called in asyncify) panicked. Panic message: {:?}",
//                         err
//                     );
//                 }
//             });
//         }
//
//         Self {
//             senders
//         }
//     }
// }