# Index Strategy

## Storage Model

- The active runtime backend is a compact binary cache snapshot, not SQLite.
  SQLite remains a documented option through the `IndexBackend` abstraction, but
  `WinBlaze.Native` opens `SqliteIndexRepository` with `IndexBackend::BinaryCache`.
- V1 decision: binary cache remains the primary runtime index. SQLite should not
  return as the primary backend unless benchmark or maintenance evidence shows a
  clear advantage for large query/update workloads.
- The default runtime path is `%LOCALAPPDATA%\WinBlaze\index\winblaze.index.bin`.
- Snapshot writes use a temp file and atomic rename:
  `winblaze.index.tmp` -> `winblaze.index.bin`, with `winblaze.index.bak` used
  as a short-lived recovery copy.
- Startup snapshot loading first tries the primary snapshot, then falls back to
  the backup snapshot if the primary is corrupt.
- The binary file starts with magic `WBIX` and format version `1`.
- Records are serialized in this order:
  1. volume records
  2. scan-session records
  3. directory records
  4. file records
  5. file lineage records
  6. file change sets
- Strings are length-prefixed UTF-8. Optional strings and timestamps are encoded
  with a one-byte presence flag followed by the value when present.
- Integer IDs and byte counts are little-endian numeric fields.
- The schema version and migration list still exist for the repository
  abstraction, but the binary snapshot format is versioned separately by
  `INDEX_FORMAT_VERSION`.

## Update Strategy

- The native bridge creates a fresh buffered transaction for each full scan.
- Scanner events are persisted into that transaction as they arrive:
  - session/volume events upsert the volume and scan session
  - directory events upsert directories
  - file events upsert files
  - progress, issue, and summary events are not stored as catalog records
  - completed/cancelled/failed events update the scan session state
- The repository flushes the buffered transaction on lifecycle boundaries:
  session start, volume discovery, summary, completed, cancelled, and failed.
- Incremental file change detection exists in the index layer through
  `apply_incremental_files`, lineage records, and file change sets. The native
  bridge exposes `wb_scan_session_start_incremental`, which starts from the
  persisted snapshot and applies file changes from the new scan transaction.
  The C++ UI bridge exposes `StartIncrementalScan`, the visible UI has an
  `Incremental rescan` command, and smoke coverage verifies an added file is
  reflected after rescan. Directory-walk rescans path-match current files to the
  previous snapshot before diffing so ephemeral scan IDs do not create false
  moved/renamed counts. UI diagnostics report added, removed, modified, renamed,
  and moved counts for the latest incremental rescan.
- Search reads the latest loaded snapshot and does not trigger filesystem scans.

## Maintenance

- `invalidate_cache` clears the in-memory state and removes primary, backup, and
  temp snapshot files.
- `compact_cache` removes auxiliary files and rewrites the current snapshot.
- Corrupt-primary recovery and cache invalidation are covered by index tests.
- Cache migration documentation is still needed before changing
  `INDEX_FORMAT_VERSION`.

## Search

- Search should query the index directly.
- Search should not rescan the filesystem just to answer a query.
- Query APIs should support scope, size, date, and text matching.
