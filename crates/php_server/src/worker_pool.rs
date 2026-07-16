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
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::warn;

const DEFAULT_PHP_WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;
const _: () = assert!(DEFAULT_PHP_WORKER_STACK_BYTES <= 16 * 1024 * 1024);

/// Worker stack size for native spills and runtime helpers. PHP call depth is
/// bounded independently by the VM, so pinned workers do not reserve the
/// historical 128 MiB Tokio stack. The old variable remains a compatibility
/// fallback for deployments that configured both pools together.
fn php_worker_stack_bytes() -> usize {
    std::env::var("PHRUST_SERVER_PHP_WORKER_STACK_BYTES")
        .or_else(|_| std::env::var("PHRUST_SERVER_TOKIO_WORKER_STACK_BYTES"))
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_PHP_WORKER_STACK_BYTES)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_PHP_WORKER_STACK_BYTES, PhpWorkerPool};
    use std::sync::{Arc, Barrier};

    #[test]
    fn default_php_worker_stack_is_bounded() {
        assert!(std::hint::black_box(DEFAULT_PHP_WORKER_STACK_BYTES) <= 16 * 1024 * 1024);
    }

    #[tokio::test]
    async fn serial_requests_reuse_the_warm_worker() {
        let pool = PhpWorkerPool::new(4);
        let first = pool.execute(current_worker_name).await;
        let second = pool.execute(current_worker_name).await;
        let third = pool.execute(current_worker_name).await;

        assert_eq!(first, "php-worker-0");
        assert_eq!(second, first);
        assert_eq!(third, first);
    }

    #[tokio::test]
    async fn concurrent_requests_still_use_distinct_workers() {
        let pool = PhpWorkerPool::new(2);
        let barrier = Arc::new(Barrier::new(2));
        let left_barrier = Arc::clone(&barrier);
        let right_barrier = Arc::clone(&barrier);
        let (left, right) = tokio::join!(
            pool.execute(move || {
                left_barrier.wait();
                current_worker_name()
            }),
            pool.execute(move || {
                right_barrier.wait();
                current_worker_name()
            })
        );

        assert_ne!(left, right);
    }

    fn current_worker_name() -> String {
        std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_owned()
    }
}

/// Type-erased worker computation and its post-release completion callback.
///
/// The callback publishes the result only after the worker has returned to
/// the idle set. A caller that immediately submits the next serial request
/// therefore sees the just-warmed worker and does not spread one-request
/// allocator high-water across the entire pool.
type Completion = Box<dyn FnOnce() + Send + 'static>;
type Job = Box<dyn FnOnce() -> Completion + Send + 'static>;

struct WorkerJob {
    task: Job,
    worker: usize,
    idle_workers: std::sync::Arc<Mutex<Vec<usize>>>,
    permit: OwnedSemaphorePermit,
}

/// Fixed pool of dedicated PHP worker threads.
#[derive(Debug)]
pub(crate) struct PhpWorkerPool {
    workers: Vec<mpsc::Sender<WorkerJob>>,
    idle_workers: std::sync::Arc<Mutex<Vec<usize>>>,
    available_workers: std::sync::Arc<Semaphore>,
}

impl PhpWorkerPool {
    /// Spawns `workers` dedicated PHP threads sharing one job queue.
    pub(crate) fn new(workers: usize) -> Self {
        let worker_count = workers.max(1);
        let idle_workers =
            std::sync::Arc::new(Mutex::new((0..worker_count).rev().collect::<Vec<_>>()));
        let available_workers = std::sync::Arc::new(Semaphore::new(worker_count));
        let mut senders = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let (sender, receiver) = mpsc::channel::<WorkerJob>();
            senders.push(sender);
            std::thread::Builder::new()
                .name(format!("php-worker-{index}"))
                .stack_size(php_worker_stack_bytes())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        let WorkerJob {
                            task,
                            worker,
                            idle_workers,
                            permit,
                        } = job;
                        let completion = task();
                        if let Ok(mut idle) = idle_workers.lock() {
                            idle.push(worker);
                        }
                        drop(permit);
                        completion();
                    }
                })
                .expect("spawn php worker thread");
        }
        Self {
            workers: senders,
            idle_workers,
            available_workers,
        }
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
        let task: Job = Box::new(move || {
            let result = job();
            Box::new(move || {
                let _ = reply_sender.send(result);
            })
        });
        let permit = match std::sync::Arc::clone(&self.available_workers)
            .acquire_owned()
            .await
        {
            Ok(permit) => permit,
            Err(_) => return run_inline(task, reply_receiver),
        };
        let worker = self
            .idle_workers
            .lock()
            .ok()
            .and_then(|mut idle| idle.pop());
        let Some(worker) = worker else {
            drop(permit);
            return run_inline(task, reply_receiver);
        };
        let queued = WorkerJob {
            task,
            worker,
            idle_workers: std::sync::Arc::clone(&self.idle_workers),
            permit,
        };
        match self.workers[worker].send(queued) {
            Ok(()) => reply_receiver
                .await
                .expect("php worker dropped the request reply"),
            Err(mpsc::SendError(failed)) => {
                warn!(worker, "PHP worker unavailable; running request inline");
                // This sender is permanently unusable. Do not return its
                // availability permit to the dispatcher.
                failed.permit.forget();
                run_inline(failed.task, reply_receiver)
            }
        }
    }
}

fn run_inline<T>(task: Job, reply_receiver: oneshot::Receiver<T>) -> T {
    warn!("PHP worker pool unavailable; running request inline");
    tokio::task::block_in_place(move || {
        let completion = task();
        completion();
        reply_receiver
            .blocking_recv()
            .expect("inline PHP job reply")
    })
}
