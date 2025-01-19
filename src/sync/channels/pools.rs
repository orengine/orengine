// TODO docs
use crate::runtime::Task;
use crate::sync::channels::states::{RecvCallState, SendCallState};
use ahash::HashMap;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::ptr;

pub(super) type SendDeque<T> = VecDeque<(Task, *mut SendCallState, *const T)>;
pub(super) type RecvDeque<T> = VecDeque<(Task, *mut RecvCallState, *mut T)>;

pub(super) struct Deques<T> {
    pub(super) senders: SendDeque<T>,
    pub(super) receivers: RecvDeque<T>,
}

impl<T> Deques<T> {
    fn new() -> Self {
        Self {
            senders: SendDeque::with_capacity(2),
            receivers: RecvDeque::with_capacity(2),
        }
    }
}

pub(super) struct DequesPoolGuard<T> {
    inner: ManuallyDrop<Deques<T>>,
}

impl<T> From<Deques<T>> for DequesPoolGuard<T> {
    fn from(inner: Deques<T>) -> Self {
        Self {
            inner: ManuallyDrop::new(inner),
        }
    }
}

impl<T> Deref for DequesPoolGuard<T> {
    type Target = Deques<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for DequesPoolGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> Drop for DequesPoolGuard<T> {
    fn drop(&mut self) {
        channel_inner_vec_deque_pool().put(unsafe { ManuallyDrop::take(&mut self.inner) });
    }
}

#[derive(Default)]
pub(super) struct ChannelInnerVecDequePool<T = ()> {
    storage: HashMap<usize, Vec<Deques<T>>>,
}

impl ChannelInnerVecDequePool {
    pub(super) fn get<T>(&mut self) -> DequesPoolGuard<T> {
        let typed_self =
            unsafe { &mut *(ptr::from_mut(self).cast::<ChannelInnerVecDequePool<T>>()) };
        let pool = typed_self.storage.entry(size_of::<T>()).or_default();

        let deques = pool.pop().unwrap_or_else(|| Deques::new());

        DequesPoolGuard::from(deques)
    }

    pub(super) fn put<T>(&mut self, deques: Deques<T>) {
        let typed_self =
            unsafe { &mut *(ptr::from_mut(self).cast::<ChannelInnerVecDequePool<T>>()) };
        let pool = typed_self.storage.entry(size_of::<T>()).or_default();

        pool.push(deques);
    }
}

thread_local! {
    static CHANNEL_INNER_VEC_DEQUE_POOL: UnsafeCell<ChannelInnerVecDequePool> = UnsafeCell::new(ChannelInnerVecDequePool::default());
}

pub(super) fn channel_inner_vec_deque_pool() -> &'static mut ChannelInnerVecDequePool {
    CHANNEL_INNER_VEC_DEQUE_POOL
        .with(|pool| unsafe { &mut *pool.get().cast::<ChannelInnerVecDequePool>() })
}
