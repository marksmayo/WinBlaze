# Benchmark Methodology

## Goals

- Measure scan elapsed time on repeatable datasets.
- Track UI working set during scans.
- Track UI responsiveness through frame and flush diagnostics.
- Keep generated datasets outside the repository while preserving expected
  counts through manifests.

## Dataset Generation

Generate datasets with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size tiny -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size small -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size medium -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size fanout -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size fanout-large -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size scale -Clean
```

Datasets are written under `C:\tmp\WinBlazeBench\<size>` by default.
Manifests are written beside the dataset directory as
`C:\tmp\WinBlazeBench\<size>.manifest.json` so manifest files do not affect scan
totals.

Available profiles:

- `tiny`: balanced smoke-sized tree, 72 files, 72 KB.
- `small`: balanced development baseline, 1,536 files, 6 MB.
- `medium`: balanced larger local baseline, 9,216 files, 72 MB.
- `fanout`: one dense leaf directory, 2,048 files, 1 MB, for exercising large
  sibling-list behavior.
- `fanout-large`: one dense leaf directory, 8,192 files, 2 MB, for larger
  sibling-list stress without the broader tree shape of `scale`.
- `scale`: balanced larger-count tree, 16,384 zero-byte files and 545
  directories, for exercising UI event volume and memory-per-file behavior
  without consuming meaningful disk space.

## UI Benchmark Runner

Run:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size tiny -GenerateDataset
```

The runner:

1. optionally regenerates the selected dataset
2. launches the current Debug UI executable
3. sets the scan root to the generated dataset
4. starts a scan
5. waits until UI correctness diagnostics report the manifest file and directory
   totals
6. captures working set, frame diagnostics, correctness text, and treemap status
7. checks for recent WinBlaze `Application Error` entries
8. closes the app

The runner emits JSON. The key fields are:

- `elapsed_ms`: wall-clock time from invoking `Start scan` until expected
  correctness totals appear in the UI
- `working_set_mb`: UI process working set after the scan
- `working_set_bytes_per_file`: working set divided by manifest file count
- `frame_status`: UI batching, frame interval, and flush metrics shown by the app
- `flush_count`, `queued_events`, `last_latency_ms`, `last_input_ms`,
  `flush_cost_ms`, `frame_count`, and `last_frame_ms`: parsed structured UI
  responsiveness counters
- `peak_frame_ms`: parsed peak composition frame interval when available
- `peak_flush_ms`: parsed peak UI flush cost when available
- `treemap_render_flushes`, `treemap_render_requests`, and
  `treemap_render_coalesced`: parsed GPU treemap redraw/coalescing counters
- `scan_duration_ms`: parsed app-reported scan duration from diagnostics
- `correctness`: issue count and summary/catalog reconciliation shown by the app
- `treemap_status`: GPU treemap render status

Optional thresholds can turn the benchmark into a gate:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size tiny -GenerateDataset -MaxElapsedMs 3000 -MaxWorkingSetMb 300 -MaxPeakFrameMs 250 -MaxPeakFlushMs 100
```

Threshold values should be calibrated per machine/profile before they are used
as release gates.

Checked-in local budget files:

- `benchmarks\performance-budgets.json`: conservative Debug thresholds.
- `benchmarks\performance-budgets.release.json`: conservative Release
  thresholds calibrated from the first local Release median capture.

To capture a local baseline set across multiple profiles:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-baseline-set.ps1 -Profiles tiny,fanout,scale -GenerateDatasets
```

The baseline-set runner writes `benchmarks\winblaze-baselines.json` with the
selected profile results.

For repeated first/warmed runs, use:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark-suite.ps1 -Size tiny -Runs 3 -GenerateDataset
```

The suite regenerates the dataset once when requested, then launches the UI
benchmark repeatedly and emits per-run JSON plus first-run, warmed median, and
overall median elapsed timings. The first run is treated as the cold-style run;
later runs are treated as warmed OS/cache runs.

To capture Release median baselines across multiple profiles, build the Release
app first, then run:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-release-baseline-set.ps1 -Profiles tiny,fanout,scale -Runs 3 -GenerateDatasets
```

The Release baseline runner writes `benchmarks\winblaze-release-medians.json`
with first-run elapsed time, warmed median elapsed time, overall median elapsed
time, median working set, median frame/flush/input latency counters, median
treemap render counters, and the underlying per-run records for each profile.

To generate a Markdown calibration summary from the recorded Release medians,
environment capture, and Release budgets:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\write-performance-calibration-report.ps1
```

The report is written to `benchmarks\performance-calibration-report.md`.

To time persisted snapshot loading without starting a scan, first run a scan
benchmark so a catalog snapshot exists, then run:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-cache-load.ps1 -OutputPath benchmarks\winblaze-cache-load-baseline.json
```

The cache-load runner launches the UI, waits until runtime diagnostics show
`results=loaded`, captures native cache read/decode timing from the runtime
diagnostics, captures catalog and treemap status, checks for recent WinBlaze
`Application Error` entries, and emits JSON. With `-OutputPath`, it writes the
same data to a baseline file, including structured cache read/decode timings,
entry count, and load cap.

To measure backend-only index memory/storage overhead without WinUI:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-index-memory-benchmark.ps1 -Files 100000 -OutputPath benchmarks\winblaze-index-memory-baseline.json
```

The runner builds synthetic `FileRecord` entries in the binary-cache repository
and emits JSON with elapsed time, current working set, working-set bytes per
file, snapshot bytes, and snapshot bytes per file. With `-OutputPath`, it writes
a checked-in local baseline to `benchmarks\winblaze-index-memory-baseline.json`.

To create a planning estimate for larger synthetic index counts:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\estimate-index-scale.ps1 -SampleFiles 100000
```

The estimator runs the backend-only index memory example, records the sample,
and writes linear projections for 1 million, 10 million, and 50 million indexed
files. This is budget-planning evidence only; it does not replace real
filesystem traversal or UI virtualization tests.

To measure backend-only directory-walk fallback correctness and throughput
without WinUI:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-directory-walk-benchmark.ps1 -DatasetRoot C:\tmp\WinBlazeBenchDirWalk -Size tiny -GenerateDataset -OutputPath benchmarks\winblaze-directory-walk-baseline.json
```

The runner scans a generated dataset through the scanner's directory-walk path,
checks file/directory/byte totals against the dataset manifest, and emits JSON
with elapsed time, issue count, issue counts by kind, bounded recent issue
details, and files per second. This is the repeatable fallback-path benchmark
for non-NTFS and subdirectory scans. The checked-in local tiny baseline records
72 files, 13 directories, 73,728 bytes, zero issues, and 2 ms elapsed time on
the local development machine.

To measure a UI-driven incremental rescan, run:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-incremental-benchmark.ps1 -Size tiny -GenerateDataset
```

The incremental runner performs a full scan of the generated dataset, applies a
mutation, triggers `Incremental rescan`, waits for the expected file count and
change counts in correctness diagnostics, then emits elapsed time, working set,
correctness, frame diagnostics, parsed input/flush/frame latency counters, scan
duration, and treemap render counters. Use `-Mutation add`, `-Mutation remove`,
or `-Mutation modify`; the default is `add`.

To record all standard incremental mutation cases:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-incremental-benchmark-suite.ps1 -Size tiny -Mutations add,remove,modify -GenerateDataset -OutputPath benchmarks\winblaze-incremental-baseline.json
```

The checked-in local Release incremental baseline lives at
`benchmarks\winblaze-incremental-baseline.json`.

## Baseline Rules

- Run benchmarks from a clean working tree when collecting release baselines.
- Regenerate datasets with `-Clean` before cold-scan timing.
- Run each profile at least three times and record median elapsed time.
- Record hardware, Windows version, build configuration, and commit/build ID.
- Keep competitor baselines separate from WinBlaze runs and record the exact
  competitor version.

Record local competitor tool inventory and optional manual timings with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\record-competitor-baselines.ps1 -Size tiny
powershell.exe -ExecutionPolicy Bypass -File benchmarks\write-competitor-report.ps1
```

The recorder writes `benchmarks\competitor-baselines.json`, including dataset
manifest totals, installed competitor paths/versions when found, and optional
manual elapsed timings passed as `-WizTreeElapsedMs`, `-WinDirStatElapsedMs`, or
`-EverythingElapsedMs`.

## Required Environment Record

Every baseline report should include:

- CPU model and core/thread count
- installed RAM
- storage device type and model when known
- filesystem type
- Windows version/build
- power mode
- whether antivirus/realtime indexing was enabled
- WinBlaze build configuration and commit/build ID
- dataset profile and manifest totals

Capture the repeatable machine/storage portion with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\record-environment.ps1
```

The script writes `benchmarks\environment.json` with OS version, CPU, RAM,
dataset filesystem, dataset drive capacity/free space, and active power scheme
when Windows exposes it. In restricted shells, protected CIM fields may be
`null`; keep the captured file anyway and supplement missing fields manually for
release baselines.

The default benchmark hardware target for early development is the primary
developer workstation. Release-candidate baselines should add at least one
lower-spec Windows machine before comparing against competitor tools.

## Current Gaps

- Release repeated-run medians are captured locally for tiny, fanout,
  fanout-large, and scale profiles in
  `benchmarks\winblaze-release-medians.json`, checked-in local Release
  threshold gates pass for those profiles, and
  `benchmarks\performance-calibration-report.md` summarizes the current
  machine-specific calibration. Broader release-machine coverage remains a
  release-readiness activity, not a blocker for the P1 rendering stack.
- Incremental-rescan timing has a dedicated UI runner and a suite runner for
  generated add, remove, and modify cases. A local Release tiny-profile
  add/remove/modify baseline is recorded in
  `benchmarks\winblaze-incremental-baseline.json`; larger calibrated baselines
  are still pending.
- Cache-load timing measures startup-to-`results=loaded` and now also surfaces
  native binary-cache read/decode timing in UI diagnostics. A local Release
  baseline is recorded in `benchmarks\winblaze-cache-load-baseline.json`; larger
  calibrated baselines are still pending.
- Backend index memory/storage overhead has a checked-in local 100,000-file
  baseline in `benchmarks\winblaze-index-memory-baseline.json`, and
  `benchmarks\index-scale-estimate.json` projects 1M/10M/50M planning targets
  from a 100,000-file synthetic sample.
- Directory-walk fallback timing has a backend-only runner with manifest
  correctness checks; non-NTFS physical-media calibration is still pending.
- Debug and Release local-gate thresholds exist for tiny, fanout, fanout-large, and scale
  elapsed time, working set, peak frame interval, and peak flush cost. These are
  workstation stability gates, not final product performance targets.
- The `fanout`, `fanout-large`, and `scale` profiles cover repeatable local
  large-UI behavior. The MVP tens-of-millions contract is bounded UI
  materialization plus streaming/batched scanner events, recorded in
  `docs\P2_COMPLETION_EVIDENCE.md`; larger hardware-calibrated runs remain
  release validation.
- Competitor inventory is recorded through
  `benchmarks\record-competitor-baselines.ps1`; manual elapsed timings against
  WizTree, WinDirStat, and Everything are still pending.
