# Search and Filtering

## Search Model

- Search should operate on the persistent index.
- Search should not trigger a filesystem rescan.
- Search should accept a free-text pattern plus structured filters.

## Supported Filters

- Size range, including B/KB/MB/GB/TB suffixes in the shell minimum-size field
- Date range, using UTC day boundaries for modified-after and modified-before filters
- Extension, with semicolon-separated extension tokens
- Path scope through indexed path matching in the main search field
- File versus directory inclusion

## Matching Rules

- Prefer substring matching by default.
- Support prefix matching for faster narrowing.
- Allow exact and contains modes where the UI needs stricter control.

## UI Behavior

- Search and filter controls should update the current query state immediately.
- Queries should be explicit and visible to the user.
- The shell should show when search is scoped, filtered, or empty-state limited.
