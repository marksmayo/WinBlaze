# P2 Completion Evidence

Priority 2 is complete for the MVP correctness contract. Remaining physical
media and extreme-scale work is release calibration, not missing scanner/index
behavior.

## Large Catalogs

The app uses a streaming scanner/index path and bounded UI materialization:

- scanner events are batched through `ScanEventPipeline`
- persisted catalog loading is capped at 8,192 entries
- visible tree/list rows are paged to 256 rows
- treemap rendering is limited to the top 10 catalog tiles
- generated `scale` UI benchmarks cover 16,384 files and pass Release budgets
- synthetic index projection covers 1M, 10M, and 50M planning targets
- `LargeUiScalePlan` regression coverage verifies the 50M-file UI materialization
  contract without requiring a 50M-file local filesystem fixture

## Disconnect And Transient I/O

The scanner classifies Windows removable-media and interrupted-operation errors
as transient scan issues instead of fatal crashes. Regression coverage includes:

- device not ready
- device missing
- open failed
- semaphore timeout
- no media in drive
- I/O device error
- device not connected
- operation aborted
- request aborted

Physical hot-unplug testing remains a release checklist item because it requires
hardware and timing outside the deterministic repo test suite.

## Filesystem Metadata Inconsistency

Directory-walk fallback now validates the scan root before creating catalog
records. Missing roots and file-as-root inputs emit diagnostics plus an empty
summary and do not create fake root directory rows.

Regression coverage includes:

- missing-root backend scanner test
- file-root backend scanner test
- Windows `ERROR_DIRECTORY` classified as not found
- UI negative smoke for missing root and file-as-root diagnostics
- live incremental add, modify, and remove smoke coverage

## Verification Commands

```powershell
cargo test -p winblaze-scanner
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-release-baseline-set.ps1 -Profiles tiny,fanout,fanout-large,scale -Runs 3 -GenerateDatasets
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-incremental-benchmark-suite.ps1 -AppPath src\WinBlaze.UI\bin\x64\Release\WinBlaze.UI.exe -Size tiny -Mutations add,remove,modify -GenerateDataset -OutputPath benchmarks\winblaze-incremental-baseline.json
```
