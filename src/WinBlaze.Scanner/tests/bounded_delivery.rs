use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use winblaze_core::ScanEvent;
use winblaze_scanner::{
    ScanBackend, ScanController, ScanPipelineConfig, ScanRequest, ScanRuntimeConfig,
};

#[test]
fn recycling_consumer_panic_releases_cancelled_rendezvous_producer() {
    let root = std::env::temp_dir().join(format!("wb-recycling-unwind-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (controller, receiver) = ScanController::recycling_bounded_channel(0);
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            pipeline: ScanPipelineConfig {
                batch_size: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    });
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let consumer = std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            receiver.for_each_event(|event| {
                assert!(matches!(event, ScanEvent::SessionStarted(_)));
                ready_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                panic!("intentional consumer unwind");
            });
        }));
        assert!(result.is_err());
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    handle.cancel();
    drop(controller);
    release_tx.send(()).unwrap();
    consumer.join().unwrap();
    let (joined_tx, joined_rx) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        handle.join();
        joined_tx.send(()).unwrap();
    });
    joined_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("producer blocked after consumer unwind");
    waiter.join().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recycling_delivery_preserves_backpressure_and_exact_completion() {
    let root = std::env::temp_dir().join(format!("wb-recycling-slow-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for file in 0..16 {
        std::fs::write(root.join(format!("file-{file}.bin")), b"abc").unwrap();
    }
    let (controller, receiver) = ScanController::recycling_bounded_channel(0);
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            pipeline: ScanPipelineConfig {
                batch_size: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    });
    drop(controller);
    let finished = Arc::new(AtomicBool::new(false));
    let joined = Arc::clone(&finished);
    let waiter = std::thread::spawn(move || {
        handle.join();
        joined.store(true, Ordering::Release);
    });
    let mut files = 0;
    let mut completed = false;
    let mut root_id = None;
    receiver.for_each_event(|event| {
        assert!(!completed);
        match event {
            ScanEvent::SessionStarted(_) => {
                std::thread::sleep(Duration::from_millis(30));
                assert!(
                    !finished.load(Ordering::Acquire),
                    "producer ran past a paused rendezvous consumer"
                );
            }
            ScanEvent::DirectoryFound(directory) => root_id = Some(directory.id),
            ScanEvent::FileFound(file) => {
                assert_eq!(file.parent_directory_id, root_id.unwrap());
                assert_eq!(file.size_bytes, 3);
                files += 1;
            }
            ScanEvent::Summary(summary) => {
                assert_eq!((summary.files_seen, summary.total_size_bytes), (16, 48))
            }
            ScanEvent::Completed => completed = true,
            ScanEvent::Failed(message) => panic!("fixture scan failed: {message}"),
            ScanEvent::Cancelled | ScanEvent::Issue(_) => panic!("owned fixture must complete"),
            _ => {}
        }
    });
    waiter.join().unwrap();
    assert!(completed);
    assert_eq!(files, 16);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn dropping_a_bounded_consumer_releases_cancelled_producers() {
    let root = std::env::temp_dir().join(format!("wb-bounded-drop-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (controller, rx) = ScanController::bounded_channel(0);
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            ..Default::default()
        },
    });
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap().first(),
        Some(ScanEvent::SessionStarted(_))
    ));
    handle.cancel();
    drop(rx);
    drop(controller);
    let (tx, joined) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        handle.join();
        tx.send(()).unwrap();
    });
    joined
        .recv_timeout(Duration::from_secs(5))
        .expect("cancelled producer did not join after receiver drop");
    waiter.join().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn slow_consumer_bounds_delivery_without_losing_files_or_terminal_completion() {
    let root = std::env::temp_dir().join(format!("wb-bounded-delivery-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for id in 0..16 {
        std::fs::write(root.join(format!("file-{id}.bin")), b"abc").unwrap();
    }
    let (controller, rx) = ScanController::bounded_channel(1);
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            pipeline: ScanPipelineConfig {
                batch_size: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    });
    let finished = Arc::new(AtomicBool::new(false));
    let joined = Arc::clone(&finished);
    let waiter = std::thread::spawn(move || {
        handle.join();
        joined.store(true, Ordering::Release);
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while !finished.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let ran_ahead = finished.load(Ordering::Acquire);
    drop(controller);
    let mut files = 0;
    let mut bytes = 0;
    let mut completed = false;
    for batch in rx {
        assert_eq!(batch.len(), 1);
        for event in batch {
            match event {
                ScanEvent::FileFound(file) => {
                    files += 1;
                    bytes += file.size_bytes;
                }
                ScanEvent::Completed => completed = true,
                ScanEvent::Failed(message) => panic!("scan failed: {message}"),
                ScanEvent::Cancelled | ScanEvent::Issue(_) => panic!("owned fixture must complete"),
                _ => {}
            }
        }
    }
    waiter.join().unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(completed);
    assert_eq!((files, bytes), (16, 48));
    assert!(
        !ran_ahead,
        "producer queued the whole scan while its one-batch consumer was idle"
    );
}
