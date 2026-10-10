# Export feature TDD evidence

Implemented 2026-10-03 after reviewing the feature parity table.

| Public behavior | RED | GREEN |
| --- | --- | --- |
| Versioned complete JSON catalog | Missing `write_json` compiler error | Schema version 1, root totals and numeric fields test passes |
| JSON Unicode and control characters | Escaping assertion failed | Quotes, backslashes, newline/tab and all control bytes are escaped; Unicode retained |
| Files-only extension CSV | Missing `write_csv_extension` compiler error | Mixed-case `*.bIn` selects matching files and excludes directories |
| Invalid extension input | `unwrap_err` failed because invalid filter was accepted | Empty, wildcard/multi-pattern/path/compound extension inputs rejected before writing |
| User-facing export commands | UI regression could not find Export JSON | Rebuilt File menu exposes both commands; actual JSON/CSV saved and parsed; invalid filter feedback verified |

Additional public-interface tests cover 8,200 JSON files, derived paths, missing/negative timestamps, integer sizes above JavaScript's exact-number range and destination errors. Full workspace tests, strict clippy and formatting pass. Rust line coverage: **93.32%**, exceeding the **92%** repository gate.

The native extension interface validates filters before opening the destination. Export work runs off the UI thread and scan controls remain blocked during export. JSON and extension CSV benchmarks each run n=10 and validate actual output. The UI save-dialog test needs desktop access outside the filesystem sandbox; the successful run saved two catalog files to JSON, one matching file to CSV, and rejected `*.*`.

JSON earns full feature support. Filtered export earns partial support because only one extension is supported. CSV import, content duplicate detection, ACL export, monitoring and PNG export remain gaps.
