//! Records which paths under a scanned root changed since its scan started,
//! so an incremental refresh can re-read only those directories.
//!
//! One watch at a time (the GUI scans one root at a time). It starts with a
//! directory walk, before the walk reads anything, so every change the walk
//! may have missed is recorded; re-reading a directory the walk already saw
//! is harmless. A notification overflow or a watch failure marks the record
//! incomplete, and the refresh then walks the whole tree as before.

use std::collections::BTreeSet;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

const FILE_LIST_DIRECTORY: u32 = 0x0001;
const FILE_SHARE_ALL: u32 = 0x0000_0007;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
const FILE_NOTIFY_CHANGE_FILE_NAME: u32 = 0x0001;
const FILE_NOTIFY_CHANGE_DIR_NAME: u32 = 0x0002;
const FILE_NOTIFY_CHANGE_ATTRIBUTES: u32 = 0x0004;
const FILE_NOTIFY_CHANGE_SIZE: u32 = 0x0008;
const FILE_NOTIFY_CHANGE_LAST_WRITE: u32 = 0x0010;
const FILE_NOTIFY_CHANGE_CREATION: u32 = 0x0040;
const INVALID_HANDLE_VALUE: isize = -1;
const WAIT_OBJECT_0: u32 = 0;
const INFINITE: u32 = 0xFFFF_FFFF;
const BUFFER_BYTES: usize = 64 * 1024;

#[repr(C)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: isize,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: isize,
    ) -> isize;
    fn ReadDirectoryChangesW(
        directory: isize,
        buffer: *mut c_void,
        length: u32,
        subtree: i32,
        filter: u32,
        returned: *mut u32,
        overlapped: *mut Overlapped,
        completion: *mut c_void,
    ) -> i32;
    fn GetOverlappedResult(
        handle: isize,
        overlapped: *mut Overlapped,
        transferred: *mut u32,
        wait: i32,
    ) -> i32;
    fn CancelIoEx(handle: isize, overlapped: *mut Overlapped) -> i32;
    fn CreateEventW(security: *mut c_void, manual: i32, initial: i32, name: *const u16) -> isize;
    fn SetEvent(event: isize) -> i32;
    fn ResetEvent(event: isize) -> i32;
    fn WaitForMultipleObjects(count: u32, handles: *const isize, all: i32, ms: u32) -> u32;
    fn WaitForSingleObject(handle: isize, ms: u32) -> u32;
    fn CloseHandle(handle: isize) -> i32;
}

/// What changed under the root, as the watch thread recorded it.
#[derive(Default)]
struct Record {
    /// Paths relative to the root, as Windows reported them.
    paths: BTreeSet<String>,
    /// Notifications were lost or the watch stopped: only a full walk is safe.
    incomplete: bool,
    synced: u64,
}

struct Shared {
    record: Mutex<Record>,
    synced: Condvar,
}

struct Events {
    stop: isize,
    sync: isize,
}

// SAFETY: event handles are process-wide kernel handles, usable on any thread.
unsafe impl Send for Events {}
unsafe impl Sync for Events {}

impl Drop for Events {
    fn drop(&mut self) {
        // SAFETY: both handles came from CreateEventW and are closed once.
        unsafe {
            CloseHandle(self.stop);
            CloseHandle(self.sync);
        }
    }
}

/// A running watch of one root.
pub struct ChangeWatch {
    root: String,
    shared: Arc<Shared>,
    events: Arc<Events>,
    thread: Option<thread::JoinHandle<()>>,
    requested: u64,
}

/// The changes recorded since the watch started, or since the last take.
pub struct TakenChanges {
    pub paths: Vec<String>,
}

impl ChangeWatch {
    /// Opens the root and arms the first read before returning, so changes
    /// made after this call are recorded. None when the root cannot be watched.
    pub fn start(root: &str) -> Option<Self> {
        let wide: Vec<u16> = std::path::Path::new(root)
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: the name is NUL-terminated; other pointer arguments are null.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_ALL,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                0,
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle == 0 {
            return None;
        }
        // SAFETY: plain manual-reset events with no name or security.
        let (stop, sync, completed) = unsafe {
            (
                CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()),
                CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null()),
                CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()),
            )
        };
        if stop == 0 || sync == 0 || completed == 0 {
            // SAFETY: close whichever handles were created.
            unsafe {
                for event in [stop, sync, completed] {
                    if event != 0 {
                        CloseHandle(event);
                    }
                }
                CloseHandle(handle);
            }
            return None;
        }
        let events = Arc::new(Events { stop, sync });
        let shared = Arc::new(Shared {
            record: Mutex::new(Record::default()),
            synced: Condvar::new(),
        });
        let mut reader = Reader {
            handle,
            completed,
            overlapped: Box::new(Overlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                event: completed,
            }),
            buffer: vec![0u32; BUFFER_BYTES / 4],
            shared: Arc::clone(&shared),
            armed: false,
        };
        // From here the reader owns the directory handle and the completion
        // event: dropping it (a failed arm or spawn) cancels and waits out its
        // read before freeing the buffer, then closes them.
        if !reader.arm() {
            return None;
        }
        let thread_events = Arc::clone(&events);
        let thread = thread::Builder::new()
            .name("winblaze-change-watch".into())
            .spawn(move || reader.run(&thread_events))
            .ok()?;
        Some(Self {
            root: root.to_owned(),
            shared,
            events,
            thread: Some(thread),
            requested: 0,
        })
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    /// Takes every change made before this call, after the watch thread has
    /// read all pending notifications. None when the record is incomplete
    /// (the caller must walk everything) or the watch thread did not answer.
    pub fn take(&mut self, timeout: Duration) -> Option<TakenChanges> {
        self.requested += 1;
        let requested = self.requested;
        // SAFETY: the sync event lives as long as `self.events`.
        unsafe { SetEvent(self.events.sync) };
        let guard = self.shared.record.lock().unwrap_or_else(|p| p.into_inner());
        let (mut record, wait) = self
            .shared
            .synced
            .wait_timeout_while(guard, timeout, |record| record.synced < requested)
            .unwrap_or_else(|p| p.into_inner());
        if wait.timed_out() || record.incomplete {
            return None;
        }
        Some(TakenChanges {
            paths: std::mem::take(&mut record.paths).into_iter().collect(),
        })
    }
}

impl Drop for ChangeWatch {
    fn drop(&mut self) {
        // SAFETY: the stop event lives as long as `self.events`.
        unsafe { SetEvent(self.events.stop) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Reader {
    handle: isize,
    completed: isize,
    overlapped: Box<Overlapped>,
    buffer: Vec<u32>,
    shared: Arc<Shared>,
    /// A read is pending on the buffer and OVERLAPPED.
    armed: bool,
}

impl Drop for Reader {
    fn drop(&mut self) {
        // SAFETY: cancel and wait out the pending read before its buffer and
        // OVERLAPPED are freed, then close the handles this reader owns.
        unsafe {
            if self.armed {
                CancelIoEx(self.handle, &mut *self.overlapped);
                let mut bytes = 0u32;
                GetOverlappedResult(self.handle, &mut *self.overlapped, &mut bytes, 1);
            }
            CloseHandle(self.handle);
            CloseHandle(self.completed);
        }
    }
}

// SAFETY: the reader and its pending read move to the watch thread once,
// before the first wait, and stay there.
unsafe impl Send for Reader {}

impl Reader {
    fn arm(&mut self) -> bool {
        // SAFETY: the buffer and OVERLAPPED are heap allocations that live,
        // unmoved, until the read completes or is cancelled and waited out.
        let armed = unsafe {
            ResetEvent(self.completed);
            ReadDirectoryChangesW(
                self.handle,
                self.buffer.as_mut_ptr().cast(),
                (self.buffer.len() * 4) as u32,
                1,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_ATTRIBUTES
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_CREATION,
                std::ptr::null_mut(),
                &mut *self.overlapped,
                std::ptr::null_mut(),
            ) != 0
        };
        self.armed = armed;
        if !armed {
            self.mark_incomplete();
        }
        armed
    }

    fn mark_incomplete(&self) {
        self.shared
            .record
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .incomplete = true;
    }

    /// Collects a finished read (if any) and re-arms. `wait` blocks for a
    /// read that a cancel is completing.
    fn collect(&mut self, wait: bool) -> bool {
        let mut bytes = 0u32;
        // SAFETY: the OVERLAPPED belongs to the read armed on this handle.
        let ok = unsafe {
            GetOverlappedResult(
                self.handle,
                &mut *self.overlapped,
                &mut bytes,
                i32::from(wait),
            )
        } != 0;
        if ok {
            self.record(bytes as usize);
        } else {
            let error = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            // 995: cancelled by the sync below; 996: still pending.
            if error == 996 {
                return true;
            }
            if error != 995 {
                // The read ended with an error: nothing is pending now.
                self.armed = false;
                self.mark_incomplete();
                return false;
            }
        }
        self.arm()
    }

    fn record(&self, bytes: usize) {
        let mut record = self.shared.record.lock().unwrap_or_else(|p| p.into_inner());
        if bytes == 0 {
            // The system's buffer overflowed: some changes are unknown.
            record.incomplete = true;
            return;
        }
        let words = &self.buffer;
        let mut offset = 0usize;
        loop {
            if offset + 12 > bytes || !offset.is_multiple_of(4) {
                record.incomplete = true;
                return;
            }
            let next = words[offset / 4] as usize;
            let name_bytes = words[offset / 4 + 2] as usize;
            if !name_bytes.is_multiple_of(2) || offset + 12 + name_bytes > bytes {
                record.incomplete = true;
                return;
            }
            // SAFETY: the range was bounds-checked against the bytes written.
            let name = unsafe {
                std::slice::from_raw_parts(
                    (words.as_ptr() as *const u8).add(offset + 12).cast::<u16>(),
                    name_bytes / 2,
                )
            };
            match String::from_utf16(name) {
                Ok(path) => {
                    record.paths.insert(path);
                }
                // A name the refresh cannot match exactly: walk instead.
                Err(_) => record.incomplete = true,
            }
            if next == 0 {
                return;
            }
            offset += next;
        }
    }

    fn run(mut self, events: &Events) {
        let handles = [self.completed, events.stop, events.sync];
        let mut alive = true;
        loop {
            // SAFETY: all three handles stay open for the thread's lifetime.
            let signalled = unsafe { WaitForMultipleObjects(3, handles.as_ptr(), 0, INFINITE) };
            match signalled.wrapping_sub(WAIT_OBJECT_0) {
                0 if alive => alive = self.collect(false),
                1 => break,
                2 => {
                    if alive {
                        // Cancel the pending read to collect anything it holds,
                        // then drain reads the kernel completes at once from
                        // changes it buffered in between.
                        // SAFETY: cancels only this handle's own read.
                        unsafe { CancelIoEx(self.handle, &mut *self.overlapped) };
                        alive = self.collect(true);
                        // SAFETY: polls the completion event without waiting.
                        while alive && unsafe { WaitForSingleObject(self.completed, 0) } == 0 {
                            alive = self.collect(false);
                        }
                    }
                    let mut record = self.shared.record.lock().unwrap_or_else(|p| p.into_inner());
                    record.synced += 1;
                    self.shared.synced.notify_all();
                }
                _ => {
                    if !alive {
                        // Nothing armed: wait only for stop or sync.
                        thread::sleep(Duration::from_millis(1));
                    } else {
                        self.mark_incomplete();
                        break;
                    }
                }
            }
            if !alive {
                // SAFETY: stops the completed event from spinning the wait.
                unsafe { ResetEvent(self.completed) };
            }
        }
        // Drop cancels any pending read and closes the handles.
        debug_assert_eq!(alive, self.armed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("winblaze-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        root
    }

    #[test]
    fn a_take_returns_every_change_made_before_it_exactly_once() {
        let root = temp_root("take");
        let mut watch = ChangeWatch::start(root.to_str().unwrap()).expect("watch starts");
        std::fs::write(root.join("new.txt"), b"x").unwrap();
        std::fs::write(root.join("sub").join("inner.txt"), b"y").unwrap();
        // No sleep: the take itself must see changes made before it.
        let taken = watch.take(Duration::from_secs(2)).expect("complete record");
        assert!(
            taken.paths.contains(&"new.txt".to_string()),
            "{:?}",
            taken.paths
        );
        assert!(
            taken.paths.contains(&"sub\\inner.txt".to_string()),
            "{:?}",
            taken.paths
        );
        let again = watch.take(Duration::from_secs(2)).expect("complete record");
        assert!(again.paths.is_empty(), "{:?}", again.paths);
        std::fs::write(root.join("later.txt"), b"z").unwrap();
        let later = watch.take(Duration::from_secs(2)).expect("complete record");
        assert!(
            later.paths.contains(&"later.txt".to_string()),
            "{:?}",
            later.paths
        );
        drop(watch);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_root_that_cannot_be_opened_is_not_watched() {
        assert!(ChangeWatch::start(r"C:\does-not-exist-winblaze-watch").is_none());
    }
}
