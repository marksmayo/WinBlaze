#![forbid(unsafe_code)]

pub mod query;
pub mod schema;
pub mod store;
pub mod tree;
#[cfg(test)]
mod tests;

pub use query::{IndexCatalog, IndexRecordKind, IndexSearchHit};
pub use tree::{DirRollup, TreeEntry, TreeIndex};
pub use schema::{
    CREATE_DIRECTORY_TABLE, CREATE_FILE_TABLE, CREATE_META_TABLE, CREATE_PATH_INDEXES,
    CREATE_SESSION_TABLE, CREATE_VOLUME_TABLE, MIGRATIONS, SCHEMA_VERSION,
};
pub use store::{
    BufferedIndexTransaction, IndexBackend, IndexRepository, IndexSnapshot, IndexStorageError,
    IndexTransaction, SqliteIndexRepository,
};
