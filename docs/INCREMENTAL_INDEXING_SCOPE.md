# Incremental Indexing Scope

## V1 Decision

V1 incremental indexing should support explicit user-triggered rescans of a
previously scanned root. It should not try to maintain a background live index.

## In Scope

- User starts an incremental rescan from the UI for the current root.
- Scanner walks or enumerates the selected root again.
- Index layer compares previous and current file records.
- File additions, removals, size changes, renames, and moves are captured through
  existing change-set and lineage structures.
- UI reloads the refreshed catalog snapshot after the rescan.
- Diagnostics show elapsed time, changed file count, removed file count, and
  issue count.

## Out Of Scope For V1

- Background filesystem watchers.
- Always-on indexing.
- Cross-boot journal replay.
- Network path incremental guarantees.
- Perfect rename/move detection when filesystem identity metadata is missing.
- Conflict resolution for concurrently changing files beyond existing transient
  I/O handling.

## Current Implementation State

- Lower-level index change detection exists through `apply_incremental_files`.
- Lineage and file change-set records are serialized in the binary snapshot.
- The native bridge exposes `wb_scan_session_start_incremental`, which preserves
  the existing snapshot and applies incremental file changes when the rescan
  emits summary/completed events.
- The C++ `NativeBridge` loads that symbol and exposes
  `StartIncrementalScan`.
- The active UI exposes an `Incremental rescan` command and reloads the
  refreshed snapshot after completion.
- Checked-in UI smoke verifies full scan followed by incremental rescan after
  adding a file to the fixture.
- The incremental benchmark runner verifies generated add, remove, and modify
  mutations.
- Correctness diagnostics show added/removed/modified/renamed/moved counts for
  incremental rescans.
- Directory-walk rescans use path-matched file IDs before diffing so ephemeral
  per-run IDs do not produce false move/rename counts.

## Required Next Steps

1. Add negative-case UI smoke coverage for removals and changed files.
2. Add larger calibrated incremental benchmark baselines.
