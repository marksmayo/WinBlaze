# Data Model

## Core Entities

- `FileRecord`: file identity, parent directory, path, sizes, attributes, and times.
- `DirectoryRecord`: directory identity, parent, path, and aggregate totals.
- `VolumeRecord`: mount point, filesystem type, capacity, and root directory.
- `ScanSession`: current scan state and progress for a volume/root.

## Identity Rules

- Use stable numeric identifiers inside the process and index.
- Keep parent relationships explicit.
- Track renames and moves through lineage once incremental rescans are available.

## Aggregation Rules

- Direct bytes belong to the current directory only.
- Total bytes include descendants.
- Entry totals should distinguish direct entries from recursive totals.
