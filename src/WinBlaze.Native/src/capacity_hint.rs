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
    format!("{root}\0{:016x}", hash.finish())
}

/// Read a bounded advisory cache and return the exact scope's estimate with
/// 5% room for growth. Missing, corrupt or unknown scopes are cache misses.
pub fn load_hint(path: &Path, scope: &str) -> Option<CapacityHint> {
    let hints = read_hints(path)?;
    let hint = hints.iter().find(|hint| hint.scope == scope)?;
    estimate(scope, hint.files, hint.directories)
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
    estimate(scope, files, directories).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid or excessive allocation estimate",
        )
    })?;
    let mut hints = read_hints(path).unwrap_or_default();
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
