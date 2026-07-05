use std::hash::{BuildHasherDefault, Hasher};

/// Identity hasher for maps keyed by scanner-assigned ids.
///
/// # Invariant
/// Only sound for keys that are already well-distributed u64 values with no
/// adversarial input: MFT record numbers and the scanner's sequentially
/// assigned `FileId`/`DirectoryId` values. Dense sequential ids map to
/// distinct hash-table buckets by construction, so skipping SipHash is safe
/// there and saves a full hash round per insert/lookup on multi-million-entry
/// maps. Never use this for string-derived or externally supplied keys.
#[derive(Default)]
pub struct IdHasher {
    state: u64,
}

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        // Ids always arrive via write_u64/write_u32; tolerate other widths by
        // folding bytes so the hasher stays total, even if never hit today.
        for &byte in bytes {
            self.state = self.state.rotate_left(8) ^ u64::from(byte);
        }
    }

    fn write_u32(&mut self, value: u32) {
        self.state = u64::from(value);
    }

    fn write_u64(&mut self, value: u64) {
        self.state = value;
    }

    fn write_usize(&mut self, value: usize) {
        self.state = value as u64;
    }
}

pub type BuildIdHasher = BuildHasherDefault<IdHasher>;

/// HashMap keyed by dense scanner ids (see [`IdHasher`] invariant).
pub type IdHashMap<K, V> = std::collections::HashMap<K, V, BuildIdHasher>;

/// HashSet keyed by dense scanner ids (see [`IdHasher`] invariant).
pub type IdHashSet<K> = std::collections::HashSet<K, BuildIdHasher>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_maps_roundtrip_inserts_and_lookups() {
        let mut map: IdHashMap<u64, &str> = IdHashMap::default();
        for id in 0..1024_u64 {
            map.insert(id, "value");
        }
        assert_eq!(map.len(), 1024);
        assert_eq!(map.get(&512), Some(&"value"));
        assert_eq!(map.get(&2048), None);
    }
}
