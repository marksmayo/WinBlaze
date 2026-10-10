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

    /// Includes the calling worker; additional workers need independent work.
    pub fn worker_count_for_work(&self, available: usize) -> WorkerCount {
        WorkerCount(self.worker_count.0.max(1).min(available.max(1)))
    }

    pub fn spawn_worker<F>(&self, worker: F) -> thread::JoinHandle<()>
    where
        F: FnOnce() + Send + 'static,
    {
        thread::spawn(worker)
    }
}

#[cfg(test)]
mod tests {
    use super::{ScanScheduler, WorkerCount};
    #[test]
    fn workers_follow_available_work_without_exceeding_the_limit() {
        let scheduler = ScanScheduler::new(WorkerCount(4));
        assert_eq!(scheduler.worker_count_for_work(0), WorkerCount(1));
        assert_eq!(scheduler.worker_count_for_work(2), WorkerCount(2));
        assert_eq!(scheduler.worker_count_for_work(100), WorkerCount(4));
    }
}
