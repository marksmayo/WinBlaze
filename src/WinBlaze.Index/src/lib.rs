#![forbid(unsafe_code)]

mod directory_records;
pub mod export;
mod file_records;
pub mod import;
pub mod query;
pub mod schema;
pub mod store;
#[cfg(test)]
mod tests;
pub mod tree;
pub mod tree_search;

pub use query::{IndexCatalog, IndexRecordKind, IndexSearchHit};
pub use schema::{
    CREATE_DIRECTORY_TABLE, CREATE_FILE_TABLE, CREATE_META_TABLE, CREATE_PATH_INDEXES,
    CREATE_SESSION_TABLE, CREATE_VOLUME_TABLE, MIGRATIONS, SCHEMA_VERSION,
};
pub use store::{
    BufferedIndexTransaction, IndexBackend, IndexRepository, IndexSnapshot, IndexStorageError,
    IndexTransaction, SqliteIndexRepository,
};
pub use tree::{DirRollup, TreeEntry, TreeIndex};

pub mod storage_cost;

pub mod multiple_roots;
