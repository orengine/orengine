use std::future::Future;

/// `AsyncWaitGroup` is a synchronization primitive that allows to [`wait`](Self::wait)
/// until all tasks are [`completed`](Self::done).
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
/// use orengine::sleep;
/// use orengine::sync::{shared_scope, AsyncWaitGroup, WaitGroup};
///
/// # async fn foo() {
/// let wait_group = WaitGroup::new();
/// let number_executed_tasks = AtomicUsize::new(0);
///
/// shared_scope(|scope| async {
///     for i in 0..10 {
///         wait_group.inc();
///         scope.spawn(async {
///             sleep(Duration::from_millis(i)).await;
///             number_executed_tasks.fetch_add(1, SeqCst);
///             wait_group.done();
///         });
///     }
///
///     wait_group.wait().await; // wait until all tasks are completed
///     assert_eq!(number_executed_tasks.load(SeqCst), 10);
/// }).await;
/// # }
/// ```
pub trait AsyncWaitGroup {
    /// Adds `count` to the `LocalWaitGroup` counter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use orengine::{sleep, Local};
    /// use orengine::sync::{local_scope, AsyncWaitGroup, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    /// let number_executed_tasks = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     wait_group.add(10);
    ///     for i in 0..10 {
    ///         scope.spawn(async {
    ///             sleep(Duration::from_millis(i)).await;
    ///             *number_executed_tasks.get_mut() += 1;
    ///             wait_group.done();
    ///         });
    ///     }
    ///
    ///     wait_group.wait().await; // wait until 10 tasks are completed
    ///     assert_eq!(*number_executed_tasks, 10);
    /// }).await;
    /// # }
    /// ```
    fn add(&self, count: usize);

    /// [`Adds`](Self::add) 1 to the `LocalWaitGroup` counter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use orengine::{sleep, Local};
    /// use orengine::sync::{local_scope, AsyncWaitGroup, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    /// let number_executed_tasks = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wait_group.inc();
    ///         scope.spawn(async {
    ///             sleep(Duration::from_millis(i)).await;
    ///             *number_executed_tasks.get_mut() += 1;
    ///             wait_group.done();
    ///         });
    ///     }
    ///
    ///     wait_group.wait().await; // wait until all tasks are completed
    ///     assert_eq!(*number_executed_tasks, 10);
    /// }).await;
    /// # }
    /// ```
    #[inline(always)]
    fn inc(&self) {
        self.add(1);
    }

    /// Returns the `LocalWaitGroup` counter.
    ///
    /// Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncWaitGroup, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    /// assert_eq!(wait_group.count(), 0);
    /// wait_group.inc();
    /// assert_eq!(wait_group.count(), 1);
    /// wait_group.done();
    /// assert_eq!(wait_group.count(), 0);
    /// # }
    /// ```
    fn count(&self) -> usize;

    /// Decreases the `LocalWaitGroup` counter by 1 and wakes up all tasks that are waiting
    /// if the counter reaches 0.
    ///
    /// Returns the current value of the counter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{local_scope, AsyncWaitGroup, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    ///
    /// local_scope(|scope| async {
    ///     wait_group.inc();
    ///     scope.spawn(async {
    ///         // wake up the waiting task, because a current and the only one task is done
    ///         let count = wait_group.done();
    ///         assert_eq!(count, 0);
    ///     });
    ///
    ///     wait_group.wait().await; // wait until all tasks are completed
    /// }).await;
    /// # }
    /// ```
    fn done(&self) -> usize;

    /// Waits until the `LocalWaitGroup` counter reaches 0.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use orengine::{sleep, Local};
    /// use orengine::sync::{local_scope, AsyncWaitGroup, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    /// let number_executed_tasks = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wait_group.inc();
    ///         scope.spawn(async {
    ///             sleep(Duration::from_millis(i)).await;
    ///             *number_executed_tasks.get_mut() += 1;
    ///             wait_group.done();
    ///         });
    ///     }
    ///
    ///     wait_group.wait().await; // wait until all tasks are completed
    ///     assert_eq!(*number_executed_tasks, 10);
    /// }).await;
    /// # }
    /// ```
    fn wait(&self) -> impl Future<Output = ()>;
}
