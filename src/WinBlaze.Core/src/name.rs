//! Owned display names with no heap allocation for ordinary short filenames.
use std::{fmt, ops::Deref};

/// Immutable UTF-8 name. Long names retain exact owned storage.
#[derive(Clone, PartialEq, Eq)]
pub struct FileName(Storage);

#[derive(Clone, PartialEq, Eq)]
enum Storage {
    Inline { bytes: [u8; 22], length: u8 },
    Heap(Box<str>),
}

impl From<&str> for FileName {
    fn from(name: &str) -> Self {
        if name.len() <= 22 {
            let mut bytes = [0; 22];
            bytes[..name.len()].copy_from_slice(name.as_bytes());
            Self(Storage::Inline {
                bytes,
                length: name.len() as u8,
            })
        } else {
            Self(Storage::Heap(name.into()))
        }
    }
}

impl From<String> for FileName {
    fn from(name: String) -> Self {
        if name.len() <= 22 {
            Self::from(name.as_str())
        } else {
            Self(Storage::Heap(name.into()))
        }
    }
}

impl From<Box<str>> for FileName {
    fn from(name: Box<str>) -> Self {
        if name.len() <= 22 {
            Self::from(name.as_ref())
        } else {
            Self(Storage::Heap(name))
        }
    }
}

impl Default for FileName {
    fn default() -> Self {
        Self::from("")
    }
}

impl Deref for FileName {
    type Target = str;
    fn deref(&self) -> &str {
        match &self.0 {
            Storage::Inline { bytes, length } => {
                std::str::from_utf8(&bytes[..usize::from(*length)]).expect("owned name is UTF-8")
            }
            Storage::Heap(name) => name,
        }
    }
}

impl FileName {
    /// The name as a string slice (mirrors `String::as_str`).
    pub fn as_str(&self) -> &str {
        self
    }
}

impl AsRef<str> for FileName {
    fn as_ref(&self) -> &str {
        self
    }
}

impl From<FileName> for String {
    fn from(name: FileName) -> Self {
        match name.0 {
            Storage::Heap(name) => name.into_string(),
            Storage::Inline { .. } => name.to_string(),
        }
    }
}

impl PartialEq<String> for FileName {
    fn eq(&self, other: &String) -> bool {
        &**self == other.as_str()
    }
}

impl std::borrow::Borrow<str> for FileName {
    fn borrow(&self) -> &str {
        self
    }
}

impl std::hash::Hash for FileName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&**self, state);
    }
}

impl fmt::Display for FileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

impl fmt::Debug for FileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl PartialEq<str> for FileName {
    fn eq(&self, other: &str) -> bool {
        &**self == other
    }
}

impl PartialEq<&str> for FileName {
    fn eq(&self, other: &&str) -> bool {
        &**self == *other
    }
}

/// Optional per-record path in one pointer: empty paths (the norm, since
/// scanners derive file paths from the parent directory) cost no allocation,
/// and present ones pay a second indirection only when read.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RecordPath(Option<Box<Box<str>>>);

impl From<Box<str>> for RecordPath {
    fn from(path: Box<str>) -> Self {
        Self((!path.is_empty()).then(|| Box::new(path)))
    }
}

impl From<String> for RecordPath {
    fn from(path: String) -> Self {
        Self::from(path.into_boxed_str())
    }
}

impl From<&str> for RecordPath {
    fn from(path: &str) -> Self {
        Self::from(Box::<str>::from(path))
    }
}

impl Deref for RecordPath {
    type Target = str;
    fn deref(&self) -> &str {
        self.0.as_deref().map_or("", |path| path)
    }
}

impl AsRef<str> for RecordPath {
    fn as_ref(&self) -> &str {
        self
    }
}

impl std::hash::Hash for RecordPath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&**self, state);
    }
}

impl fmt::Display for RecordPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

impl fmt::Debug for RecordPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl PartialEq<str> for RecordPath {
    fn eq(&self, other: &str) -> bool {
        &**self == other
    }
}

impl PartialEq<&str> for RecordPath {
    fn eq(&self, other: &&str) -> bool {
        &**self == *other
    }
}

#[cfg(test)]
mod tests {
    use super::{FileName, RecordPath};
    #[test]
    fn inline_and_long_unicode_names_keep_owned_bytes_and_clone() {
        for text in [
            "",
            "file-000-000-0000.bin",
            "é日😀",
            "a filename longer than the inline capacity 😀",
        ] {
            let name = FileName::from(text);
            assert_eq!(name.as_ref(), text);
            assert_eq!(name.clone(), name);
            assert_eq!(name.to_string(), text);
            assert_eq!(FileName::from(text.to_owned()), name);
            assert_eq!(FileName::from(Box::<str>::from(text)), name);
        }
        assert_eq!(std::mem::size_of::<FileName>(), 24);
    }

    #[test]
    fn names_support_borrowed_hash_lookup_and_standard_string_views() {
        let mut names = std::collections::HashMap::new();
        for text in ["short.txt", "a filename exceeding inline capacity.txt"] {
            let name = FileName::from(text);
            assert!(name == text);
            assert!(name == *text);
            assert_eq!(format!("{name:?}"), format!("{text:?}"));
            names.insert(name, text.len());
            assert_eq!(names.get(text), Some(&text.len()));
        }
        assert!(FileName::default().is_empty());
    }

    #[test]
    fn record_paths_are_one_pointer_and_empty_paths_do_not_allocate() {
        assert_eq!(std::mem::size_of::<RecordPath>(), 8);
        assert_eq!(RecordPath::from(""), RecordPath::default());
        assert!(RecordPath::from(String::new()).0.is_none());
        let path = RecordPath::from(r"C:\dir\é😀.txt");
        assert_eq!(&*path, r"C:\dir\é😀.txt");
        assert_eq!(path.to_string(), r"C:\dir\é😀.txt");
        assert_eq!(format!("{path:?}"), format!("{:?}", r"C:\dir\é😀.txt"));
        assert!(path == r"C:\dir\é😀.txt");
        assert!(RecordPath::default().is_empty());
    }
}
