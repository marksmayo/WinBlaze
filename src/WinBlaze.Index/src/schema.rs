pub const SCHEMA_VERSION: i64 = 1;

pub const CREATE_META_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
"#;

pub const CREATE_VOLUME_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS volumes (
    id INTEGER PRIMARY KEY NOT NULL,
    mount_point TEXT NOT NULL,
    label TEXT NULL,
    file_system INTEGER NOT NULL,
    total_bytes INTEGER NOT NULL,
    free_bytes INTEGER NOT NULL,
    root_directory_id INTEGER NOT NULL
);
"#;

pub const CREATE_SESSION_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    session_id INTEGER PRIMARY KEY NOT NULL,
    volume_id INTEGER NOT NULL,
    root_path TEXT NOT NULL,
    state INTEGER NOT NULL,
    completed_items INTEGER NOT NULL,
    total_items INTEGER NOT NULL,
    completed_bytes INTEGER NOT NULL,
    total_bytes INTEGER NOT NULL,
    FOREIGN KEY(volume_id) REFERENCES volumes(id)
);
"#;

pub const CREATE_DIRECTORY_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS directories (
    id INTEGER PRIMARY KEY NOT NULL,
    parent_directory_id INTEGER NULL,
    name TEXT NOT NULL,
    full_path TEXT NOT NULL,
    direct_bytes INTEGER NOT NULL,
    total_bytes INTEGER NOT NULL,
    direct_entries INTEGER NOT NULL,
    total_entries INTEGER NOT NULL,
    FOREIGN KEY(parent_directory_id) REFERENCES directories(id)
);
"#;

pub const CREATE_FILE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY NOT NULL,
    parent_directory_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    full_path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    allocation_bytes INTEGER NOT NULL,
    attributes INTEGER NOT NULL,
    created_utc INTEGER NULL,
    modified_utc INTEGER NULL,
    accessed_utc INTEGER NULL,
    FOREIGN KEY(parent_directory_id) REFERENCES directories(id)
);
"#;

pub const CREATE_PATH_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_files_parent_directory_id ON files(parent_directory_id);",
    "CREATE INDEX IF NOT EXISTS idx_directories_parent_directory_id ON directories(parent_directory_id);",
    "CREATE INDEX IF NOT EXISTS idx_sessions_volume_id ON sessions(volume_id);",
];

pub const MIGRATIONS: &[&str] = &[
    CREATE_META_TABLE,
    CREATE_VOLUME_TABLE,
    CREATE_SESSION_TABLE,
    CREATE_DIRECTORY_TABLE,
    CREATE_FILE_TABLE,
];
