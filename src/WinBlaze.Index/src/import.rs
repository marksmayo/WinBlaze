//! Reopen an exported scan without accessing its source filesystem.
use crate::{BufferedIndexTransaction, TreeIndex};
use std::{
    borrow::Cow,
    collections::HashMap,
    io::{self, Read},
};
use winblaze_core::{DirectoryId, DirectoryRecord, FileAttributes, FileId, FileRecord};

// Hash-map probes made while linking rows to directories; counted in tests only.
#[cfg(test)]
thread_local! {
    static ANCESTOR_PROBES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PARENT_PROBES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[inline(always)]
fn count_ancestor_probe() {
    #[cfg(test)]
    ANCESTOR_PROBES.with(|count| count.set(count.get() + 1));
}

#[inline(always)]
fn count_parent_probe() {
    #[cfg(test)]
    PARENT_PROBES.with(|count| count.set(count.get() + 1));
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Hashes import paths a word at a time. Every row's path is hashed at least
/// once (the duplicate check, then directory lookups), and SipHash over 250k
/// long paths was about half of reopening a scan. The seed is random per
/// process, as for the standard hasher, so a crafted file cannot choose its
/// collisions in advance.
#[derive(Clone, Copy)]
struct PathHashState {
    seed: u64,
}

impl PathHashState {
    fn new() -> Self {
        use std::hash::{BuildHasher, Hasher};
        Self {
            seed: std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish(),
        }
    }
}

impl Default for PathHashState {
    fn default() -> Self {
        Self::new()
    }
}

impl std::hash::BuildHasher for PathHashState {
    type Hasher = PathHasher;
    fn build_hasher(&self) -> PathHasher {
        PathHasher(self.seed)
    }
}

struct PathHasher(u64);

impl PathHasher {
    #[inline]
    fn mix(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(23) ^ word).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

impl std::hash::Hasher for PathHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut words = bytes.chunks_exact(8);
        for word in &mut words {
            self.mix(u64::from_le_bytes(word.try_into().expect("8-byte chunk")));
        }
        let rest = words.remainder();
        let mut last = [0u8; 8];
        last[..rest.len()].copy_from_slice(rest);
        self.mix(u64::from_le_bytes(last) ^ ((bytes.len() as u64) << 56));
    }

    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.mix(u64::from(value));
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.mix(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.mix(value as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        // Murmur3's finalizer: the table uses both the low and the top bits.
        let mut hash = self.0;
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        hash ^ (hash >> 33)
    }
}

/// A path compared as `str::to_lowercase` would, allocating only for non-ASCII
/// paths: ASCII lowercasing is byte-wise, and both forms hash the same bytes.
enum FoldedPath<'a> {
    Ascii(&'a str),
    Lowered(String),
}

impl<'a> FoldedPath<'a> {
    fn new(path: &'a str) -> Self {
        if path.is_ascii() {
            Self::Ascii(path)
        } else {
            Self::Lowered(path.to_lowercase())
        }
    }

    fn raw(&self) -> &[u8] {
        match self {
            Self::Ascii(path) => path.as_bytes(),
            Self::Lowered(path) => path.as_bytes(),
        }
    }

    fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        let (bytes, fold) = match self {
            Self::Ascii(path) => (path.as_bytes(), true),
            Self::Lowered(path) => (path.as_bytes(), false),
        };
        bytes
            .iter()
            .map(move |&b| if fold { b.to_ascii_lowercase() } else { b })
    }
}

impl std::hash::Hash for FoldedPath<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Words of ASCII-lowercased bytes. Lowering is a no-op on the output
        // of `to_lowercase`, so an ASCII path and a non-ASCII path that fold
        // to the same text (K and the Kelvin sign) write identical words.
        let bytes = self.raw();
        let mut words = bytes.chunks_exact(8);
        for word in &mut words {
            let mut folded: [u8; 8] = word.try_into().expect("8-byte chunk");
            folded.make_ascii_lowercase();
            state.write_u64(u64::from_le_bytes(folded));
        }
        let rest = words.remainder();
        let mut last = [0u8; 8];
        last[..rest.len()].copy_from_slice(rest);
        last.make_ascii_lowercase();
        state.write_u64(u64::from_le_bytes(last));
        state.write_usize(bytes.len());
    }
}

impl PartialEq for FoldedPath<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ascii(left), Self::Ascii(right)) => left.eq_ignore_ascii_case(right),
            _ => self.bytes().eq(other.bytes()),
        }
    }
}

impl Eq for FoldedPath<'_> {}

fn is_descendant(path: &str, parent: &str) -> bool {
    if path.is_ascii() && parent.is_ascii() {
        return path
            .as_bytes()
            .get(..parent.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(parent.as_bytes()))
            && path.as_bytes().get(parent.len()) == Some(&b'\\');
    }
    path.to_lowercase()
        .starts_with(&(parent.to_lowercase() + "\\"))
}

/// One parsed field: a slice of the input unless unescaping needed a copy.
type Field<'a> = Cow<'a, str>;
type Row<'a> = Vec<Field<'a>>;

fn owned_rows(rows: Vec<Row<'_>>) -> Vec<Vec<String>> {
    rows.into_iter()
        .map(|row| row.into_iter().map(Cow::into_owned).collect())
        .collect()
}

fn field_rows(rows: Vec<Vec<String>>) -> Vec<Row<'static>> {
    rows.into_iter()
        .map(|row| row.into_iter().map(Cow::Owned).collect())
        .collect()
}

fn rows(input: &str, delimiter: char) -> io::Result<Vec<Row<'_>>> {
    // Byte scanning: the delimiter, quote and line breaks are ASCII, so every
    // split point is a UTF-8 boundary and unquoted fields copy as whole slices.
    let delimiter = u8::try_from(delimiter).map_err(|_| invalid("Unsupported CSV delimiter"))?;
    let bytes = input.as_bytes();
    let mut rows = Vec::new();
    let mut row: Row = Vec::with_capacity(7);
    let mut at = 0;
    loop {
        let mut field: Field = Cow::Borrowed("");
        let mut closed = false;
        if bytes.get(at) == Some(&b'"') {
            at += 1;
            loop {
                let Some(quote) = bytes[at..].iter().position(|&b| b == b'"') else {
                    return Err(invalid("Unclosed CSV quote"));
                };
                let piece = &input[at..at + quote];
                at += quote + 1;
                let escaped = bytes.get(at) == Some(&b'"');
                if field.is_empty() && !escaped {
                    // The usual case: the whole quoted field is one input slice.
                    field = Cow::Borrowed(piece);
                } else {
                    field.to_mut().push_str(piece);
                }
                if escaped {
                    field.to_mut().push('"');
                    at += 1;
                } else {
                    break;
                }
            }
            closed = true;
        }
        // Scan to the end of this field.
        let start = at;
        let end_of_row = loop {
            match bytes.get(at) {
                None => break None,
                Some(&b) if b == delimiter => break Some(false),
                Some(b'\n') => break Some(true),
                Some(b'\r') if bytes.get(at + 1) == Some(&b'\n') => break Some(true),
                Some(b'"') => return Err(invalid("Unexpected CSV quote")),
                Some(_) if closed => return Err(invalid("Characters after closing CSV quote")),
                Some(_) => at += 1,
            }
        };
        if !closed {
            field = Cow::Borrowed(&input[start..at]);
        }
        match end_of_row {
            Some(false) => {
                at += 1;
                row.push(field);
            }
            Some(true) => {
                at += if bytes[at] == b'\r' { 2 } else { 1 };
                row.push(field);
                if row.iter().any(|v| !v.is_empty()) {
                    let next_capacity = row.len().max(7);
                    rows.push(std::mem::replace(
                        &mut row,
                        Vec::with_capacity(next_capacity),
                    ));
                } else {
                    row.clear();
                }
            }
            None => {
                if !field.is_empty() || !row.is_empty() || closed {
                    row.push(field);
                    rows.push(row);
                }
                return Ok(rows);
            }
        }
    }
}

/// Imports WinBlaze CSV. Paths and sizes are metadata, never filesystem requests.
pub fn read_csv(input: impl Read) -> io::Result<TreeIndex> {
    read_csv_with_local_time(input, |_| {
        Err(invalid(
            "WizTree dates require the exporting computer's timezone",
        ))
    })
}

/// Time convention selected when reopening a WizTree export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CsvTimeZone {
    #[default]
    Local,
    Utc,
}

/// Converts WizTree's local wall-clock seconds to UTC using the exporting machine's timezone.
pub fn read_csv_with_local_time(
    input: impl Read,
    local_to_utc: impl Fn(i64) -> io::Result<i64>,
) -> io::Result<TreeIndex> {
    read_csv_with_timezone(input, local_to_utc, CsvTimeZone::Local)
}

/// Reopens CSV using an explicit time convention for unlabelled WizTree dates.
pub fn read_csv_with_timezone(
    input: impl Read,
    local_to_utc: impl Fn(i64) -> io::Result<i64>,
    timezone: CsvTimeZone,
) -> io::Result<TreeIndex> {
    read_delimited(input, local_to_utc, timezone, ',')
}

/// Reopens tab-delimited WinBlaze or WizTree scans, including quoted fields.
pub fn read_tsv_with_timezone(
    input: impl Read,
    local_to_utc: impl Fn(i64) -> io::Result<i64>,
    timezone: CsvTimeZone,
) -> io::Result<TreeIndex> {
    read_delimited(input, local_to_utc, timezone, '\t')
}

fn read_delimited(
    mut input: impl Read,
    local_to_utc: impl Fn(i64) -> io::Result<i64>,
    timezone: CsvTimeZone,
    delimiter: char,
) -> io::Result<TreeIndex> {
    let mut text = String::new();
    input.read_to_string(&mut text)?;
    let mut rows = rows(text.trim_start_matches('\u{feff}'), delimiter)?;
    if rows
        .first()
        .and_then(|r| r.first())
        .is_some_and(|v| v.starts_with("Generated by WizTree "))
    {
        rows.remove(0);
    }
    let mut detailed_metadata = HashMap::new();
    let detailed_header = [
        "Path",
        "Kind",
        "LogicalBytes",
        "AllocatedBytes",
        "Files",
        "Directories",
        "ModifiedUnixSeconds",
        "Attributes",
        "CreatedUnixSeconds",
        "AccessedUnixSeconds",
        "Owner",
        "VolumeIdentity",
        "FileIdentity",
        "LinkCount",
    ];
    if rows
        .first()
        .is_some_and(|row| row.iter().map(|value| &**value).eq(detailed_header))
    {
        for row in rows.iter_mut().skip(1) {
            if row.len() != 14 {
                return Err(invalid("Wrong number of detailed CSV fields"));
            }
            if row[1] == "File" {
                let optional_time = |text: &str| -> io::Result<Option<i64>> {
                    if text.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(unix_to_ticks(
                            text.parse()
                                .map_err(|_| invalid("Invalid detailed timestamp"))?,
                        )?))
                    }
                };
                let optional_number = |text: &str| -> io::Result<Option<String>> {
                    if text.is_empty() {
                        Ok(None)
                    } else {
                        text.parse::<u128>()
                            .map_err(|_| invalid("Invalid detailed identity"))?;
                        Ok(Some(text.to_owned()))
                    }
                };
                let volume = optional_number(&row[11])?;
                if volume
                    .as_ref()
                    .is_some_and(|value| value.parse::<u64>().is_err())
                {
                    return Err(invalid("Invalid volume identity"));
                }
                detailed_metadata.insert(
                    row[0].to_string(),
                    ImportedMetadata {
                        attributes: Some(
                            row[7].parse().map_err(|_| invalid("Invalid attributes"))?,
                        ),
                        created: optional_time(&row[8])?,
                        accessed: optional_time(&row[9])?,
                        export: crate::tree::FileExportMetadata {
                            owner: (!row[10].is_empty()).then(|| row[10].to_string()),
                            volume_identity: volume,
                            file_identity: optional_number(&row[12])?,
                            link_count: if row[13].is_empty() {
                                None
                            } else {
                                Some(row[13].parse().map_err(|_| invalid("Invalid link count"))?)
                            },
                        },
                    },
                );
            }
            row.truncate(7);
        }
        rows[0].truncate(7);
    }
    let header = rows.first().ok_or_else(|| invalid("Empty CSV"))?;
    let canonical = [
        "Name",
        "Files",
        "Folders",
        "Logical Size",
        "Physical Size",
        "Attributes",
        "Last Change",
        "WinDirStat Attributes",
        "Index",
    ];
    if canonical.iter().all(|caption| {
        header
            .iter()
            .any(|value| value.eq_ignore_ascii_case(caption))
    }) {
        let mut mapping = Vec::with_capacity(10);
        for caption in canonical {
            let indexes: Vec<_> = header
                .iter()
                .enumerate()
                .filter(|(_, value)| value.eq_ignore_ascii_case(caption))
                .map(|(index, _)| index)
                .collect();
            if indexes.len() != 1 {
                return Err(invalid("Duplicate CSV column"));
            }
            mapping.push(indexes[0]);
        }
        if let Some(owner) = header
            .iter()
            .position(|value| value.eq_ignore_ascii_case("Owner"))
        {
            mapping.push(owner);
        }
        let width = header.len();
        for row in &mut rows {
            if row.len() != width {
                return Err(invalid("Wrong number of CSV fields"));
            }
            *row = mapping
                .iter()
                .map(|&index| std::mem::take(&mut row[index]))
                .collect();
        }
    }
    let header = rows.first().ok_or_else(|| invalid("Empty CSV"))?;
    let first_caption = header.first().map(|value| &**value);
    let foreign_windirstat = first_caption == Some("Name")
        || (matches!(header.len(), 9 | 10)
            && header.iter().any(|caption| caption.contains("WinDirStat"))
            && rows.get(1).and_then(|row| row.get(7)).is_some_and(|flags| {
                flags.starts_with("0x") && u32::from_str_radix(&flags[2..], 16).is_ok()
            }));
    if first_caption == Some("Path")
        && !header.iter().map(|value| &**value).eq([
            "Path",
            "Kind",
            "LogicalBytes",
            "AllocatedBytes",
            "Files",
            "Directories",
            "ModifiedUnixSeconds",
        ])
    {
        return Err(invalid("Invalid WinBlaze CSV header"));
    }
    if !foreign_windirstat && !matches!(first_caption, Some("Path" | "Name" | "File Name")) {
        return Err(invalid("Unsupported CSV schema"));
    }
    let allow_legacy_ticks = first_caption == Some("Path");
    // Foreign schemas are rewritten row by row; WinBlaze's own rows stay borrowed.
    if foreign_windirstat {
        let rows = owned_rows(rows);
        let metadata = windirstat_metadata(&rows);
        return build_import_metadata(field_rows(windirstat(rows)?), allow_legacy_ticks, metadata);
    }
    if first_caption == Some("File Name") {
        let rows = wiztree(owned_rows(rows), &local_to_utc, timezone)?;
        return build_import_metadata(field_rows(rows), allow_legacy_ticks, detailed_metadata);
    }
    build_import_metadata(rows, allow_legacy_ticks, detailed_metadata)
}

#[derive(Default)]
struct ImportedMetadata {
    attributes: Option<u32>,
    created: Option<i64>,
    accessed: Option<i64>,
    export: crate::tree::FileExportMetadata,
}
fn build_import_metadata(
    mut rows: Vec<Row<'_>>,
    allow_legacy_ticks: bool,
    metadata: HashMap<String, ImportedMetadata>,
) -> io::Result<TreeIndex> {
    for row in rows.iter_mut().skip(1) {
        if row.get(1).is_some_and(|kind| kind == "Directory") {
            let len = row[0].trim_end_matches('\\').len();
            match &mut row[0] {
                Cow::Borrowed(path) => *path = &path[..len],
                Cow::Owned(path) => path.truncate(len),
            }
        }
    }
    // Read-only from here: directory keys below are slices of these rows.
    let rows = rows;
    // Value: the id, and whether this path's ancestors are already registered.
    let hash_state = PathHashState::new();
    let mut directories: HashMap<&str, (DirectoryId, bool), PathHashState> =
        HashMap::with_hasher(hash_state);
    let mut seen = std::collections::HashSet::with_capacity_and_hasher(rows.len(), hash_state);
    for row in rows.iter().skip(1) {
        if row.len() != 7 {
            return Err(invalid("Wrong number of CSV fields"));
        }
        if row[0].is_empty() || row[0].contains('\0') || !seen.insert(FoldedPath::new(&row[0])) {
            return Err(invalid("Empty or duplicate CSV path"));
        }
        for column in &row[2..6] {
            column
                .parse::<u64>()
                .map_err(|_| invalid("Invalid byte count or entry count"))?;
        }
        if !row[6].is_empty() {
            row[6]
                .parse::<i64>()
                .map_err(|_| invalid("Invalid timestamp"))?;
        }
        if row[1] == "Directory" {
            let id = DirectoryId(directories.len() as u64 + 1);
            directories.insert(&*row[0], (id, false));
        }
    }
    let multiple = directories.contains_key(crate::multiple_roots::ROOT_PATH);
    let top_roots: std::collections::HashSet<&str, PathHashState> = if multiple {
        directories
            .keys()
            .filter(|path| {
                **path != crate::multiple_roots::ROOT_PATH
                    && !directories.keys().any(|parent| {
                        *parent != crate::multiple_roots::ROOT_PATH && is_descendant(path, parent)
                    })
            })
            .copied()
            .collect()
    } else {
        std::collections::HashSet::with_hasher(hash_state)
    };
    // The common containing directory becomes the root for files-only exports.
    let mut parents = rows.iter().skip(1).map(|r| {
        if r[1] == "Directory" {
            Ok(&*r[0])
        } else {
            r[0].rsplit_once('\\')
                .map(|(p, _)| p)
                .ok_or_else(|| invalid("File has no parent path"))
        }
    });
    let mut common = parents
        .next()
        .ok_or_else(|| invalid("No scan records"))??
        .to_owned();
    if multiple {
        common = crate::multiple_roots::ROOT_PATH.to_owned();
    }
    for parent in parents {
        let parent = parent?;
        if multiple {
            continue;
        }
        while !parent.eq_ignore_ascii_case(&common) && !is_descendant(parent, &common) {
            let len = common
                .rsplit_once('\\')
                .map(|(p, _)| p.len())
                .ok_or_else(|| invalid("CSV spans unrelated volumes"))?;
            common.truncate(len);
        }
    }
    let mut linked = "";
    for row in rows.iter().skip(1) {
        let mut path = if row[1] == "Directory" {
            &*row[0]
        } else {
            row[0].rsplit_once('\\').unwrap().0
        };
        // Consecutive rows usually share a parent that is already linked.
        if !linked.is_empty() && path == linked {
            continue;
        }
        linked = path;
        loop {
            count_ancestor_probe();
            let next_id = DirectoryId(directories.len() as u64 + 1);
            let entry = directories.entry(path).or_insert((next_id, false));
            // A linked directory already registered every ancestor up to the root.
            if std::mem::replace(&mut entry.1, true) {
                break;
            }
            if path.eq_ignore_ascii_case(&common) || top_roots.contains(path) {
                break;
            }
            path = path
                .rsplit_once('\\')
                .ok_or_else(|| invalid("Invalid directory hierarchy"))?
                .0;
        }
    }
    let mut tx = BufferedIndexTransaction::default();
    tx.reserve(
        rows.iter().skip(1).filter(|r| r[1] == "File").count(),
        directories.len(),
    );
    for (&path, &(id, _)) in &directories {
        let parent = path
            .rsplit_once('\\')
            .and_then(|(p, _)| directories.get(p))
            .map(|&(parent, _)| parent)
            .or_else(|| (multiple && path != common).then(|| directories[common.as_str()].0));
        tx.insert_directory(DirectoryRecord {
            id,
            parent_directory_id: parent,
            name: if top_roots.contains(path) {
                path.to_owned()
            } else {
                path.rsplit('\\').next().unwrap_or(path).into()
            },
            full_path: if path.len() == 2 && path.ends_with(':') {
                format!("{path}\\")
            } else {
                path.to_owned()
            },
            direct_bytes: 0,
            total_bytes: 0,
            direct_entries: 0,
            total_entries: 0,
        });
    }
    let mut last_parent: Option<(&str, DirectoryId)> = None;
    for (ordinal, row) in rows.iter().skip(1).enumerate() {
        if row[1] == "Directory" {
            continue;
        }
        if row[1] != "File" {
            return Err(invalid("Unknown record kind"));
        }
        let path: &str = &row[0];
        let (parent_path, name) = path
            .rsplit_once('\\')
            .ok_or_else(|| invalid("File has no parent path"))?;
        // Consecutive files usually share a directory: compare before hashing.
        let parent = match last_parent {
            Some((known, id)) if known == parent_path => id,
            _ => {
                count_parent_probe();
                let id = directories
                    .get(parent_path)
                    .ok_or_else(|| invalid("Missing parent directory"))?
                    .0;
                last_parent = Some((parent_path, id));
                id
            }
        };
        let details = if metadata.is_empty() {
            None
        } else {
            metadata.get(path)
        };
        tx.insert_file(FileRecord {
            id: FileId(ordinal as u64 + 1),
            parent_directory_id: parent,
            name: name.into(),
            // Derived from the parent directory on demand, as for scanned files.
            full_path: Default::default(),
            size_bytes: row[2]
                .parse()
                .map_err(|_| invalid("Invalid logical bytes"))?,
            allocation_bytes: row[3]
                .parse()
                .map_err(|_| invalid("Invalid allocated bytes"))?,
            attributes: details
                .and_then(|value| value.attributes)
                .map(FileAttributes)
                .unwrap_or(FileAttributes::ARCHIVE),
            timestamps: winblaze_core::FileTimestamps::new(
                details.and_then(|value| value.created),
                if row[6].is_empty() {
                    None
                } else {
                    Some(import_timestamp(&row[6], allow_legacy_ticks)?)
                },
                details.and_then(|value| value.accessed),
            ),
        });
    }
    let mut tree = TreeIndex::build(tx);
    if !metadata.is_empty() {
        let supplemental: Vec<_> = tree
            .files()
            .iter()
            .filter_map(|file| {
                metadata
                    .get(&tree.file_display_path(file))
                    .map(|value| (file.id.0, value.export.clone()))
            })
            .collect();
        for (id, value) in supplemental {
            if value.owner.is_some()
                || value.file_identity.is_some()
                || value.volume_identity.is_some()
                || value.link_count.is_some()
            {
                tree.set_file_export_metadata(id, value);
            }
        }
    }
    if tree.root().is_none() {
        return Err(invalid("No scan records"));
    }
    for row in rows.iter().skip(1).filter(|row| row[1] == "Directory") {
        let totals = tree
            .rollup_for(directories[&*row[0]].0 .0)
            .ok_or_else(|| invalid("Disconnected directory"))?;
        if totals.logical_bytes != row[2].parse::<u64>().unwrap()
            || totals.physical_bytes != row[3].parse::<u64>().unwrap()
            || totals.file_count != row[4].parse::<u64>().unwrap()
        {
            return Err(invalid(
                "Incomplete directory-only CSV: export individual files",
            ));
        }
    }
    Ok(tree)
}

// Preserve the installed exporter positional order for translated JSON keys.
struct OrderedJsonFields(Vec<(String, serde_json::Value)>);
impl<'de> serde::Deserialize<'de> for OrderedJsonFields {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> serde::de::Visitor<'de> for Fields {
            type Value = OrderedJsonFields;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("ordered WinDirStat object fields")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut fields = Vec::new();
                while let Some(field) = map.next_entry()? {
                    fields.push(field);
                }
                Ok(OrderedJsonFields(fields))
            }
        }
        deserializer.deserialize_map(Fields)
    }
}
fn localized_windirstat_json(entries: Vec<OrderedJsonFields>) -> io::Result<TreeIndex> {
    let width = entries.first().map_or(9, |entry| entry.0.len());
    let mut rows = vec![vec![String::new(); width]];
    let mut captions = None;
    for OrderedJsonFields(fields) in entries {
        if !matches!(fields.len(), 9 | 10) || !fields[7].0.contains("WinDirStat") {
            return Err(invalid("Unsupported localized WinDirStat JSON schema"));
        }
        let names: Vec<_> = fields.iter().map(|(name, _)| name.clone()).collect();
        if captions.as_ref().is_some_and(|previous| previous != &names) {
            return Err(invalid("Inconsistent localized JSON columns"));
        }
        captions = Some(names);
        let mut row = Vec::with_capacity(fields.len());
        for (index, (_, value)) in fields.into_iter().enumerate() {
            if (1..=4).contains(&index) {
                row.push(
                    value
                        .as_u64()
                        .ok_or_else(|| invalid("Invalid JSON byte/count field"))?
                        .to_string(),
                );
            } else {
                row.push(
                    value
                        .as_str()
                        .ok_or_else(|| invalid("Invalid JSON text field"))?
                        .to_owned(),
                );
            }
        }
        rows.push(row);
    }
    let metadata = windirstat_metadata(&rows);
    build_import_metadata(field_rows(windirstat(rows)?), false, metadata)
}

/// Reopens WinBlaze's versioned JSON or English WinDirStat JSON without filesystem access.
/// Integer sizes are parsed losslessly; unsupported versions are rejected.
pub fn read_json(input: impl Read) -> io::Result<TreeIndex> {
    #[derive(serde::Deserialize)]
    struct Scan {
        schema_version: u32,
        entries: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Entry {
        path: String,
        kind: String,
        logical_bytes: u64,
        allocated_bytes: u64,
        files: u64,
        directories: u64,
        modified_unix_seconds: Option<i64>,
        attributes: Option<u32>,
        created_unix_seconds: Option<i64>,
        accessed_unix_seconds: Option<i64>,
        owner: Option<String>,
        volume_identity: Option<String>,
        file_identity: Option<String>,
        link_count: Option<u32>,
    }
    #[derive(serde::Deserialize)]
    struct WinDirStatEntry {
        #[serde(rename = "Name")]
        path: String,
        #[serde(rename = "Files")]
        files: u64,
        #[serde(rename = "Folders")]
        folders: u64,
        #[serde(rename = "Logical Size")]
        logical: u64,
        #[serde(rename = "Physical Size")]
        physical: u64,
        #[serde(rename = "Attributes")]
        attributes: String,
        #[serde(rename = "Last Change")]
        modified: String,
        #[serde(rename = "WinDirStat Attributes")]
        flags: String,
        #[serde(rename = "Index")]
        index: String,
        #[serde(rename = "Owner", default)]
        owner: Option<String>,
    }
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Document {
        WinBlaze(Scan),
        WinDirStat(Vec<WinDirStatEntry>),
        LocalizedWinDirStat(Vec<OrderedJsonFields>),
    }
    let document: Document =
        serde_json::from_reader(input).map_err(|error| invalid(&error.to_string()))?;
    let document = match document {
        Document::LocalizedWinDirStat(entries) => return localized_windirstat_json(entries),
        Document::WinBlaze(scan) => scan,
        Document::WinDirStat(entries) => {
            let mut rows = Vec::with_capacity(entries.len() + 1);
            // Normalize through the same typed flag/date validation as CSV.
            rows.push(vec![String::new(); 10]);
            for entry in entries {
                u64::from_str_radix(entry.index.trim_start_matches("0x"), 16)
                    .map_err(|_| invalid("Invalid WinDirStat index"))?;
                rows.push(vec![
                    entry.path,
                    entry.files.to_string(),
                    entry.folders.to_string(),
                    entry.logical.to_string(),
                    entry.physical.to_string(),
                    entry.attributes,
                    entry.modified,
                    entry.flags,
                    entry.index,
                    entry.owner.unwrap_or_default(),
                ]);
            }
            let metadata = windirstat_metadata(&rows);
            return build_import_metadata(field_rows(windirstat(rows)?), false, metadata);
        }
    };
    if document.schema_version != 1 {
        return Err(invalid("Unsupported JSON scan version"));
    }
    let mut rows = Vec::with_capacity(document.entries.len() + 1);
    let mut metadata = HashMap::new();
    rows.push(vec!["Path".into()]);
    for entry in document.entries {
        if entry
            .volume_identity
            .as_ref()
            .is_some_and(|value| value.parse::<u64>().is_err())
            || entry
                .file_identity
                .as_ref()
                .is_some_and(|value| value.parse::<u128>().is_err())
        {
            return Err(invalid("Invalid physical identity metadata"));
        }
        if entry.kind == "File" {
            metadata.insert(
                entry.path.clone(),
                ImportedMetadata {
                    attributes: entry.attributes,
                    created: entry.created_unix_seconds.map(unix_to_ticks).transpose()?,
                    accessed: entry.accessed_unix_seconds.map(unix_to_ticks).transpose()?,
                    export: crate::tree::FileExportMetadata {
                        owner: entry.owner,
                        volume_identity: entry.volume_identity,
                        file_identity: entry.file_identity,
                        link_count: entry.link_count,
                    },
                },
            );
        }
        rows.push(vec![
            entry.path,
            entry.kind,
            entry.logical_bytes.to_string(),
            entry.allocated_bytes.to_string(),
            entry.files.to_string(),
            entry.directories.to_string(),
            entry
                .modified_unix_seconds
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ]);
    }
    build_import_metadata(field_rows(rows), false, metadata)
}

fn windirstat_metadata(rows: &[Vec<String>]) -> HashMap<String, ImportedMetadata> {
    rows.iter()
        .skip(1)
        .filter_map(|row| {
            let owner = row.get(9).filter(|value| !value.is_empty())?;
            Some((
                row[0].clone(),
                ImportedMetadata {
                    export: crate::tree::FileExportMetadata {
                        owner: Some(owner.clone()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ))
        })
        .collect()
}

fn windirstat(rows: Vec<Vec<String>>) -> io::Result<Vec<Vec<String>>> {
    // WinDirStat 2.8 writes a fixed positional schema with localized captions.
    // Its optional final owner field is localized too; validate data types below.
    let optional_owner = rows[0].len() == 10;
    let mut out = Vec::with_capacity(rows.len());
    out.push(vec!["Path".into()]);
    for mut row in rows.into_iter().skip(1) {
        if row.len() != if optional_owner { 10 } else { 9 } {
            return Err(invalid("Invalid WinDirStat CSV fields"));
        }
        let flags = u32::from_str_radix(row[7].trim_start_matches("0x"), 16)
            .map_err(|_| invalid("Invalid WinDirStat flags"))?;
        let time = if row[6].is_empty() {
            String::new()
        } else {
            utc_date(&row[6])?.to_string()
        };
        out.push(vec![
            std::mem::take(&mut row[0]),
            if flags & 4 != 0 { "Directory" } else { "File" }.into(),
            std::mem::take(&mut row[3]),
            std::mem::take(&mut row[4]),
            std::mem::take(&mut row[1]),
            std::mem::take(&mut row[2]),
            time,
        ]);
    }
    Ok(out)
}

fn utc_date(text: &str) -> io::Result<i64> {
    let mut fields = text.split(['-', 'T', ':', 'Z']).filter(|s| !s.is_empty());
    let mut numbers = [0i64; 6];
    for number in &mut numbers {
        *number = fields
            .next()
            .ok_or_else(|| invalid("Invalid date"))?
            .parse()
            .map_err(|_| invalid("Invalid date"))?;
    }
    if text.len() != 20 || !text.ends_with('Z') || fields.next().is_some() {
        return Err(invalid("Invalid date"));
    }
    let (year, month, day, hour, minute, second) = (
        numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
    );
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..60).contains(&second)
    {
        return Err(invalid("Invalid date"));
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let maximum = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1601..=9999).contains(&year) || day > maximum {
        return Err(invalid("Invalid calendar date"));
    }
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    Ok(days * 86400 + hour * 3600 + minute * 60 + second)
}

fn wiztree(
    rows: Vec<Vec<String>>,
    local_to_utc: &impl Fn(i64) -> io::Result<i64>,
    timezone: CsvTimeZone,
) -> io::Result<Vec<Vec<String>>> {
    let mut out = Vec::with_capacity(rows.len());
    out.push(vec!["Path".into()]);
    for mut row in rows.into_iter().skip(1) {
        if row.len() < 7 {
            return Err(invalid("Invalid WizTree CSV fields"));
        }
        let flags: u32 = row[4]
            .parse()
            .map_err(|_| invalid("Invalid WizTree attributes"))?;
        let directory = flags & 16 != 0 || row[0].ends_with('\\');
        let path = if directory {
            let len = row[0].trim_end_matches('\\').len();
            row[0].truncate(len);
            std::mem::take(&mut row[0])
        } else {
            std::mem::take(&mut row[0])
        };
        let time = if row[3].is_empty() {
            String::new()
        } else {
            let mut date = String::with_capacity(row[3].len() + 1);
            date.extend(row[3].chars().map(|ch| match ch {
                '/' => '-',
                ' ' => 'T',
                _ => ch,
            }));
            date.push('Z');
            let seconds = utc_date(&date)?;
            match timezone {
                CsvTimeZone::Local => local_to_utc(seconds)?,
                CsvTimeZone::Utc => seconds,
            }
            .to_string()
        };
        out.push(vec![
            path,
            if directory { "Directory" } else { "File" }.into(),
            std::mem::take(&mut row[1]),
            std::mem::take(&mut row[2]),
            std::mem::take(&mut row[5]),
            std::mem::take(&mut row[6]),
            time,
        ]);
    }
    Ok(out)
}

fn unix_to_ticks(seconds: i64) -> io::Result<i64> {
    seconds
        .checked_add(11_644_473_600)
        .and_then(|value| value.checked_mul(10_000_000))
        .filter(|value| *value >= 0)
        .ok_or_else(|| invalid("Timestamp out of Windows date range"))
}

fn import_timestamp(text: &str, allow_legacy_ticks: bool) -> io::Result<i64> {
    let value: i64 = text.parse().map_err(|_| invalid("Invalid timestamp"))?;
    // Earlier WinBlaze exports mislabeled FILETIME ticks as Unix seconds.
    // Values beyond the year-9999 Unix range are unambiguously legacy ticks.
    if allow_legacy_ticks && (253_402_300_800..=2_650_467_743_999_999_999).contains(&value) {
        return Ok(value);
    }
    unix_to_ticks(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\r\n";

    fn deep_export(depth: usize, files: usize) -> (String, String) {
        let mut directory = String::from("C:");
        for level in 0..depth {
            directory.push_str(&format!("\\level{level}"));
        }
        let mut csv = String::from(HEADER);
        csv.push_str(&format!(
            "\"C:\",Directory,{files},{files},{files},{depth},\r\n"
        ));
        for file in 0..files {
            csv.push_str(&format!(
                "\"{directory}\\f{file}.bin\",File,1,1,0,0,1700000000\r\n"
            ));
        }
        (csv, directory)
    }

    fn reset_probes() {
        ANCESTOR_PROBES.with(|count| count.set(0));
        PARENT_PROBES.with(|count| count.set(0));
    }

    #[test]
    fn folded_paths_hash_and_compare_like_lowercased_text() {
        use std::hash::BuildHasher;
        let state = PathHashState::new();
        let hash = |path: &str| state.hash_one(FoldedPath::new(path));
        for (left, right) in [
            ("C:\\Users\\Me\\Report.TXT", "c:\\users\\me\\report.txt"),
            ("Z:\\o\\\u{212A}.txt", "Z:\\o\\k.txt"),
            ("Z:\\EIGHTCHR", "z:\\eightchr"),
        ] {
            assert!(
                FoldedPath::new(left) == FoldedPath::new(right),
                "{left} vs {right}"
            );
            assert_eq!(hash(left), hash(right), "{left} vs {right}");
        }
        for (left, right) in [
            ("C:\\a", "C:\\b"),
            ("C:\\abcdefgh", "C:\\abcdefgh\\x"),
            ("C:\\abcdefgh\u{0}", "C:\\abcdefgh"),
            ("Z:\\o\\\u{c9}.txt", "Z:\\o\\e.txt"),
        ] {
            assert!(
                FoldedPath::new(left) != FoldedPath::new(right),
                "{left} vs {right}"
            );
            assert_ne!(hash(left), hash(right), "{left} vs {right}");
        }
    }

    #[test]
    fn parsed_fields_borrow_the_input_unless_a_quote_is_escaped() {
        let input = "plain,\"quoted text\",\"say \"\"hi\"\"\"\r\n";
        let parsed = rows(input, ',').unwrap();
        let inside = |field: &str| {
            let start = input.as_ptr() as usize;
            (start..start + input.len()).contains(&(field.as_ptr() as usize))
        };
        assert_eq!(parsed[0].len(), 3);
        assert_eq!(&*parsed[0][0], "plain");
        assert_eq!(&*parsed[0][1], "quoted text");
        assert_eq!(&*parsed[0][2], "say \"hi\"");
        assert!(
            inside(&parsed[0][0]),
            "an unquoted field must not be copied"
        );
        assert!(
            inside(&parsed[0][1]),
            "a quoted field without escapes must not be copied"
        );
        assert!(
            !inside(&parsed[0][2]),
            "an escaped quote needs an unescaped copy"
        );
    }

    #[test]
    fn sibling_files_do_not_rewalk_their_shared_ancestors() {
        let (csv, directory) = deep_export(50, 1000);
        reset_probes();
        let tree = read_csv(csv.as_bytes()).unwrap();
        let probes = ANCESTOR_PROBES.with(|count| count.get());
        assert_eq!(tree.files().len(), 1000);
        assert_eq!(tree.directories().len(), 51);
        assert!(
            tree.directories().iter().any(|d| d.full_path == directory),
            "deepest directory must be created from the file paths"
        );
        // One walk up the 50 levels, then at most one probe per further row.
        assert!(probes <= 1000 + 60, "ancestor probes: {probes}");
    }

    #[test]
    fn consecutive_files_reuse_their_parent_lookup() {
        let (csv, _) = deep_export(5, 1000);
        reset_probes();
        read_csv(csv.as_bytes()).unwrap();
        let probes = PARENT_PROBES.with(|count| count.get());
        assert!(probes <= 2, "parent probes: {probes}");
    }

    #[test]
    fn interleaved_parents_still_resolve_each_file_to_its_own_directory() {
        let mut csv = String::from(HEADER);
        csv.push_str("\"C:\\r\",Directory,15,15,4,2,\r\n");
        for (path, bytes) in [("a\\1", 1), ("b\\2", 2), ("a\\3", 4), ("b\\4", 8)] {
            csv.push_str(&format!("\"C:\\r\\{path}\",File,{bytes},{bytes},0,0,\r\n"));
        }
        let tree = read_csv(csv.as_bytes()).unwrap();
        let bytes = |path: &str| {
            let directory = tree
                .directories()
                .iter()
                .find(|d| d.full_path == path)
                .unwrap();
            tree.rollup_for(directory.id.0).unwrap().logical_bytes
        };
        assert_eq!(bytes("C:\\r\\a"), 5);
        assert_eq!(bytes("C:\\r\\b"), 10);
        assert_eq!(bytes("C:\\r"), 15);
    }

    #[test]
    fn a_directory_row_listed_after_its_files_keeps_its_ancestors() {
        // The explicit row for "C:\r\a\deep" is registered before any walk, so
        // a walk that reaches it must still continue up to the root.
        let mut csv = String::from(HEADER);
        csv.push_str("\"C:\\r\",Directory,3,3,2,3,\r\n");
        csv.push_str("\"C:\\r\\a\\deep\\x\\1\",File,1,1,0,0,\r\n");
        csv.push_str("\"C:\\r\\a\\deep\",Directory,3,3,2,1,\r\n");
        csv.push_str("\"C:\\r\\a\\deep\\x\\2\",File,2,2,0,0,\r\n");
        let tree = read_csv(csv.as_bytes()).unwrap();
        let names: Vec<_> = tree
            .directories()
            .iter()
            .map(|d| d.full_path.as_str())
            .collect();
        for path in ["C:\\r", "C:\\r\\a", "C:\\r\\a\\deep", "C:\\r\\a\\deep\\x"] {
            assert!(names.contains(&path), "missing {path}: {names:?}");
        }
        assert_eq!(names.len(), 4);
    }
}
