# Benchmarks

Performance harness for scan speed, memory usage, and responsiveness.

## Repeatable Datasets

Use `make-datasets.ps1` to create deterministic benchmark trees under `C:\tmp`
without checking generated data into the repository:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size tiny -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size small -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size medium -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size fanout -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size fanout-large -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size scale -Clean
```

Profiles:

- `tiny`: 3 top-level dirs, 9 leaf dirs, 72 files, 72 KB.
- `small`: 8 top-level dirs, 64 leaf dirs, 1,536 files, 6 MB.
- `medium`: 16 top-level dirs, 192 leaf dirs, 9,216 files, 72 MB.
- `fanout`: 1 top-level dir, 1 dense leaf dir, 2,048 files, 1 MB.
- `fanout-large`: 1 top-level dir, 1 dense leaf dir, 8,192 files, 2 MB.
- `scale`: 32 top-level dirs, 512 leaf dirs, 16,384 zero-byte files.

Each generated dataset writes a sibling `<size>.manifest.json` under the dataset
root with expected file, directory, and byte totals. The manifest is intentionally
outside the scanned dataset directory so it does not affect totals.

Record the machine/storage context for local baselines:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\record-environment.ps1
```

## Planned Harness Work

- UI-driven scan timing and diagnostics:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size tiny -GenerateDataset
```

The runner launches the current Debug UI, scans the requested dataset, waits for
the expected summary totals from `<size>.manifest.json`, captures working set, frame
diagnostics, correctness diagnostics, and treemap status, then emits JSON.
Use `-MaxElapsedMs`, `-MaxWorkingSetMb`, `-MaxPeakFrameMs`, and
`-MaxPeakFlushMs` to turn a run into a threshold gate.

For repeated first/warmed measurements:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark-suite.ps1 -Size tiny -Runs 3 -GenerateDataset
```

The suite launches a fresh UI process for each run and labels run 1 as `first`
and later runs as `warmed`.

For Release median baselines across the standard profiles:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-release-baseline-set.ps1 -Profiles tiny,fanout,fanout-large,scale -Runs 3 -GenerateDatasets
```

The runner writes `benchmarks\winblaze-release-medians.json` with first-run,
warmed median, overall median, structured latency/render medians, and per-run
results for each profile.

Generate the Markdown calibration summary with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\write-performance-calibration-report.ps1
```

For a local multi-profile baseline set:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-baseline-set.ps1 -Profiles tiny,fanout,scale -GenerateDatasets
```

The runner writes `benchmarks\winblaze-baselines.json`.
Add `-EnforceBudgets` to apply the conservative Debug thresholds in
`benchmarks\performance-budgets.json` across the selected profiles. For Release
builds, pass `-BudgetPath benchmarks\performance-budgets.release.json`.

For persisted snapshot load timing after a scan has created a cache:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-cache-load.ps1 -OutputPath benchmarks\winblaze-cache-load-baseline.json
```

This runner waits for `results=loaded` without starting a new scan and emits the
native binary-cache read/decode diagnostic line when a persisted snapshot exists.
With `-OutputPath`, it records elapsed time, working set, cache read/decode
timings, entry count, and load cap.

For backend-only index memory/storage overhead:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-index-memory-benchmark.ps1 -Files 100000 -OutputPath benchmarks\winblaze-index-memory-baseline.json
```

For synthetic large-count index planning:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\estimate-index-scale.ps1 -SampleFiles 100000
```

For backend-only directory-walk fallback timing and correctness without launching
WinUI:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-directory-walk-benchmark.ps1 -DatasetRoot C:\tmp\WinBlazeBenchDirWalk -Size tiny -GenerateDataset -OutputPath benchmarks\winblaze-directory-walk-baseline.json
```

The checked-in local baseline at `benchmarks\winblaze-directory-walk-baseline.json`
records the generated tiny profile with manifest-matched file, directory, and
byte totals and zero scan issues.

For incremental rescan timing:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-incremental-benchmark.ps1 -Size tiny -GenerateDataset
```

The runner performs an initial full scan, writes one added file, triggers
`Incremental rescan`, and emits JSON once correctness diagnostics show the
updated file count and `incremental added=1`.
Use `-Mutation add`, `-Mutation remove`, or `-Mutation modify` to cover the
supported generated-dataset mutation cases.

For a combined add/remove/modify incremental baseline:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-incremental-benchmark-suite.ps1 -Size tiny -Mutations add,remove,modify -GenerateDataset -OutputPath benchmarks\winblaze-incremental-baseline.json
```

- cold scan benchmarks for balanced and dense fan-out profiles
- warmed repeated-run benchmarks
- persisted cache-load benchmarks
- backend directory-walk fallback benchmarks
- incremental rescan benchmarks for add/remove/modify generated dataset cases
- machine/storage environment capture
- competitor comparison notes

Record competitor tool inventory and optional manual timings:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\record-competitor-baselines.ps1 -Size tiny
powershell.exe -ExecutionPolicy Bypass -File benchmarks\write-competitor-report.ps1
```
