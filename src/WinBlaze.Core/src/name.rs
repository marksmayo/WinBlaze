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

impl AsRef<str> for FileName {
    fn as_ref(&self) -> &str {
        self
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

#[cfg(test)]
mod tests {
    use super::FileName;
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
}
