//! One directory's direct entries, recorded exactly as the directory walk
//! records them. A refresh that knows which directories changed re-reads
//! only those instead of walking the whole tree again.

use std::io;
use std::path::Path;

use winblaze_core::{DirectoryId, FileId, FileRecord};

const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// A child directory: only its name matters to a refresh, which leaves
/// structural changes to a full walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedDirectory {
    pub name: String,
    /// Junctions, symbolic links and cloud folders: the walk decides whether
    /// to descend them by policy, so a refresh never patches around them.
    pub reparse_or_cloud: bool,
}

/// The direct children of one directory.
#[derive(Debug, Default)]
pub struct DirectoryListing {
    pub directories: Vec<ListedDirectory>,
    /// Records as the walk stores them, with id 0 for the caller to assign.
    /// `allocation_bytes` is the entry's own allocation: the walk charges a
    /// hard-linked file once across the tree, which one directory cannot
    /// know, so callers must check [`file_link_count`] before trusting it.
    pub files: Vec<FileRecord>,
}

/// Reads `directory`'s entries with the walk's own enumeration. Any entry
/// error fails the listing: a partial listing would look like deletions.
pub fn list_directory(directory: &Path, parent: DirectoryId) -> io::Result<DirectoryListing> {
    let entries = crate::winfind::read_dir_auto(directory, true, None)?;
    let mut listing = DirectoryListing::default();
    for entry in entries {
        let entry = entry.map_err(|failure| failure.error)?;
        if entry.attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            listing.directories.push(ListedDirectory {
                reparse_or_cloud: entry.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
                    || entry.cloud_folder,
                name: entry.name.into_display().to_string(),
            });
        } else {
            let allocation_bytes = entry.allocation_bytes;
            listing.files.push(crate::controller::walked_file_record(
                FileId(0),
                parent,
                entry,
                allocation_bytes,
            ));
        }
    }
    Ok(listing)
}

/// Hard links to the file at `path` (1 for an ordinary file).
pub fn file_link_count(path: &Path) -> io::Result<u32> {
    crate::file_details::query_file_details(path).map(|details| details.links)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_records_files_like_the_walk_and_names_child_directories() {
        let root = std::env::temp_dir().join(format!("winblaze-listing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("child")).unwrap();
        std::fs::write(root.join("a.txt"), b"hello").unwrap();
        let listing = list_directory(&root, DirectoryId(7)).unwrap();
        assert_eq!(
            listing.directories,
            vec![ListedDirectory {
                name: "child".into(),
                reparse_or_cloud: false
            }]
        );
        assert_eq!(listing.files.len(), 1);
        let file = &listing.files[0];
        assert_eq!(&*file.name, "a.txt");
        assert_eq!(file.size_bytes, 5);
        assert_eq!(file.parent_directory_id, DirectoryId(7));
        assert!(file.modified_utc().is_some());
        assert_eq!(file_link_count(&root.join("a.txt")).unwrap(), 1);
        std::fs::hard_link(root.join("a.txt"), root.join("b.txt")).unwrap();
        assert_eq!(file_link_count(&root.join("a.txt")).unwrap(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}
