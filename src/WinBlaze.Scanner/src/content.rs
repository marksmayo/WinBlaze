//! Duplicate groups are verified byte-for-byte; metadata alone never proves equality.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HashAlgorithm {
    #[default]
    None,
    Sha256,
    Md5,
    Sha1,
    Sha512,
}
impl HashAlgorithm {
    fn provider(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Sha256 => Some("SHA256"),
            Self::Md5 => Some("MD5"),
            Self::Sha1 => Some("SHA1"),
            Self::Sha512 => Some("SHA512"),
        }
    }
}

#[derive(Default, Debug)]
pub struct DuplicateReport {
    pub groups: Vec<Vec<PathBuf>>,
    pub errors: Vec<(PathBuf, String)>,
    pub reclaimable_bytes: u64,
    pub hashes: BTreeMap<PathBuf, String>,
}

pub fn find_duplicates(paths: impl IntoIterator<Item = PathBuf>) -> io::Result<DuplicateReport> {
    find_duplicates_with_algorithm(paths, HashAlgorithm::None)
}

/// Hashes reduce candidate comparisons; every reported group is still byte-verified.
pub fn find_duplicates_with_algorithm(
    paths: impl IntoIterator<Item = PathBuf>,
    algorithm: HashAlgorithm,
) -> io::Result<DuplicateReport> {
    let mut report = DuplicateReport::default();
    let mut sizes: BTreeMap<u64, Vec<PathBuf>> = BTreeMap::new();
    let mut identities = std::collections::HashSet::new();
    for path in paths {
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if offline_content(&meta) => {
                report.errors.push((
                    path,
                    "Skipped offline/cloud placeholder: content reads could download data".into(),
                ));
            }
            Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
                match crate::file_details::query_file_details(&path) {
                    Ok(details) if details.links <= 1 || identities.insert(details.identity) => {
                        sizes.entry(meta.len()).or_default().push(path)
                    }
                    Ok(_) => {}
                    Err(error) => report.errors.push((path, error.to_string())),
                }
            }
            Ok(_) => {}
            Err(error) => report.errors.push((path, error.to_string())),
        }
    }
    let mut a = Vec::new();
    let mut b = Vec::new();
    for (size, mut candidates) in sizes {
        if candidates.len() < 2 {
            continue;
        }
        candidates.sort();
        candidates.dedup();
        let mut groups: Vec<Vec<PathBuf>> = Vec::new();
        for path in candidates {
            if let Some(provider) = algorithm.provider() {
                match crate::content_hash::file_digest(&path, provider) {
                    Ok(digest) => {
                        report.hashes.insert(path.clone(), digest);
                    }
                    Err(error) => {
                        report.errors.push((path, error.to_string()));
                        continue;
                    }
                }
            }
            let mut destination = None;
            let mut failed = false;
            for (index, group) in groups.iter().enumerate() {
                if algorithm != HashAlgorithm::None
                    && report.hashes.get(&group[0]) != report.hashes.get(&path)
                {
                    continue;
                }
                match equal_contents(&group[0], &path, size, &mut a, &mut b) {
                    Ok(true) => {
                        destination = Some(index);
                        break;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        report.errors.push((path.clone(), error.to_string()));
                        failed = true;
                        break;
                    }
                }
            }
            if failed {
                continue;
            }
            if let Some(index) = destination {
                groups[index].push(path);
            } else {
                groups.push(vec![path]);
            }
        }
        for group in groups.into_iter().filter(|group| group.len() > 1) {
            report.reclaimable_bytes = report
                .reclaimable_bytes
                .saturating_add(size.saturating_mul((group.len() - 1) as u64));
            report.groups.push(group);
        }
    }
    Ok(report)
}

pub(crate) fn offline_content(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // OFFLINE, RECALL_ON_OPEN, RECALL_ON_DATA_ACCESS.
        metadata.file_attributes() & (0x1000 | 0x40000 | 0x400000) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

/// Returns a read-only local content handle. Reparse/cloud attributes are
/// checked both before and after opening; Windows no-recall/reparse flags avoid
/// following a replacement leaf, and sharing denies concurrent writes.
pub fn open_local_file(path: &Path) -> io::Result<File> {
    let reject = |metadata: &std::fs::Metadata| {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            !metadata.is_file()
                || metadata.file_attributes() & (0x400 | 0x1000 | 0x40000 | 0x400000) != 0
        }
        #[cfg(not(windows))]
        {
            !metadata.is_file() || metadata.file_type().is_symlink()
        }
    };
    if reject(&std::fs::symlink_metadata(path)?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Skipped offline/cloud or reparse content",
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000 | 0x00100000);
    }
    let file = options.open(path)?;
    if reject(&file.metadata()?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Content became offline/cloud or reparse",
        ));
    }
    Ok(file)
}

fn equal_contents(
    left: &Path,
    right: &Path,
    expected: u64,
    a: &mut Vec<u8>,
    b: &mut Vec<u8>,
) -> io::Result<bool> {
    let mut left = open_local_file(left)?;
    let mut right = open_local_file(right)?;
    let initial_left = left.metadata()?;
    let initial_right = right.metadata()?;
    if initial_left.len() != expected || initial_right.len() != expected {
        return Err(io::Error::other("File changed during duplicate detection"));
    }
    let capacity = expected.min(64 * 1024) as usize;
    a.resize(capacity, 0);
    b.resize(capacity, 0);
    let mut compared = 0u64;
    loop {
        let remaining = (expected - compared).min(a.len() as u64) as usize;
        if remaining == 0 {
            break;
        }
        left.read_exact(&mut a[..remaining])?;
        right.read_exact(&mut b[..remaining])?;
        if a[..remaining] != b[..remaining] {
            return Ok(false);
        }
        compared += remaining as u64;
    }
    let end_left = left.metadata()?;
    let end_right = right.metadata()?;
    if end_left.len() != expected
        || end_right.len() != expected
        || end_left.modified()? != initial_left.modified()?
        || end_right.modified()? != initial_right.modified()?
    {
        return Err(io::Error::other("File changed during duplicate detection"));
    }
    Ok(true)
}
