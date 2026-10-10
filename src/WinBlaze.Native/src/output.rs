//! Stage complete outputs before replacing the selected report file.
use std::{
    fs::{self, OpenOptions},
    io::{self, BufWriter, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn atomic_write(
    path: &Path,
    writer: impl FnOnce(&mut BufWriter<std::fs::File>) -> io::Result<()>,
) -> io::Result<()> {
    atomic_write_with_capacity(path, 1 << 20, writer)
}

pub(crate) fn atomic_write_with_capacity(
    path: &Path,
    capacity: usize,
    writer: impl FnOnce(&mut BufWriter<std::fs::File>) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temp = parent.join(format!(
        ".winblaze-export-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| {
        let mut out = BufWriter::with_capacity(capacity, file);
        writer(&mut out)?;
        out.flush()?;
        out.get_ref().sync_all()?;
        drop(out);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
