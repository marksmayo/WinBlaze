//! Stage complete outputs before replacing the selected report file.
use std::{
    fs::{self, OpenOptions},
    io::{self, BufWriter, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Writer buffer for an export of `records` rows: a bounded scratch for small
/// catalogs (a tiny export must not reserve a megabyte), the established
/// 1 MiB for ordinary ones, and 4 MiB once a volume's millions of rows make
/// each flush a large sequential write.
pub(crate) fn export_buffer_capacity(records: usize) -> usize {
    if records >= 1_000_000 {
        4 << 20
    } else {
        records.saturating_mul(256).clamp(8 * 1024, 1 << 20)
    }
}
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

#[cfg(test)]
mod tests {
    #[test]
    fn export_buffers_grow_with_the_catalog_and_stay_small_for_small_ones() {
        assert_eq!(super::export_buffer_capacity(0), 8 * 1024);
        assert_eq!(super::export_buffer_capacity(72), 18_432);
        assert_eq!(super::export_buffer_capacity(100_000), 1 << 20);
        assert_eq!(super::export_buffer_capacity(999_999), 1 << 20);
        assert_eq!(super::export_buffer_capacity(1_000_000), 4 << 20);
        assert_eq!(super::export_buffer_capacity(5_000_000), 4 << 20);
    }
}
