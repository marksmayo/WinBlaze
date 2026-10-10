//! Assign physical bytes once per volume/file identity, keeping logical aliases.
use std::collections::{HashMap, HashSet};
#[derive(Default)]
pub struct AllocationTracker {
    // Most scans stay on one volume. Keep its identity sets directly owned,
    // avoiding an outer hash lookup/allocation for each shard's common case.
    volumes: TrackedVolumes,
}
#[derive(Default)]
enum TrackedVolumes {
    #[default]
    Empty,
    Single(u64, VolumeIdentities),
    Multiple(HashMap<u64, VolumeIdentities>),
}
#[derive(Default)]
struct VolumeIdentities {
    narrow: NarrowIdentities,
    wide: HashSet<u128>,
}
// Clustered file references can be represented exactly by bits. Scattered
// references retain the existing hash set, rather than allocating a huge range.
struct NarrowIdentities {
    sparse: HashSet<u64>,
    dense: Option<DenseIdentities>,
    next_compaction: usize,
}
struct DenseIdentities {
    first: u64,
    words: Vec<u64>,
    count: usize,
}
impl Default for NarrowIdentities {
    fn default() -> Self {
        Self {
            sparse: HashSet::new(),
            dense: None,
            next_compaction: 4096,
        }
    }
}
impl NarrowIdentities {
    fn insert(&mut self, file: u64) -> bool {
        if let Some(dense) = &mut self.dense {
            if let Some(offset) = file.checked_sub(dense.first) {
                if let Some(word) = usize::try_from(offset / 64)
                    .ok()
                    .and_then(|index| dense.words.get_mut(index))
                {
                    let bit = 1 << (offset % 64);
                    let first = *word & bit == 0;
                    *word |= bit;
                    dense.count += usize::from(first);
                    return first;
                }
            }
        }
        let first = self.sparse.insert(file);
        if first && self.sparse.len() >= self.next_compaction {
            self.compact();
        }
        first
    }

    fn compact(&mut self) {
        // Select a majority region without allocating a histogram or sorting.
        // Distant sequence values must not defeat an otherwise compact cluster.
        let prefix = if let Some(dense) = &self.dense {
            dense.first >> 32
        } else {
            let (mut candidate, mut votes) = (0, 0usize);
            for file in &self.sparse {
                let prefix = file >> 32;
                if votes == 0 {
                    candidate = prefix;
                    votes = 1;
                } else if prefix == candidate {
                    votes += 1;
                } else {
                    votes -= 1;
                }
            }
            candidate
        };
        let (mut first, mut last, mut count) = (u64::MAX, 0, 0usize);
        for &file in &self.sparse {
            if file >> 32 == prefix {
                first = first.min(file & !63);
                last = last.max(file);
                count += 1;
            }
        }
        if count == 0 || (self.dense.is_none() && count <= self.sparse.len() / 2) {
            self.next_compaction = self.sparse.len().saturating_mul(2);
            return;
        }
        if let Some(dense) = &self.dense {
            first = first.min(dense.first);
            last = last.max(dense.first + (dense.words.len() as u64 * 64 - 1));
            count += dense.count;
        }
        let words = ((u128::from(last) - u128::from(first)) / 64 + 1).next_power_of_two();
        // Allow geometric spare capacity, including the 64-way shard stride.
        // Cap each window at 1 MiB; arbitrary distant references stay sparse.
        if words > count as u128 * 2 || words > 131_072 {
            self.next_compaction = self.sparse.len().saturating_mul(2);
            return;
        }
        if u128::from(first) + words * 64 > 1u128 << 64 {
            first = ((1u128 << 64) - words * 64) as u64;
        }
        let mut words = vec![0; words as usize];
        let mut retained_count = 0;
        if let Some(dense) = self.dense.take() {
            let offset = ((dense.first - first) / 64) as usize;
            words[offset..offset + dense.words.len()].copy_from_slice(&dense.words);
            retained_count = dense.count;
        }
        self.sparse.retain(|&file| {
            if let Some(offset) = file.checked_sub(first) {
                if let Some(word) = usize::try_from(offset / 64)
                    .ok()
                    .and_then(|index| words.get_mut(index))
                {
                    *word |= 1 << (offset % 64);
                    retained_count += 1;
                    return false;
                }
            }
            true
        });
        self.sparse.shrink_to_fit();
        self.dense = Some(DenseIdentities {
            first,
            words,
            count: retained_count,
        });
        // Geometric growth bounds rebuilding work on increasing file IDs.
        self.next_compaction = self.sparse.len().max(1).saturating_mul(2);
    }
}
impl VolumeIdentities {
    fn claim(&mut self, file: u128, bytes: u64) -> u64 {
        let first = if file <= u64::MAX as u128 {
            self.narrow.insert(file as u64)
        } else {
            self.wide.insert(file)
        };
        if first {
            bytes
        } else {
            0
        }
    }
}
impl AllocationTracker {
    pub fn claim(&mut self, (volume, file): (u64, u128), bytes: u64) -> u64 {
        if bytes == 0 {
            return 0;
        }
        match &mut self.volumes {
            TrackedVolumes::Empty => {
                let mut identities = VolumeIdentities::default();
                let claimed = identities.claim(file, bytes);
                self.volumes = TrackedVolumes::Single(volume, identities);
                claimed
            }
            TrackedVolumes::Single(primary, identities) if *primary == volume => {
                identities.claim(file, bytes)
            }
            TrackedVolumes::Multiple(volumes) => {
                volumes.entry(volume).or_default().claim(file, bytes)
            }
            TrackedVolumes::Single(..) => {
                // Promote once: mixed-volume traffic gets the same lookup path
                // for every volume, and all earlier claims move intact.
                let TrackedVolumes::Single(primary, identities) = std::mem::take(&mut self.volumes)
                else {
                    unreachable!("single-volume promotion")
                };
                let mut volumes = HashMap::from([(primary, identities)]);
                let claimed = volumes.entry(volume).or_default().claim(file, bytes);
                self.volumes = TrackedVolumes::Multiple(volumes);
                claimed
            }
        }
    }
}
/// Independent identity shards avoid a scan-wide lock on every file.
pub struct ConcurrentAllocationTracker {
    shards: [IdentityShard; 64],
}
// Keep neighboring shards' locks and mutable inline sets on separate lines.
// The complete retained-storage regression includes this alignment overhead.
#[repr(align(64))]
struct IdentityShard(std::sync::Mutex<AllocationTracker>);
impl Default for ConcurrentAllocationTracker {
    fn default() -> Self {
        Self {
            shards: std::array::from_fn(|_| {
                IdentityShard(std::sync::Mutex::new(AllocationTracker::default()))
            }),
        }
    }
}
impl ConcurrentAllocationTracker {
    pub fn claim(&self, identity: (u64, u128), bytes: u64) -> u64 {
        if bytes == 0 {
            return 0;
        }
        let index = ((identity.1 as u64) ^ ((identity.1 >> 64) as u64) ^ identity.0) as usize & 63;
        self.shards[index]
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .claim(identity, bytes)
    }
}
