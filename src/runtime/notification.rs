use crate::Executor;

#[derive(Clone)]
pub(crate) enum Notification {
    StoppedCurrentExecutor,
    RegisteredExecutor(&'static Executor),
    StoppedExecutor(&'static Executor)
}

