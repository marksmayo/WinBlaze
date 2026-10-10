//! Owned file records with compact ID bookkeeping and a transferable arena.
use winblaze_core::{FileId, FileRecord, IdHashMap};

#[derive(Clone, Debug)]
enum FilePositions {
    Dense(Vec<u32>, usize),
    Sparse(IdHashMap<FileId, usize>),
}

impl Default for FilePositions {
    fn default() -> Self {
        Self::Dense(Vec::new(), 0)
    }
}

impl FilePositions {
    fn with_capacity(capacity: usize) -> Self {
        Self::Dense(
            Vec::with_capacity(if capacity == 0 {
                0
            } else {
                capacity.saturating_add(1)
            }),
            capacity,
        )
    }

    fn reserve(&mut self, total: usize, live: usize) {
        match self {
            Self::Dense(slots, hint) if total > 0 => {
                *hint = (*hint).max(total);
                slots.reserve(total.saturating_add(1).saturating_sub(slots.len()));
            }
            Self::Dense(_, _) => {}
            Self::Sparse(positions) => positions.reserve(total.saturating_sub(live)),
        }
    }

    fn get(&self, id: &FileId) -> Option<usize> {
        match self {
            Self::Dense(slots, _) => {
                let encoded = *slots.get(usize::try_from(id.0).ok()?)?;
                encoded.checked_sub(1).map(|position| position as usize)
            }
            Self::Sparse(positions) => positions.get(id).copied(),
        }
    }

    fn insert(&mut self, id: FileId, position: usize, live: usize) {
        match self {
            Self::Dense(slots, hint) => {
                let index = usize::try_from(id.0).ok();
                let encoded = position
                    .checked_add(1)
                    .and_then(|value| u32::try_from(value).ok());
                let limit = slots
                    .capacity()
                    .max(hint.saturating_add(1))
                    // Concurrent worker batches can arrive before earlier
                    // IDs. Permit a bounded early hole (at most 64 KiB of
                    // slots), rather than permanently promoting the entire
                    // walk catalog to a sparse map for a delayed first batch.
                    .max(live.saturating_mul(4).saturating_add(16_384));
                if let (Some(index), Some(encoded)) = (index, encoded) {
                    if index < limit {
                        if index >= slots.len() {
                            slots.resize(index + 1, 0);
                        }
                        slots[index] = encoded;
                        return;
                    }
                }
            }
            Self::Sparse(positions) => {
                positions.insert(id, position);
                return;
            }
        }
        // Sparse or unrepresentable IDs retain the original map semantics.
        // Build the replacement before releasing the dense lookup, preserving
        // every existing entry, including zero IDs and holes after removal.
        let capacity = match self {
            Self::Dense(_, hint) => live.max(*hint),
            Self::Sparse(_) => unreachable!("sparse insertion returned above"),
        };
        let mut positions = IdHashMap::with_capacity_and_hasher(capacity, Default::default());
        if let Self::Dense(slots, _) = self {
            for (id, &encoded) in slots.iter().enumerate() {
                if let Some(position) = encoded.checked_sub(1) {
                    positions.insert(FileId(id as u64), position as usize);
                }
            }
        }
        positions.insert(id, position);
        *self = Self::Sparse(positions);
    }

    fn remove(&mut self, id: &FileId) -> Option<usize> {
        match self {
            Self::Dense(slots, _) => {
                let encoded = std::mem::take(slots.get_mut(usize::try_from(id.0).ok()?)?);
                encoded.checked_sub(1).map(|position| position as usize)
            }
            Self::Sparse(positions) => positions.remove(id),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FileRecords {
    positions: FilePositions,
    records: Vec<FileRecord>,
}

impl FileRecords {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            positions: FilePositions::with_capacity(capacity),
            records: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn reserve(&mut self, total: usize) {
        let additional = total.saturating_sub(self.records.len());
        self.positions.reserve(total, self.records.len());
        self.records.reserve(additional);
    }

    pub(crate) fn insert(&mut self, id: FileId, record: FileRecord) {
        if let Some(position) = self.positions.get(&id) {
            self.records[position] = record;
        } else {
            let position = self.records.len();
            // Large walk scans have no reliable total estimate. Limit their
            // geometric slack while preserving amortized growth; small arenas
            // keep Vec's normal growth policy. Grow by about 60%, rather than
            // many small steps that increase cumulative allocator work. The
            // publication path independently enforces its spare-capacity budget.
            let capacity = self.records.capacity();
            if position == capacity && capacity >= 65_536 {
                self.records.reserve_exact((capacity / 5).saturating_mul(3));
            }
            self.records.push(record);
            self.positions.insert(id, position, self.records.len());
        }
    }

    pub(crate) fn remove(&mut self, id: &FileId) {
        if let Some(position) = self.positions.remove(id) {
            self.records.swap_remove(position);
            if let Some(moved) = self.records.get(position) {
                self.positions
                    .insert(moved.id, position, self.records.len());
            }
        }
    }

    pub(crate) fn values(&self) -> std::slice::Iter<'_, FileRecord> {
        self.records.iter()
    }

    pub(crate) fn into_records(self) -> Vec<FileRecord> {
        let Self {
            positions,
            mut records,
        } = self;
        // Release lookup storage before asking the allocator to shrink the
        // transferred arena. Geometric growth slack otherwise survives for
        // the entire lifetime of the published catalog.
        drop(positions);
        // A shrink can allocate a second volume-sized arena. Keep bounded
        // spare capacity instead; explicit overestimates and small geometrically
        // grown arenas still shed excessive slack before publication.
        if records.capacity().saturating_sub(records.len()) > records.len() / 8 {
            records.shrink_to_fit();
        }
        records
    }
}

impl FromIterator<(FileId, FileRecord)> for FileRecords {
    fn from_iter<T: IntoIterator<Item = (FileId, FileRecord)>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut records = Self::with_capacity(iter.size_hint().0);
        for (id, record) in iter {
            records.insert(id, record);
        }
        records
    }
}

// Transaction equality has always been independent of insertion order.
impl PartialEq for FileRecords {
    fn eq(&self, other: &Self) -> bool {
        self.records.len() == other.records.len()
            && self.records.iter().all(|record| {
                other
                    .positions
                    .get(&record.id)
                    .is_some_and(|position| other.records[position] == *record)
            })
    }
}
impl Eq for FileRecords {}
