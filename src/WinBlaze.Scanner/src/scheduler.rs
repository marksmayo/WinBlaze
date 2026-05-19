use std::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerCount(pub usize);

pub struct ScanScheduler {
    worker_count: WorkerCount,
}

impl ScanScheduler {
    pub fn new(worker_count: WorkerCount) -> Self {
        Self { worker_count }
    }

    pub fn worker_count(&self) -> WorkerCount {
        self.worker_count
    }

    pub fn spawn_worker<F>(&self, worker: F) -> thread::JoinHandle<()>
    where
        F: FnOnce() + Send + 'static,
    {
        thread::spawn(worker)
    }
}
