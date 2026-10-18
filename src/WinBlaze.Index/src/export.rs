//! Streaming UTF-8 CSV export of the complete catalog, independent of UI caps.
use std::io::{self, Write};

use crate::TreeIndex;

fn quoted(output: &mut impl Write, text: &str) -> io::Result<()> {
    output.write_all(b"\"")?;
    escaped(output, text)?;
    output.write_all(b"\"")
}

fn escaped(output: &mut impl Write, text: &str) -> io::Result<()> {
    let mut parts = text.as_bytes().split(|&byte| byte == b'"');
    if let Some(first) = parts.next() {
        output.write_all(first)?;
    }
    for part in parts {
        output.write_all(b"\"\"")?;
        output.write_all(part)?;
    }
    Ok(())
}

fn file_path(
    output: &mut impl Write,
    tree: &TreeIndex,
    file: &winblaze_core::FileRecord,
) -> io::Result<()> {
    if !file.full_path.is_empty() {
        return quoted(output, &file.full_path);
    }
    output.write_all(b"\"")?;
    if let Some(parent) = tree
        .directory_full_path(file.parent_directory_id.0)
        .filter(|parent| !parent.is_empty())
    {
        escaped(output, parent)?;
        if !parent.ends_with('\\') {
            output.write_all(b"\\")?;
        }
    }
    escaped(output, &file.name)?;
    output.write_all(b"\"")
}

/// Writes integer byte counts and Unix UTC seconds, with RFC 4180 quoting/CRLF.
/// Directory counts exclude the exported directory itself. Allocation values
/// retain the scanner's accounting semantics; export does not recalculate them.
pub fn write_csv(tree: &TreeIndex, output: &mut impl Write) -> io::Result<()> {
    write_csv_selected(tree, None, output)
}

/// Writes the same complete catalog as CSV using tab separators.
/// Quoted fields retain literal tabs, commas, quotes, and newlines.
pub fn write_tsv(tree: &TreeIndex, output: &mut impl Write) -> io::Result<()> {
    struct TabWriter<'a, W> {
        output: &'a mut W,
        quoted: bool,
    }
    impl<W: Write> Write for TabWriter<'_, W> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            // Each call is fully written or fails. State crosses writer calls
            // so doubled quotes and split fields retain CSV quoting semantics.
            let mut start = 0;
            for (index, &byte) in bytes.iter().enumerate() {
                if byte == b'"' {
                    self.quoted = !self.quoted;
                } else if byte == b',' && !self.quoted {
                    self.output.write_all(&bytes[start..index])?;
                    self.output.write_all(b"\t")?;
                    start = index + 1;
                }
            }
            self.output.write_all(&bytes[start..])?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.output.flush()
        }
    }
    write_csv(
        tree,
        &mut TabWriter {
            output,
            quoted: false,
        },
    )
}

/// Exports every matching file (no directories/UI cap). Accepts bin, .bin, *.bin.
pub fn write_csv_extension(
    tree: &TreeIndex,
    extension: &str,
    output: &mut impl Write,
) -> io::Result<()> {
    let extension = normalize_export_extension(extension)?;
    write_csv_selected(
        tree,
        Some(&|file: &winblaze_core::FileRecord| {
            file.name
                .rsplit_once('.')
                .is_some_and(|(_, actual)| actual.eq_ignore_ascii_case(extension))
        }),
        output,
    )
}

/// Validates the single-extension export syntax before opening a destination.
pub fn normalize_export_extension(extension: &str) -> io::Result<&str> {
    let extension = extension
        .trim()
        .strip_prefix("*.")
        .or_else(|| extension.trim().strip_prefix('.'))
        .unwrap_or(extension.trim());
    if extension.is_empty()
        || extension
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || "*?;./\\:\"<>|".contains(ch))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Specify one extension, such as bin, .bin, or *.bin",
        ));
    }
    Ok(extension)
}

/// Files-only selection. Patterns are case-insensitive filename wildcards;
/// size uses logical bytes and date bounds are inclusive Unix UTC seconds.
#[derive(Clone, Debug, Default)]
pub struct ExportFilter {
    pub patterns: Vec<String>,
    pub min_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub modified_after_utc: Option<i64>,
    pub modified_before_utc: Option<i64>,
}
impl ExportFilter {
    pub fn validate(&self) -> io::Result<()> {
        if self
            .min_bytes
            .zip(self.max_bytes)
            .is_some_and(|(min, max)| min > max)
            || self
                .modified_after_utc
                .zip(self.modified_before_utc)
                .is_some_and(|(min, max)| min > max)
            || self
                .patterns
                .iter()
                .any(|p| p.is_empty() || p.chars().any(|c| c.is_control() || "\\/:;".contains(c)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid filename patterns or reversed bounds",
            ));
        }
        Ok(())
    }
    fn matches(&self, file: &winblaze_core::FileRecord, compiled: &[Vec<char>]) -> bool {
        self.min_bytes.is_none_or(|min| file.size_bytes >= min)
            && self.max_bytes.is_none_or(|max| file.size_bytes <= max)
            && self.modified_after_utc.is_none_or(|min| {
                file.modified_utc()
                    .is_some_and(|time| unix_seconds(time) >= min)
            })
            && self.modified_before_utc.is_none_or(|max| {
                file.modified_utc()
                    .is_some_and(|time| unix_seconds(time) <= max)
            })
            && (self.patterns.is_empty() || {
                if file.name.is_ascii() && self.patterns.iter().all(|p| p.is_ascii()) {
                    self.patterns
                        .iter()
                        .any(|p| ascii_wildcard(p.as_bytes(), file.name.as_bytes()))
                } else {
                    let lowered = file.name.to_lowercase();
                    let mut name = Vec::with_capacity(lowered.chars().count());
                    name.extend(lowered.chars());
                    compiled.iter().any(|p| unicode_wildcard(p, &name))
                }
            })
    }
}
fn unicode_wildcard(p: &[char], n: &[char]) -> bool {
    let (mut pi, mut ni, mut star, mut retry) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            retry = ni;
        } else if let Some(index) = star {
            retry += 1;
            ni = retry;
            pi = index + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}
// ASCII filenames need neither Unicode folding nor temporary character arrays.
fn ascii_wildcard(p: &[u8], n: &[u8]) -> bool {
    let (mut pi, mut ni, mut star, mut retry) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi].eq_ignore_ascii_case(&n[ni])) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            retry = ni;
        } else if let Some(index) = star {
            retry += 1;
            ni = retry;
            pi = index + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Streams all matching files and validates before writing any output.
pub fn write_csv_filtered(
    tree: &TreeIndex,
    filter: &ExportFilter,
    output: &mut impl Write,
) -> io::Result<()> {
    filter.validate()?;
    let compiled: Vec<Vec<char>> = if tree.files().is_empty() {
        Vec::new()
    } else {
        filter
            .patterns
            .iter()
            .map(|p| p.to_lowercase().chars().collect())
            .collect()
    };
    write_csv_selected(tree, Some(&|file| filter.matches(file, &compiled)), output)
}

fn write_csv_selected(
    tree: &TreeIndex,
    filter: Option<&dyn Fn(&winblaze_core::FileRecord) -> bool>,
    output: &mut impl Write,
) -> io::Result<()> {
    output.write_all(
        b"Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\r\n",
    )?;
    let mut row = Vec::with_capacity(512);
    let mut parent_prefix = Vec::with_capacity(256);
    let mut parent_id = None;
    let root = tree.root().map(|(directory, _)| directory);
    let root_id = root.map(|directory| directory.id);
    for directory in root
        .into_iter()
        .chain(
            tree.directories()
                .iter()
                .filter(|directory| Some(directory.id) != root_id),
        )
        .filter(|_| filter.is_none())
    {
        let rollup = tree.rollup_for(directory.id.0).copied().unwrap_or_default();
        row.clear();
        quoted(&mut row, &directory.full_path)?;
        row.extend_from_slice(b",Directory,");
        for number in [
            rollup.logical_bytes,
            rollup.physical_bytes,
            rollup.file_count,
            rollup.item_count.saturating_sub(rollup.file_count),
        ] {
            csv_integer(&mut row, number);
            row.push(b',');
        }
        csv_timestamp(&mut row, rollup.modified_utc_max);
        output.write_all(&row)?;
    }
    for file in tree
        .files()
        .iter()
        .filter(|file| filter.is_none_or(|matches| matches(file)))
    {
        row.clear();
        if !file.full_path.is_empty() {
            quoted(&mut row, &file.full_path)?;
        } else {
            if parent_id != Some(file.parent_directory_id.0) {
                parent_prefix.clear();
                parent_prefix.push(b'"');
                if let Some(parent) = tree
                    .directory_full_path(file.parent_directory_id.0)
                    .filter(|parent| !parent.is_empty())
                {
                    escaped(&mut parent_prefix, parent)?;
                    if !parent.ends_with('\\') {
                        parent_prefix.push(b'\\');
                    }
                }
                parent_id = Some(file.parent_directory_id.0);
            }
            row.extend_from_slice(&parent_prefix);
            escaped(&mut row, &file.name)?;
            row.push(b'"');
        }
        row.extend_from_slice(b",File,");
        csv_integer(&mut row, file.size_bytes);
        row.push(b',');
        csv_integer(&mut row, file.allocation_bytes);
        row.extend_from_slice(b",0,0,");
        csv_timestamp(&mut row, file.modified_utc());
        output.write_all(&row)?;
    }
    output.flush()
}

fn csv_integer(row: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    row.extend_from_slice(&digits[start..]);
}

fn csv_timestamp(row: &mut Vec<u8>, value: Option<i64>) {
    if let Some(value) = value {
        let value = unix_seconds(value);
        if value < 0 {
            row.push(b'-');
        }
        csv_integer(row, value.unsigned_abs());
    }
    row.extend_from_slice(b"\r\n");
}

fn json_string(output: &mut impl Write, text: &str) -> io::Result<()> {
    output.write_all(b"\"")?;
    json_escaped(output, text)?;
    output.write_all(b"\"")
}

fn json_escaped(output: &mut impl Write, text: &str) -> io::Result<()> {
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte < 32 || byte == b'"' || byte == b'\\' {
            output.write_all(&text.as_bytes()[start..index])?;
            match byte {
                b'"' => output.write_all(b"\\\"")?,
                b'\\' => output.write_all(b"\\\\")?,
                b'\n' => output.write_all(b"\\n")?,
                b'\r' => output.write_all(b"\\r")?,
                b'\t' => output.write_all(b"\\t")?,
                _ => write!(output, "\\u{byte:04x}")?,
            }
            start = index + 1;
        }
    }
    output.write_all(&text.as_bytes()[start..])
}

/// Exports the full catalog as schema version 1, with integer bytes/UTC seconds.
/// Missing timestamps are null. No UI display limit is applied.
pub fn write_json(tree: &TreeIndex, output: &mut impl Write) -> io::Result<()> {
    write_json_with_metadata(tree, output, |file| {
        Ok(tree.file_export_metadata(file.id.0).cloned())
    })
}
/// Optional metadata provider. Ordinary exports perform no filesystem queries.
pub fn write_json_with_metadata(
    tree: &TreeIndex,
    output: &mut impl Write,
    mut metadata: impl FnMut(
        &winblaze_core::FileRecord,
    ) -> io::Result<Option<crate::tree::FileExportMetadata>>,
) -> io::Result<()> {
    output.write_all(b"{\"schema_version\":1,\"entries\":[")?;
    let root = tree.root().map(|(directory, _)| directory);
    let root_id = root.map(|directory| directory.id);
    // Each entry is built in one reused buffer and written once: integer
    // formatting and many small writes per record dominated large exports.
    let mut row = Vec::with_capacity(512);
    for directory in root
        .into_iter()
        .chain(tree.directories().iter().filter(|d| Some(d.id) != root_id))
    {
        if !row.is_empty() {
            output.write_all(&row)?;
            row.clear();
            row.push(b',');
        }
        let totals = tree.rollup_for(directory.id.0).copied().unwrap_or_default();
        row.extend_from_slice(b"{\"Path\":");
        json_string(&mut row, &directory.full_path)?;
        row.extend_from_slice(b",\"Kind\":\"Directory\",\"LogicalBytes\":");
        csv_integer(&mut row, totals.logical_bytes);
        row.extend_from_slice(b",\"AllocatedBytes\":");
        csv_integer(&mut row, totals.physical_bytes);
        row.extend_from_slice(b",\"Files\":");
        csv_integer(&mut row, totals.file_count);
        row.extend_from_slice(b",\"Directories\":");
        csv_integer(
            &mut row,
            totals.item_count.saturating_sub(totals.file_count),
        );
        row.extend_from_slice(b",\"ModifiedUnixSeconds\":");
        json_seconds(&mut row, totals.modified_utc_max);
        row.push(b'}');
    }
    for file in tree.files() {
        if !row.is_empty() {
            output.write_all(&row)?;
            row.clear();
            row.push(b',');
        }
        row.extend_from_slice(b"{\"Path\":");
        if !file.full_path.is_empty() {
            json_string(&mut row, &file.full_path)?;
        } else {
            row.push(b'"');
            if let Some(parent) = tree
                .directory_full_path(file.parent_directory_id.0)
                .filter(|p| !p.is_empty())
            {
                json_escaped(&mut row, parent.trim_end_matches('\\'))?;
                row.extend_from_slice(b"\\\\");
            }
            json_escaped(&mut row, &file.name)?;
            row.push(b'"');
        }
        row.extend_from_slice(b",\"Kind\":\"File\",\"LogicalBytes\":");
        csv_integer(&mut row, file.size_bytes);
        row.extend_from_slice(b",\"AllocatedBytes\":");
        csv_integer(&mut row, file.allocation_bytes);
        row.extend_from_slice(b",\"Files\":0,\"Directories\":0,\"ModifiedUnixSeconds\":");
        json_seconds(&mut row, file.modified_utc());
        row.extend_from_slice(b",\"Attributes\":");
        csv_integer(&mut row, u64::from(file.attributes.0));
        row.extend_from_slice(b",\"CreatedUnixSeconds\":");
        json_seconds(&mut row, file.created_utc());
        row.extend_from_slice(b",\"AccessedUnixSeconds\":");
        json_seconds(&mut row, file.accessed_utc());
        if let Some(metadata) = metadata(file)? {
            for (name, value) in [
                (&b",\"Owner\":"[..], &metadata.owner),
                (&b",\"VolumeIdentity\":"[..], &metadata.volume_identity),
                (&b",\"FileIdentity\":"[..], &metadata.file_identity),
            ] {
                if let Some(value) = value {
                    row.extend_from_slice(name);
                    json_string(&mut row, value)?;
                }
            }
            if let Some(count) = metadata.link_count {
                row.extend_from_slice(b",\"LinkCount\":");
                csv_integer(&mut row, u64::from(count));
            }
        }
        row.push(b'}');
    }
    output.write_all(&row)?;
    output.write_all(b"]}\n")?;
    output.flush()
}

/// Unix seconds for a FILETIME tick count, or `null` when absent.
fn json_seconds(row: &mut Vec<u8>, value: Option<i64>) {
    match value {
        Some(value) => signed_integer(row, unix_seconds(value)),
        None => row.extend_from_slice(b"null"),
    }
}

/// Unix seconds for a FILETIME tick count; nothing when absent.
fn optional_seconds(row: &mut Vec<u8>, value: Option<i64>) {
    if let Some(value) = value {
        signed_integer(row, unix_seconds(value));
    }
}

fn signed_integer(row: &mut Vec<u8>, value: i64) {
    if value < 0 {
        row.push(b'-');
    }
    csv_integer(row, value.unsigned_abs());
}

pub(crate) fn unix_seconds(ticks: i64) -> i64 {
    ticks.div_euclid(10_000_000) - 11_644_473_600
}

/// Complete RFC 4180 catalog with optional owner and lossless physical identities.
/// Provider errors propagate; the native adapter handles atomic publication.
pub fn write_csv_with_metadata(
    tree: &TreeIndex,
    output: &mut impl Write,
    mut metadata: impl FnMut(
        &winblaze_core::FileRecord,
    ) -> io::Result<Option<crate::tree::FileExportMetadata>>,
) -> io::Result<()> {
    output.write_all(b"Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds,Attributes,CreatedUnixSeconds,AccessedUnixSeconds,Owner,VolumeIdentity,FileIdentity,LinkCount\r\n")?;
    let root = tree.root().map(|(directory, _)| directory);
    let root_id = root.map(|directory| directory.id);
    // One reused buffer and one write per row, as in the plain CSV export.
    let mut row = Vec::with_capacity(512);
    for directory in root
        .into_iter()
        .chain(tree.directories().iter().filter(|d| Some(d.id) != root_id))
    {
        let totals = tree.rollup_for(directory.id.0).copied().unwrap_or_default();
        row.clear();
        quoted(&mut row, &directory.full_path)?;
        row.extend_from_slice(b",Directory,");
        for number in [
            totals.logical_bytes,
            totals.physical_bytes,
            totals.file_count,
            totals.item_count.saturating_sub(totals.file_count),
        ] {
            csv_integer(&mut row, number);
            row.push(b',');
        }
        optional_seconds(&mut row, totals.modified_utc_max);
        row.extend_from_slice(b",,,,,,,\r\n");
        output.write_all(&row)?;
    }
    for file in tree.files() {
        row.clear();
        file_path(&mut row, tree, file)?;
        row.extend_from_slice(b",File,");
        csv_integer(&mut row, file.size_bytes);
        row.push(b',');
        csv_integer(&mut row, file.allocation_bytes);
        row.extend_from_slice(b",0,0,");
        optional_seconds(&mut row, file.modified_utc());
        row.push(b',');
        csv_integer(&mut row, u64::from(file.attributes.0));
        row.push(b',');
        optional_seconds(&mut row, file.created_utc());
        row.push(b',');
        optional_seconds(&mut row, file.accessed_utc());
        let extras = metadata(file)?.unwrap_or_default();
        for value in [
            &extras.owner,
            &extras.volume_identity,
            &extras.file_identity,
        ] {
            row.push(b',');
            if let Some(value) = value {
                quoted(&mut row, value)?;
            }
        }
        row.push(b',');
        if let Some(count) = extras.link_count {
            csv_integer(&mut row, u64::from(count));
        }
        row.extend_from_slice(b"\r\n");
        output.write_all(&row)?;
    }
    output.flush()
}
