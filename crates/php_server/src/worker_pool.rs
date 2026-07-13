//! Dedicated PHP worker threads.
//!
//! Every PHP request used to run through `tokio::task::block_in_place`,
//! which converts whatever tokio worker happens to hold the request into a
//! temporary blocking thread. Those threads are ephemeral: tokio retires
//! them after an idle timeout, and every thread-keyed engine structure —
//! process-local native publication tables and worker-stable symbol epochs —
//! is discarded with them and rebuilt from cold on the next request.
//!
//! The pool replaces that with a fixed set of pinned OS threads that own
//! PHP execution for the lifetime of the process. Jobs and finished
//! responses are `Send` (see `worker_payload_tests` in `php_request`);
//! execution-side `Rc` state never crosses a thread boundary because the
//! entire synchronous request core runs inside the worker.

use std::sync::Mutex;
use std::sync::mpsc;

use tokio::sync::oneshot;
use tracing::warn;

/// Worker stack size for native PHP frames and runtime helpers. Pinned workers
/// use the same generous stack as the tokio workers, overridable through the
/// same environment variable.
fn php_worker_stack_bytes() -> usize {
    const DEFAULT_PHP_WORKER_STACK_BYTES: usize = 128 * 1024 * 1024;
    std::env::var("PHRUST_SERVER_TOKIO_WORKER_STACK_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_PHP_WORKER_STACK_BYTES)
}

/// One queued request job: a boxed closure running the synchronous request
/// core, paired with the reply channel for the finished response.
type Job = Box<dyn FnOnce() + Send + 'static>;

/// Fixed pool of dedicated PHP worker threads.
#[derive(Debug)]
pub(crate) struct PhpWorkerPool {
    sender: mpsc::Sender<Job>,
}

impl PhpWorkerPool {
    /// Spawns `workers` dedicated PHP threads sharing one job queue.
    pub(crate) fn new(workers: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = std::sync::Arc::new(Mutex::new(receiver));
        for index in 0..workers.max(1) {
            let receiver = std::sync::Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("php-worker-{index}"))
                .stack_size(php_worker_stack_bytes())
                .spawn(move || {
                    loop {
                        // Hold the lock only while dequeuing; execution runs
                        // unlocked so workers process jobs concurrently.
                        let job = {
                            let Ok(receiver) = receiver.lock() else {
                                return;
                            };
                            receiver.recv()
                        };
                        match job {
                            Ok(job) => job(),
                            // All senders dropped: the server is shutting
                            // down and the worker retires with it.
                            Err(_) => return,
                        }
                    }
                })
                .expect("spawn php worker thread");
        }
        Self { sender }
    }

    /// Runs a synchronous job on a pinned worker and awaits its result.
    ///
    /// Falls back to running the job on the calling thread inside
    /// `block_in_place` when the pool queue is unavailable, so a degraded pool
    /// degrades to the pre-pool behavior instead of failing requests.
    pub(crate) async fn execute<T, F>(&self, job: F) -> T
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let (reply_sender, reply_receiver) = oneshot::channel();
        let boxed: Job = Box::new(move || {
            let _ = reply_sender.send(job());
        });
        match self.sender.send(boxed) {
            Ok(()) => reply_receiver
                .await
                .expect("php worker dropped the request reply"),
            // The queue is gone (a worker panic poisoned it or the pool is
            // shutting down): degrade to the pre-pool behavior and run the
            // job inline. The boxed job still reports through the reply
            // channel, so the result is recovered from there.
            Err(mpsc::SendError(job)) => {
                warn!("php worker pool unavailable; running request inline");
                tokio::task::block_in_place(move || {
                    job();
                    reply_receiver
                        .blocking_recv()
                        .expect("inline php job reply")
                })
            }
        }
    }
}
