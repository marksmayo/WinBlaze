//! Combines disjoint catalogs without losing their original filesystem paths.
use crate::{BufferedIndexTransaction, TreeIndex};
use std::{collections::HashMap, io};
use winblaze_core::{DirectoryId, DirectoryRecord, FileId, VolumeId};
pub const ROOT_PATH: &str = "WinBlaze:\\Multiple roots";
pub fn combine(scans: Vec<TreeIndex>) -> io::Result<TreeIndex> {
    combine_with(scans, |_, file| file.clone())
}
/// Allows a live-scan caller to reconcile physical metadata across roots.
pub fn combine_with(
    scans: Vec<TreeIndex>,
    mut metadata: impl FnMut(&TreeIndex, &winblaze_core::FileRecord) -> winblaze_core::FileRecord,
) -> io::Result<TreeIndex> {
    if scans.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "No scan roots"));
    }
    let mut roots: Vec<String> = Vec::new();
    for scan in &scans {
        let (root, _) = scan
            .root()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Empty scan root"))?;
        let path = root.full_path.trim_end_matches('\\').to_lowercase();
        if path == ROOT_PATH.to_lowercase()
            || roots.iter().any(|prior| {
                prior == &path
                    || prior.starts_with(&(path.clone() + "\\"))
                    || path.starts_with(&(prior.clone() + "\\"))
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Duplicate or overlapping scan roots",
            ));
        }
        roots.push(path);
    }
    let mut supplemental = Vec::new();
    let mut tx = BufferedIndexTransaction::default();
    tx.reserve(
        scans.iter().map(|scan| scan.files().len()).sum(),
        1 + scans
            .iter()
            .map(|scan| scan.directories().len())
            .sum::<usize>(),
    );
    tx.insert_directory(DirectoryRecord {
        id: DirectoryId(0),
        parent_directory_id: None,
        name: "Multiple roots".into(),
        full_path: ROOT_PATH.into(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    let (mut next_directory, mut next_file, mut next_volume) = (1u64, 1u64, 1u64);
    for scan in scans {
        let mut ids = HashMap::new();
        for directory in scan.directories() {
            ids.insert(directory.id, DirectoryId(next_directory));
            next_directory += 1;
        }
        let source_root = ids[&scan.root().expect("validated root").0.id];
        for volume in scan.volumes() {
            let mut volume = volume.clone();
            volume.id = VolumeId(next_volume);
            next_volume += 1;
            volume.root_directory_id = *ids.get(&volume.root_directory_id).unwrap_or(&source_root);
            tx.insert_volume(volume);
        }
        for directory in scan.directories() {
            let mut directory = directory.clone();
            if directory.parent_directory_id.is_none() {
                directory.name = directory.full_path.clone();
            }
            directory.id = ids[&directory.id];
            directory.parent_directory_id = Some(
                directory
                    .parent_directory_id
                    .and_then(|parent| ids.get(&parent).copied())
                    .unwrap_or(DirectoryId(0)),
            );
            tx.insert_directory(directory);
        }
        for file in scan.files() {
            let mut file = metadata(&scan, file);
            if let Some(value) = scan.file_export_metadata(file.id.0) {
                supplemental.push((next_file, value.clone()));
            }
            file.id = FileId(next_file);
            next_file += 1;
            file.parent_directory_id = *ids
                .get(&file.parent_directory_id)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing file parent"))?;
            tx.insert_file(file);
        }
    }
    let mut combined = TreeIndex::build(tx);
    for (id, value) in supplemental {
        combined.set_file_export_metadata(id, value);
    }
    Ok(combined)
}
