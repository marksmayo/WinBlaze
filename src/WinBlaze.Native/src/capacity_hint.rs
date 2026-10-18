//! Advisory allocation estimates; never a source of scan results.
use std::hash::{Hash, Hasher};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

const MAGIC: &[u8; 8] = b"WBCHNT01";
const MULTI_MAGIC: &[u8; 8] = b"WBCHNT02";
const HEADER: usize = 28;
const MAX_SCOPE: usize = 4096;
const MAX_HINTS: usize = 32;
const MAX_HINT_BYTES: usize = 12 + MAX_HINTS * (HEADER - 8 + MAX_SCOPE);
const MAX_RECORD_STORAGE: usize = 512 << 20;

#[derive(Debug, PartialEq, Eq)]
pub struct CapacityHint {
    pub files: usize,
    pub directories: usize,
}

struct StoredHint {
    scope: String,
    files: usize,
    directories: usize,
}

/// Key counts by the exact requested root and scan rules; estimates cannot
/// cross folders or filtered/unfiltered scans.
pub fn scope_key(root: &str, rules: &winblaze_scanner::ScanRules) -> String {
    let root = std::path::absolute(root)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| root.to_owned());
    // This fingerprint controls allocation only, never result validity. A
    // different hash implementation after an upgrade merely causes a miss.
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    rules.exclude_paths.hash(&mut hash);
    rules.include_files.hash(&mut hash);
    // A volume whose usage moved by more than ~2% (say a large tree was added
    // or removed) gets its own estimate: reserving for the old, larger count
    // commits memory a smaller scan never touches. Each state keeps its own.
    match volume_used_bytes(&root).map(usage_bucket) {
        Some(bucket) => format!("{root}\0{:016x}\0u{bucket}", hash.finish()),
        None => format!("{root}\0{:016x}", hash.finish()),
    }
}

/// Logarithmic 2% buckets of a volume's used bytes.
fn usage_bucket(used: u64) -> u32 {
    ((used.max(1) as f64).ln() / 1.02_f64.ln()) as u32
}

/// `X:\` for a root on a local fixed, removable or RAM drive. Shares and
/// mapped network drives get no usage bucket: asking a slow or disconnected
/// server for free space blocks until the redirector times out.
#[cfg(windows)]
fn local_drive_root(root: &str) -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    let path = root.strip_prefix(r"\\?\").unwrap_or(root);
    let bytes = path.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return None;
    }
    let drive = format!(r"{}\", &path[..2]);
    let wide: Vec<u16> = drive.encode_utf16().chain([0]).collect();
    // SAFETY: NUL-terminated drive root. DRIVE_REMOVABLE, DRIVE_FIXED and
    // DRIVE_RAMDISK are local; DRIVE_REMOTE and unknown types are skipped.
    matches!(unsafe { GetDriveTypeW(wide.as_ptr()) }, 2 | 3 | 6).then_some(drive)
}

#[cfg(windows)]
fn volume_used_bytes(root: &str) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let root = local_drive_root(root)?;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = std::ffi::OsStr::new(&root)
        .encode_wide()
        .chain([0])
        .collect();
    let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: NUL-terminated path; the three outputs are valid u64s.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) };
    (ok != 0 && total >= free).then(|| total - free)
}

#[cfg(not(windows))]
fn volume_used_bytes(_root: &str) -> Option<u64> {
    None
}

/// Read a bounded advisory cache and return the exact scope's estimate with
/// 5% room for growth. Missing, corrupt or unknown scopes are cache misses.
pub fn load_hint(path: &Path, scope: &str) -> Option<CapacityHint> {
    HintFile::read(path).hint(scope)
}

/// The hint file as read once at scan start. The end-of-scan update reuses
/// it instead of reading the file again: disk bytes read are a measured
/// cost, and the file was the only file a report scan read. A hint another
/// process saved in between can be overwritten; hints are advisory only.
#[derive(Default)]
pub struct HintFile {
    hints: Vec<StoredHint>,
}

impl HintFile {
    /// Missing or corrupt files read as an empty cache.
    pub fn read(path: &Path) -> Self {
        Self {
            hints: read_hints(path).unwrap_or_default(),
        }
    }

    pub fn hint(&self, scope: &str) -> Option<CapacityHint> {
        let hint = self.hints.iter().find(|hint| hint.scope == scope)?;
        estimate(scope, hint.files, hint.directories)
    }

    /// See [`save_hint`]; merges into the hints read at scan start.
    pub fn save(
        self,
        path: &Path,
        scope: &str,
        files: usize,
        directories: usize,
    ) -> io::Result<()> {
        write_hints(path, self.hints, scope, files, directories)
    }
}

fn read_hints(path: &Path) -> Option<Vec<StoredHint>> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take((MAX_HINT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_HINT_BYTES {
        return None;
    }
    let (mut cursor, count) = if bytes.get(..8)? == MAGIC {
        (8, 1)
    } else if bytes.get(..8)? == MULTI_MAGIC {
        let count = u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?) as usize;
        (12, count)
    } else {
        return None;
    };
    if count == 0 || count > MAX_HINTS {
        return None;
    }
    let mut hints: Vec<StoredHint> = Vec::with_capacity(count);
    for _ in 0..count {
        let remaining = bytes.get(cursor..)?;
        let files =
            usize::try_from(u64::from_le_bytes(remaining.get(..8)?.try_into().ok()?)).ok()?;
        let directories =
            usize::try_from(u64::from_le_bytes(remaining.get(8..16)?.try_into().ok()?)).ok()?;
        let len = u32::from_le_bytes(remaining.get(16..20)?.try_into().ok()?) as usize;
        if len > MAX_SCOPE {
            return None;
        }
        let scope = std::str::from_utf8(remaining.get(20..20 + len)?).ok()?;
        estimate(scope, files, directories)?;
        if hints.iter().any(|hint| hint.scope == scope) {
            return None;
        }
        hints.push(StoredHint {
            scope: scope.to_owned(),
            files,
            directories,
        });
        cursor += 20 + len;
    }
    if cursor != bytes.len() {
        return None;
    }
    Some(hints)
}

fn estimate(scope: &str, files: usize, directories: usize) -> Option<CapacityHint> {
    if scope.is_empty() || scope.len() > MAX_SCOPE {
        return None;
    }
    let files = files.checked_add(files / 20)?;
    let directories = directories.checked_add(directories / 20)?;
    let bytes = files
        .checked_mul(std::mem::size_of::<winblaze_core::FileRecord>() + 4)?
        .checked_add(
            directories.checked_mul(std::mem::size_of::<winblaze_core::DirectoryRecord>() + 24)?,
        )?;
    (bytes <= MAX_RECORD_STORAGE).then_some(CapacityHint { files, directories })
}

/// Atomically retain up to 32 scopes from successful catalogs. Updating a
/// scope replaces only its own counts; overflow evicts the oldest update.
/// Invalid input leaves previous hints intact. Advice never determines scan
/// correctness or export success, so callers may ignore I/O failure.
pub fn save_hint(path: &Path, scope: &str, files: usize, directories: usize) -> io::Result<()> {
    HintFile::read(path).save(path, scope, files, directories)
}

fn write_hints(
    path: &Path,
    mut hints: Vec<StoredHint>,
    scope: &str,
    files: usize,
    directories: usize,
) -> io::Result<()> {
    estimate(scope, files, directories).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid or excessive allocation estimate",
        )
    })?;
    if let Some(position) = hints.iter().position(|hint| hint.scope == scope) {
        // Keep unchanged durable advice without a write or atomic replacement.
        if hints[position].files == files && hints[position].directories == directories {
            return Ok(());
        }
        hints.remove(position);
    } else if hints.len() == MAX_HINTS {
        hints.remove(0);
    }
    hints.push(StoredHint {
        scope: scope.to_owned(),
        files,
        directories,
    });
    crate::output::atomic_write_with_capacity(path, 4096, |writer| {
        if hints.len() == 1 {
            // Preserve the existing single-scope format and upgrade on demand.
            writer.write_all(MAGIC)?;
        } else {
            writer.write_all(MULTI_MAGIC)?;
            writer.write_all(&(hints.len() as u32).to_le_bytes())?;
        }
        for hint in &hints {
            writer.write_all(&(hint.files as u64).to_le_bytes())?;
            writer.write_all(&(hint.directories as u64).to_le_bytes())?;
            writer.write_all(&(hint.scope.len() as u32).to_le_bytes())?;
            writer.write_all(hint.scope.as_bytes())?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::{load_hint, local_drive_root, save_hint, usage_bucket, HintFile};

    #[test]
    fn a_scan_reads_its_hint_file_once_for_the_estimate_and_the_update() {
        let path = std::env::temp_dir().join(format!("wb-hint-once-{}.bin", std::process::id()));
        save_hint(&path, "C:\\a\0rules", 10_000, 100).unwrap();
        let file = HintFile::read(&path);
        assert_eq!(file.hint("C:\\a\0rules"), load_hint(&path, "C:\\a\0rules"));
        assert!(file.hint("C:\\a\0rules").is_some());
        // Disk bytes read are a benchmarked metric: the update must reuse the
        // hints read at scan start. A second read would see this corruption
        // and drop the first scope.
        std::fs::write(&path, b"not a hint file").unwrap();
        file.save(&path, "C:\\b\0rules", 20_000, 200).unwrap();
        let first = load_hint(&path, "C:\\a\0rules");
        let second = load_hint(&path, "C:\\b\0rules");
        std::fs::remove_file(&path).unwrap();
        assert!(first.is_some(), "the update re-read the hint file");
        assert!(second.is_some());
        // A missing file is an empty cache: nothing to estimate, saves start fresh.
        let missing = HintFile::read(&path);
        assert!(missing.hint("C:\\a\0rules").is_none());
    }

    #[test]
    fn only_local_drive_roots_are_queried_for_volume_usage() {
        // A share can block for the redirector timeout; it has no bucket.
        assert_eq!(local_drive_root(r"\\server\share\folder"), None);
        assert_eq!(local_drive_root(r"\\?\UNC\server\share"), None);
        assert_eq!(local_drive_root("relative"), None);
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let expected = format!(r"{}\", &system[..2]);
        assert_eq!(
            local_drive_root(&format!(r"{system}\Windows")),
            Some(expected.clone())
        );
        assert_eq!(local_drive_root(&format!(r"\\?\{system}\")), Some(expected));
    }

    #[test]
    fn usage_buckets_separate_large_changes_but_not_small_drift() {
        let gib = 1u64 << 30;
        let base = usage_bucket(192 * gib);
        // Writing a report or snapshot (a few hundred MiB) rarely changes it.
        assert!(usage_bucket(192 * gib + 300 * (1 << 20)) - base <= 1);
        // Adding a 10 GiB tree (~5%) always does.
        assert!(usage_bucket(202 * gib) - base >= 2);
    }
}
