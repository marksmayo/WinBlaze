use std::thread;

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThread() -> isize;
    fn SetThreadPriority(thread: isize, priority: i32) -> i32;
    fn GetThreadPriority(thread: isize) -> i32;
}

const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;

/// Called first on every scan thread. A background scan (the GUI's) runs one
/// step below normal, so the UI thread gets a core the moment a message
/// arrives even while every walker is busy; it still takes all idle CPU.
/// Windows starts each thread at normal priority, so each one must ask.
pub fn enter_scan_thread_priority(background: bool) {
    if background {
        // SAFETY: pseudo-handle for the calling thread; failure leaves the
        // thread at normal priority, which only costs responsiveness.
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
    }
}

/// The calling thread's priority, for a thread it spawns to adopt with
/// [`adopt_thread_priority`] (Windows threads do not inherit it).
pub(crate) fn current_thread_priority() -> i32 {
    // SAFETY: pseudo-handle for the calling thread.
    unsafe { GetThreadPriority(GetCurrentThread()) }
}

pub(crate) fn adopt_thread_priority(priority: i32) {
    if priority != 0 {
        // SAFETY: pseudo-handle for the calling thread; see above.
        unsafe {
            SetThreadPriority(GetCurrentThread(), priority);
        }
    }
}

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
    fn background_scan_threads_run_below_the_ui_thread() {
        // Windows starts every thread at normal priority; a background scan
        // worker must lower itself so the UI thread always wins a core.
        let observed = std::thread::spawn(|| {
            super::enter_scan_thread_priority(true);
            super::current_thread_priority()
        })
        .join()
        .unwrap();
        assert_eq!(observed, -1, "THREAD_PRIORITY_BELOW_NORMAL");
        let normal = std::thread::spawn(|| {
            super::enter_scan_thread_priority(false);
            super::current_thread_priority()
        })
        .join()
        .unwrap();
        assert_eq!(normal, 0, "foreground scans (the CLI) keep normal priority");
    }

    #[test]
    fn spawned_walkers_adopt_their_spawners_priority() {
        let observed = std::thread::spawn(|| {
            super::enter_scan_thread_priority(true);
            let inherited = super::current_thread_priority();
            std::thread::spawn(move || {
                super::adopt_thread_priority(inherited);
                super::current_thread_priority()
            })
            .join()
            .unwrap()
        })
        .join()
        .unwrap();
        assert_eq!(observed, -1);
    }

    #[test]
    fn workers_follow_available_work_without_exceeding_the_limit() {
        let scheduler = ScanScheduler::new(WorkerCount(4));
        assert_eq!(scheduler.worker_count_for_work(0), WorkerCount(1));
        assert_eq!(scheduler.worker_count_for_work(2), WorkerCount(2));
        assert_eq!(scheduler.worker_count_for_work(100), WorkerCount(4));
    }
}
