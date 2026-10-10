use std::sync::{
    mpsc::{self, Receiver},
    Arc, Mutex,
};

use winblaze_core::ScanEvent;

use crate::pipeline::EventBatchSender;

/// Bounded scan delivery that consumes records without exposing batch storage.
/// Dropping the receiver releases blocked producers, including during unwinding.
pub struct ScanEventReceiver {
    receiver: Receiver<Vec<ScanEvent>>,
    pool: Arc<EventBatchPool>,
}

const MAX_POOLED_BATCHES: usize = 4;
const MAX_BATCH_BYTES: usize = 64 * 1024;

/// Contains only empty arenas, never a producer sender or live records.
/// Its maximum retained event storage is four times 64 KiB.
#[derive(Debug, Default)]
pub(crate) struct EventBatchPool(Mutex<Vec<Vec<ScanEvent>>>);

impl EventBatchPool {
    pub(crate) fn take(&self, minimum: usize) -> Vec<ScanEvent> {
        let mut pool = self.0.lock().unwrap_or_else(|error| error.into_inner());
        pool.iter()
            .position(|batch| batch.capacity() >= minimum)
            .map(|index| pool.swap_remove(index))
            .unwrap_or_default()
    }

    fn recycle(&self, batch: Vec<ScanEvent>) {
        debug_assert!(batch.is_empty());
        if batch.capacity() == 0 || batch.capacity() > MAX_BATCH_BYTES / size_of::<ScanEvent>() {
            return;
        }
        let mut pool = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if pool.len() < MAX_POOLED_BATCHES {
            pool.push(batch);
        } else if let Some((index, smallest)) = pool
            .iter()
            .enumerate()
            .min_by_key(|(_, batch)| batch.capacity())
        {
            // Tiny control-only batches must not permanently fill the pool and
            // prevent data arenas from being reused later in the same scan.
            if smallest.capacity() < batch.capacity() {
                pool[index] = batch;
            }
        }
    }
}

impl ScanEventReceiver {
    pub(crate) fn channel(capacity: usize) -> (EventBatchSender, Self) {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let pool = Arc::new(EventBatchPool::default());
        (
            EventBatchSender::Recycling(sender, Arc::clone(&pool)),
            Self { receiver, pool },
        )
    }

    /// Consumes every event in producer order until all producers disconnect.
    pub fn for_each_event(self, consume: impl FnMut(ScanEvent)) {
        self.for_each_event_until(None, consume);
    }

    /// Drains owned batches until disconnect or the absolute deadline.
    /// A timeout releases blocked producers by dropping the receiver.
    pub fn for_each_event_until(
        self,
        deadline: Option<std::time::Instant>,
        mut consume: impl FnMut(ScanEvent),
    ) -> bool {
        loop {
            let batch = if let Some(deadline) = deadline {
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                match self
                    .receiver
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                {
                    Ok(batch) => batch,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return true,
                    Err(mpsc::RecvTimeoutError::Timeout) => return false,
                }
            } else {
                match self.receiver.recv() {
                    Ok(batch) => batch,
                    Err(_) => return true,
                }
            };
            let mut batch = batch;
            for event in batch.drain(..) {
                consume(event);
            }
            self.pool.recycle(batch);
        }
    }
}
