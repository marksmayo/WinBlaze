//! Compact directory-ID bookkeeping with transferable owned records.
use winblaze_core::{DirectoryId, DirectoryRecord, IdHashMap};

#[derive(Clone, Debug, Default)]
pub(crate) struct DirectoryRecords {
    positions: IdHashMap<DirectoryId, usize>,
    records: Vec<DirectoryRecord>,
}

impl DirectoryRecords {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            positions: IdHashMap::with_capacity_and_hasher(capacity, Default::default()),
            records: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn reserve(&mut self, total: usize) {
        let additional = total.saturating_sub(self.records.len());
        self.positions.reserve(additional);
        self.records.reserve(additional);
    }

    pub(crate) fn insert(&mut self, id: DirectoryId, record: DirectoryRecord) {
        use std::collections::hash_map::Entry;
        match self.positions.entry(id) {
            Entry::Occupied(entry) => self.records[*entry.get()] = record,
            Entry::Vacant(entry) => {
                entry.insert(self.records.len());
                self.records.push(record);
            }
        }
    }

    pub(crate) fn get(&self, id: &DirectoryId) -> Option<&DirectoryRecord> {
        self.positions.get(id).map(|&index| &self.records[index])
    }

    pub(crate) fn values(&self) -> std::slice::Iter<'_, DirectoryRecord> {
        self.records.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn into_records(self) -> Vec<DirectoryRecord> {
        let Self {
            positions,
            mut records,
        } = self;
        drop(positions);
        records.shrink_to_fit();
        records
    }
}

impl FromIterator<(DirectoryId, DirectoryRecord)> for DirectoryRecords {
    fn from_iter<T: IntoIterator<Item = (DirectoryId, DirectoryRecord)>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut records = Self::with_capacity(iter.size_hint().0);
        for (id, record) in iter {
            records.insert(id, record);
        }
        records
    }
}

// Public transaction equality remains independent of directory insertion order.
impl PartialEq for DirectoryRecords {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .records
                .iter()
                .all(|record| other.get(&record.id) == Some(record))
    }
}
impl Eq for DirectoryRecords {}
