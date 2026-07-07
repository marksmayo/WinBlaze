use std::sync::mpsc::Sender;

use winblaze_core::{
    DirectoryRecord, FileRecord, ScanEvent, ScanIssueRecord, ScanProgress, ScanSummary,
    VolumeRecord,
};

use crate::performance::ScanPipelineConfig;

#[derive(Debug)]
pub struct ScanEventPipeline {
    sender: Sender<Vec<ScanEvent>>,
    config: ScanPipelineConfig,
    buffer: Vec<ScanEvent>,
    buffered_bytes: usize,
    progress: ScanProgress,
}

impl ScanEventPipeline {
    pub fn new(sender: Sender<Vec<ScanEvent>>, config: ScanPipelineConfig) -> Self {
        Self {
            sender,
            config,
            buffer: Vec::with_capacity(config.batch_size.max(1)),
            buffered_bytes: 0,
            progress: ScanProgress::default(),
        }
    }

    /// Clones the underlying sender so a worker thread can drive its own
    /// pipeline instance while still funneling events into the same channel.
    pub fn cloned_sender(&self) -> Sender<Vec<ScanEvent>> {
        self.sender.clone()
    }

    pub fn config(&self) -> ScanPipelineConfig {
        self.config
    }

    pub fn emit_session_started(&mut self, volume: VolumeRecord) {
        self.enqueue(ScanEvent::SessionStarted(volume), 128);
    }

    pub fn emit_volume_discovered(&mut self, volume: VolumeRecord) {
        self.enqueue(ScanEvent::VolumeDiscovered(volume), 128);
    }

    pub fn emit_directory(&mut self, directory: DirectoryRecord) {
        let size = directory
            .full_path
            .len()
            .saturating_add(directory.name.len())
            .max(64);
        self.enqueue(ScanEvent::DirectoryFound(directory), size);
    }

    pub fn emit_file(&mut self, file: FileRecord) {
        let size = file
            .full_path
            .len()
            .saturating_add(file.name.len())
            .saturating_add(128);
        self.progress.completed_bytes = self
            .progress
            .completed_bytes
            .saturating_add(file.size_bytes);
        self.enqueue(ScanEvent::FileFound(file), size);
    }

    pub fn emit_event(&mut self, event: ScanEvent) {
        let estimated_bytes = match &event {
            ScanEvent::SessionStarted(_) => 128,
            ScanEvent::VolumeDiscovered(_) => 128,
            ScanEvent::DirectoryFound(directory) => directory
                .full_path
                .len()
                .saturating_add(directory.name.len())
                .max(64),
            ScanEvent::FileFound(file) => file
                .full_path
                .len()
                .saturating_add(file.name.len())
                .saturating_add(128),
            ScanEvent::Issue(_) => 96,
            ScanEvent::Progress(_) => 32,
            ScanEvent::Summary(_) => 64,
            ScanEvent::Completed | ScanEvent::Cancelled | ScanEvent::Failed(_) => 16,
        };

        if let ScanEvent::FileFound(file) = &event {
            self.progress.completed_bytes = self
                .progress
                .completed_bytes
                .saturating_add(file.size_bytes);
        }

        self.enqueue(event, estimated_bytes);
    }

    pub fn emit_issue(&mut self, issue: ScanIssueRecord) {
        self.enqueue(ScanEvent::Issue(issue), 96);
    }

    pub fn emit_progress(&mut self, completed_items: u64, total_items: u64, total_bytes: u64) {
        self.progress.completed_items = completed_items;
        self.progress.total_items = total_items;
        self.progress.total_bytes = total_bytes;
        self.enqueue(ScanEvent::Progress(self.progress.clone()), 32);
    }

    pub fn emit_summary(&mut self, summary: ScanSummary) {
        self.enqueue(ScanEvent::Summary(summary), 64);
    }

    pub fn emit_completed(&mut self) {
        self.enqueue(ScanEvent::Completed, 16);
    }

    pub fn emit_cancelled(&mut self) {
        self.enqueue(ScanEvent::Cancelled, 16);
    }

    pub fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let batch = std::mem::replace(
            &mut self.buffer,
            Vec::with_capacity(self.config.batch_size.max(1)),
        );
        let _ = self.sender.send(batch);
        self.buffered_bytes = 0;
    }

    fn enqueue(&mut self, event: ScanEvent, estimated_bytes: usize) {
        // Progress must reach the consumer promptly, and terminal events must
        // not sit in a buffer behind cancellation checks, so both force a
        // flush of everything queued so far (in order).
        let force_flush = matches!(
            event,
            ScanEvent::Progress(_)
                | ScanEvent::Summary(_)
                | ScanEvent::Completed
                | ScanEvent::Cancelled
                | ScanEvent::Failed(_)
        );
        self.buffer.push(event);
        self.buffered_bytes = self.buffered_bytes.saturating_add(estimated_bytes);
        if force_flush || self.should_flush() {
            self.flush();
        }
    }

    fn should_flush(&self) -> bool {
        self.buffer.len() >= self.config.batch_size.max(1)
            || self
                .config
                .should_apply_backpressure(self.buffer.len(), self.buffered_bytes as u64)
    }
}

impl Drop for ScanEventPipeline {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn pipeline_batches_and_preserves_order() {
        let (tx, rx) = mpsc::channel();
        let mut pipeline = ScanEventPipeline::new(
            tx,
            ScanPipelineConfig {
                batch_size: 2,
                max_in_flight_events: 4,
                max_in_flight_bytes: 512,
            },
        );

        pipeline.emit_progress(1, 10, 100);
        pipeline.emit_completed();
        pipeline.flush();

        let mut events = rx.try_iter().flatten();
        assert!(matches!(
            events.next().expect("first"),
            ScanEvent::Progress(_)
        ));
        assert!(matches!(
            events.next().expect("second"),
            ScanEvent::Completed
        ));
        assert!(events.next().is_none());
    }

    #[test]
    fn pipeline_batches_records_until_batch_size() {
        let (tx, rx) = mpsc::channel();
        let mut pipeline = ScanEventPipeline::new(
            tx,
            ScanPipelineConfig {
                batch_size: 3,
                max_in_flight_events: 100,
                max_in_flight_bytes: 1 << 20,
            },
        );

        for _ in 0..2 {
            pipeline.emit_file(FileRecord::default());
        }
        assert!(
            rx.try_recv().is_err(),
            "records below batch size stay buffered"
        );

        pipeline.emit_file(FileRecord::default());
        assert_eq!(rx.try_recv().expect("batch at batch_size").len(), 3);
    }

    fn roomy_config() -> ScanPipelineConfig {
        ScanPipelineConfig {
            batch_size: 100,
            max_in_flight_events: 1000,
            max_in_flight_bytes: 1 << 20,
        }
    }

    #[test]
    fn emit_event_handles_every_variant_in_order() {
        let (tx, rx) = mpsc::channel();
        let mut pipeline = ScanEventPipeline::new(tx, roomy_config());

        pipeline.emit_event(ScanEvent::VolumeDiscovered(VolumeRecord::default()));
        pipeline.emit_event(ScanEvent::DirectoryFound(DirectoryRecord::default()));
        pipeline.emit_event(ScanEvent::FileFound(FileRecord {
            size_bytes: 7,
            ..FileRecord::default()
        }));
        pipeline.emit_event(ScanEvent::Issue(ScanIssueRecord {
            kind: winblaze_core::ScanIssueKind::Unknown,
            path: None,
            message: String::from("test"),
        }));
        pipeline.emit_event(ScanEvent::Summary(ScanSummary::default()));
        pipeline.emit_event(ScanEvent::Progress(ScanProgress::default()));
        pipeline.emit_event(ScanEvent::Failed(String::from("boom")));
        pipeline.emit_event(ScanEvent::Completed);
        pipeline.emit_event(ScanEvent::Cancelled);
        pipeline.flush();

        let events: Vec<ScanEvent> = rx.try_iter().flatten().collect();
        assert_eq!(events.len(), 9);
        assert!(matches!(events[0], ScanEvent::VolumeDiscovered(_)));
        assert!(matches!(events[2], ScanEvent::FileFound(_)));
        assert!(matches!(events[8], ScanEvent::Cancelled));
    }

    #[test]
    fn dedicated_emitters_enqueue_and_terminal_forces_flush() {
        let (tx, rx) = mpsc::channel();
        let mut pipeline = ScanEventPipeline::new(tx, roomy_config());

        pipeline.emit_volume_discovered(VolumeRecord::default());
        // A cancelled event is terminal, so it force-flushes everything queued.
        pipeline.emit_cancelled();

        let events: Vec<ScanEvent> = rx.try_iter().flatten().collect();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], ScanEvent::VolumeDiscovered(_)));
        assert!(matches!(events[1], ScanEvent::Cancelled));
    }
}
