# Scanner Strategy

## Filesystem Access

- Treat NTFS as the first-class path for metadata-rich scanning.
- Use directory traversal as the fallback path for non-NTFS volumes.
- Handle long paths and reparse-point policy explicitly.
- Discover volumes and roots before scan execution starts.
- Subdirectory scans are intentionally kept on the directory-walk backend so a
  request like `C:\tmp\WinBlazeSmoke` does not widen to a full-drive MFT scan.
- Full volume-root scans may use the NTFS MFT backend when selected by the access
  plan.

## Active Pipeline

1. UI calls `NativeBridge::StartScan(rootPath, handler)`.
2. `WinBlaze.Native` creates a `ScanController` channel and starts a scanner
   worker with a `ScanRequest`.
3. The scanner builds a `ScanAccessPlan` from the requested root and backend
   hint.
4. NTFS volume-root scans stream MFT events. If NTFS enumeration fails, the
   controller emits a deduplicated issue and falls back to directory walking.
5. Directory-walk scans emit:
   - `SessionStarted`
   - root and child `DirectoryFound`
   - `FileFound`
   - periodic `Progress`
   - final `Summary`
   - `Completed` or `Cancelled`
6. Native bridge forwards each event to the UI callback and persists catalog
   events into a buffered binary-cache index transaction.
7. The UI batches incoming events through a dispatcher timer before updating
   visible controls, diagnostics, and treemap redraw state.

## Performance Strategy

- Stream events instead of buffering whole-volume results.
- Parallelize only where volume or directory boundaries make it safe.
- Keep metadata extraction narrow and avoid redundant syscalls.
- Measure the fallback path with `benchmarks\run-directory-walk-benchmark.ps1`;
  it runs the scanner's directory-walk backend against generated datasets and
  validates file, directory, and byte totals against the manifest. The checked-in
  tiny baseline records a clean fallback scan with zero issues.
- Apply backpressure so the UI remains responsive.
- Write scanner lifecycle, progress, summary, issue, and index flush events as
  JSONL to `%LOCALAPPDATA%\WinBlaze\logs\events.jsonl` for shell diagnostics
  without logging every file discovery.

## Correctness Strategy

- Continue past permission failures.
- Tolerate files being changed or removed during scan.
- Preserve results for locked or transiently unavailable files when possible.
- Deduplicate issue events by kind, path, and message within a scan.
- Keep root-level directory-walk fixtures deterministic for integration tests.
- Sparse files, hardlinks, permission failures, transient I/O, deleted/changed
  files, and reparse policy have regression coverage in lower-level scanner
  tests.
- UI correctness diagnostics currently show issue count, last issue, summary
  totals, and catalog-sample byte reconciliation.
