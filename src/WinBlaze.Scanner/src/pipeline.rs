use std::sync::mpsc::Sender;

use winblaze_core::{
    DirectoryRecord, FileRecord, ScanEvent, ScanIssueRecord, ScanProgress, ScanSummary,
    VolumeRecord,
};

use crate::performance::ScanPipelineConfig;

#[derive(Debug)]
pub struct ScanEventPipeline {
    sender: Sender<ScanEvent>,
    config: ScanPipelineConfig,
    buffer: Vec<(ScanEvent, usize)>,
    buffered_bytes: usize,
    progress: ScanProgress,
}

impl ScanEventPipeline {
    pub fn new(sender: Sender<ScanEvent>, config: ScanPipelineConfig) -> Self {
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
    pub fn cloned_sender(&self) -> Sender<ScanEvent> {
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
        for (event, _) in self.buffer.drain(..) {
            let _ = self.sender.send(event);
        }
        self.buffered_bytes = 0;
    }

    fn enqueue(&mut self, event: ScanEvent, estimated_bytes: usize) {
        self.buffer.push((event, estimated_bytes));
        self.buffered_bytes = self.buffered_bytes.saturating_add(estimated_bytes);
        if self.should_flush() {
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

        assert!(matches!(rx.recv().expect("first"), ScanEvent::Progress(_)));
        assert!(matches!(rx.recv().expect("second"), ScanEvent::Completed));
    }
}
