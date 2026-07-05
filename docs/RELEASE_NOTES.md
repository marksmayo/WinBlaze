# Release Notes

## WinBlaze 0.1.0

Build identity: `0.1.0` from `src\WinBlaze.UI\Package.appxmanifest`
package version `0.1.0.0`.

### User-Visible Scope

- Stable WinUI 3 MVP shell with root selection, scan, cancel, breadcrumbs,
  indexed search and filters, virtualized catalog rows, detail cards,
  diagnostics, and a GPU-backed treemap.
- Binary snapshot persistence under `%LOCALAPPDATA%\WinBlaze\index` with cached
  catalog loading on startup.
- Incremental rescan action from the UI for refreshing an existing indexed root.
- JSONL scanner/index events and failure reports under
  `%LOCALAPPDATA%\WinBlaze\logs`.
- Portable packaging, installer staging validation, update-manifest generation,
  and conditional signing workflow are scripted.

### Scanner And Indexing

- NTFS volume-root scans are first-class; subdirectory, non-NTFS, and fallback
  scans use directory walking.
- Search supports substring, prefix, exact matching, extension filters, date
  filters, path matching, and minimum-size filters with B/KB/MB/GB/TB suffixes.
- The active runtime index is the compact binary snapshot format. SQLite remains
  a documented future option, not the v1 runtime backend.

### Known Validation Items

- Physical removable-drive hot-unplug behavior is a release checklist item
  because it requires hardware and timing outside deterministic repo tests.
- Larger machine-calibrated performance comparisons beyond generated benchmark
  profiles should be recorded before claiming release-scale parity.
- Release signing requires external certificate and secret provisioning.

## Dependency Security Audit (2026-07-05)

`cargo audit` 0.22.2 against RustSec advisory-db (1,156 advisories loaded):
5 crate dependencies scanned, **no vulnerabilities or warnings reported**.
The Rust workspace is intentionally near-dependency-free; re-run
`cargo audit` before each release.
