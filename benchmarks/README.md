# Benchmarks

Performance harness for scan speed, memory usage, and responsiveness.

## Current Comparison Dashboard

Open `CURRENT_COMPARISON.html` for the offline responsive report. Results are
embedded at build time and remain visible when a preview disables JavaScript;
scripts enhance the report with interactive filters. The generator source is
`report-template.html.template`; `report-template.html` links to the finished
report so it can no longer be mistaken for a runnable data-filled report. It includes
dataset filters, individual runs, sampled CPU/memory charts, elevated results,
binary version history, correctness observations, and downloadable aggregate data.
Rebuild it with `node benchmarks/render-report.mjs`; verify its scripts and filters
with `node benchmarks/verify-report.mjs`. Each render archives its input evidence
under `benchmarks/runs/`. Browser visual verification is separate from these checks.

The comparison runners capture executable product/file versions and SHA-256 hashes
in each new measurement. `tool-version-history.json` appends binary changes;
different binary identities must remain separate baselines. Historical observations
without embedded identities are explicitly joined to their recorded inventories.
Use `run-elevated-comparison.ps1` and `run-elevated-native-comparison.ps1` from an
administrator terminal for MFT and native pipeline measurements. Measurement
scopes differ: engine, native indexing, GUI duration, and scan/export are not
interchangeable. See `CURRENT_COMPARISON.md` for limitations and extended findings.

## Automated comparison and CSV export

Run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File benchmarks/run-comparison.ps1`
to build release benchmark executables and collect ten observations per product
for tiny, scale, dense-directory, and full C: scans. It also measures startup and
the standalone WinBlaze engine, archives raw JSON and binary identities, and
refreshes `CURRENT_COMPARISON.html` after each stage. Always use `-Runs 10` for reruns; use `-IncludeElevated` from an administrator terminal for the MFT tests.
Historical measurements retain their original sample counts. Cache state is
uncontrolled; timings include process startup and CSV export, excluding rendering.
CPU and memory samples are lower bounds; missing samples are not zero usage.

The report ends with a source-linked feature matrix highlighting WinBlaze gaps.
Its podium includes timing, CPU, memory, and feature events, with scopes shown.
Feature support awards gold for full, silver for partial, and bronze for absent;
Equal support earns no medal and its comparison rows are omitted. Unverified capabilities receive no medal. Audit trails and optimization history stay in standalone files, outside the focused HTML comparison.

In WinBlaze, finish a scan and choose **File → Export CSV…**. Export writes the
complete catalog, including entries beyond the UI display cap, as UTF-8 RFC 4180
CSV with CRLF lines. Columns are `Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds`.
Directories contain aggregate totals; modification timestamps use Unix seconds.
The benchmark CLI is `target/release/examples/scan_export.exe <root> <output.csv>`.
CSV behavior tests: `cargo +1.96.0 test -p winblaze-index --test csv_export --offline`.

## Feature performance

`powershell.exe -NoProfile -ExecutionPolicy Bypass -File benchmarks/run-feature-comparison.ps1`
runs ten observations for each supported automated feature workflow and refreshes
the HTML after every observation. The main `run-comparison.ps1` includes this stage;
use `-SkipFeatures` to omit it. Run `run-feature-comparison.ps1 -OnlyMft` in an
administrator terminal for WizTree raw MFT dumps. `-IncludeElevated` on the main
runner includes the dump workload when already elevated.

The feature fixture has 224 one-MiB files: 32 true duplicate groups with four
copies each, 32 unique files, and 32 equal-name/size/date pairs with different
contents. Validated workloads cover content duplicates, JSON/ACL exports, filtered
CSV, extension summaries, 1920x1080 treemap PNGs, CSV import/re-export, and WinBlaze
catalog loading/cached export. WinBlaze uses an isolated snapshot under artifacts.
Raw MFT and ACL outputs are removed after validation; aggregate evidence remains.

Process time includes startup and the stated operation. CPU, working set and
private memory are sampled every 10 ms; output bytes are recorded separately.
Only validated outputs enter medians. Existing saved competitor settings apply.
Different scopes and unsupported/GUI-only operations stay explicit in the report's
per-feature coverage table. Product-specific timings receive no performance medal;
three-product, matching-fixture comparisons do. Feature support medals are separate.

Check the collected batch and malformed-output regressions with
`powershell.exe -NoProfile -ExecutionPolicy Bypass -File benchmarks/verify-feature-benchmarks.ps1`.
Run `node benchmarks/verify-report.mjs` for every medal measurement and the static report.

## Dataset generation

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

## WinBlaze optimization refresh

`powershell -ExecutionPolicy Bypass -File benchmarks/run-winblaze-refresh.ps1 -Runs 10` rebuilds Release binaries, archives current evidence, refreshes WinBlaze scan/export, startup, engine, feature, GUI, full-cache, incremental, index-memory, and public-operation tests, and regenerates the offline HTML report. Competitor observations retain their original dates and versions; this is not a newly interleaved three-product batch. `-SkipBuild` reuses verified binaries. `-IncludeElevated` requires an administrator terminal and refreshes WinBlaze native/MFT workloads without rerunning competitors.

See `PERFORMANCE_IMPROVEMENTS.md` for the 100-point candidate audit. Nine tested production changes are implemented; other entries explicitly distinguish existing optimizations, measurement improvements, and unqualified ideas. `current-optimization-results.json` records descriptive before/after medians. Live-drive changes and uncontrolled caches prevent attributing every percentage to the code changes.

Standard and administrator refresh entry points share a Windows named mutex, so their measurement workloads run sequentially. A canceled administrator prompt leaves the relevant elevated refresh pending; excluded observations do not enter medians.

### JSON and filtered exports

WinBlaze **File > Export JSON** writes schema version 1 with all catalog entries, integer byte counts and nullable Unix UTC timestamps. **File > Export CSV by extension** accepts one extension (`bin`, `.bin`, `*.bin`), matches case-insensitively and exports every matching file without a display cap. Multiple patterns and logical-size/UTC-date bounds are also supported.

`feature_catalog json-export <cached-root> <output.json>` and `feature_catalog filtered-export <cached-root> <output.csv>` exercise the same native export interfaces (the benchmark filter is `*.bin`). The n=10 runner measures startup, catalog decode and serialization plus sampled CPU/working/private memory. The File menu also supports multiple filename patterns and inclusive logical-size/UTC-date bounds; `advanced-filter` benchmarks this combination. Competitor jobs include scanning; these different scopes receive no performance medals. Validators check JSON schema, complete file/byte totals and exact extension selection, and reject corrupt outputs.

Run `powershell -NoProfile -File tests/ui/export-features.ps1` in a desktop session with permission to operate Windows save dialogs. It launches its own app with an isolated cache and saves only repository fixture exports.


### CSV interoperability and complete feature views

WinBlaze's File menu now reopens WinBlaze, WinDirStat and WizTree UTF-8 CSV scans, exports multi-pattern/size/UTC-date filtered CSV, and saves dedicated 1920x1080 treemap PNGs. Completed extension totals include every extension; Cleanup pages through all largest files. Dense treemap tiles preserve positive area and omitted children retain aggregate weight. See [red/green evidence](TDD_FEATURE_PARITY_EVIDENCE.md) for the supported schemas, timezone and rendering limits.

`feature_catalog csv-import <expected-root> <input.csv> <output.csv>` measures importing and re-exporting without scanning. `advanced-filter` and `largest-files` use the normal three-argument shape. `WinBlaze.UI.exe --treemap-png <output.png>` is the unattended adapter for the same GPU/WIC export path as the File menu, using the persisted scan cache. `run-feature-comparison.ps1 -Tools WinBlaze -Runs 10` creates foreign CSV seeds outside timing and runs 16 validated WinBlaze jobs (160 observations); all products have 25 standard jobs plus two separately gated raw-MFT jobs. Every observation records binary identity and supported resource samples. Refresh entry points share a measurement mutex.

Full GUI refreshes now prune completed generated cache copies between repeats, retaining the final cache for restore measurements. JSON result and version-history updates use atomic replacement so a disk/write failure preserves the last complete file. Older WinBlaze CSV timestamps written as FILETIME ticks are accepted and re-exported as true Unix seconds.


UI startup profiling: `pwsh -NoProfile -File benchmarks/run-ui-ready-comparison.ps1 -Runs 10` measures fresh-empty-catalog visible/maximized scan-control readiness separately from the hidden first-idle proxy. It uses isolated per-run app data and includes UI Automation plus 10 ms polling overhead. See `UI_STARTUP_IMPROVEMENTS.md` for the 50-candidate audit. Routine tracing is opt-in with `WINBLAZE_STARTUP_TRACE=1`; failure logging remains always enabled.


Additional performance medals are derived automatically from accepted `current-workflow-results.json` observations on every render: p90 process completion and timing variability (IQR / median × 100) for quick, scale, dense and full-volume datasets, plus sampled CPU, peak working set and private memory for the synthetic datasets. P90 and variability require at least ten valid observations from every product; resource medals require at least three nonmissing telemetry samples per product. Quantiles use R-7 linear interpolation. Run `node benchmarks/verify-performance-metrics.mjs` and `node benchmarks/verify-report.mjs` to check eligibility, formulas, missing values and the visible measurement table. Related events have equal medal weight; the podium is not a statistical proof of overall superiority.

Feature workload tables show every eligible combination and its medals or an explicit reason for no ranking. The expandable medal-eligibility matrix covers every recorded workload × all seven metric selectors. Scan/CSV process time, CPU and memory already award medals; missing comparator jobs, incomparable native phases and non-equivalent output sizes do not fabricate scores.


### Remaining feature parity (4 October 2026)

The portable ZIP and installer include `winblaze-cli.exe`. Commands:

```powershell
winblaze-cli C:\ reports.csv
winblaze-cli --duplicates C:\Data duplicates.csv
winblaze-cli --permissions C:\Data permissions.csv
# Administrator session required:
winblaze-cli --dump-mft C:\ volume.mft
```

Duplicate exports verify complete bytes, exclude hard-link aliases from reclaimable copies, and report read/change errors. ACL exports include owner, group and DACL as SDDL, with errors for inaccessible entries. Raw MFT export streams initialized MFT data including unused records; live positive validation requires administrator access.

Cleanup rows now offer confirmed file Move and Recycle actions, rejecting existing destinations, directories and reparse points. File > Monitor file-system changes enables recursive notifications and a debounced automatic refresh. Imported catalogs cannot monitor, and internal cache writes are excluded. Allocation queries use actual Windows storage and count hard-linked allocation once.

The new duplicate and permission jobs include startup, scan, indexing/persistence and export. Native move/recycle and change-notification adapters measure real component operations on isolated files; their scopes explicitly exclude GUI confirmation/rescan. Every rerun uses `-Runs 10`. Unknown/missing telemetry and unsupported comparator adapters receive no fabricated performance medals. Feature-support medals follow the implemented behavior and documented limits.


`run-parity-elevated.ps1 -Runs 10` is the combined administrator helper for current WinBlaze native/MFT phases and both WinBlaze/WizTree raw dumps. It checks ten validated outputs per raw job, records completion/failure, and renders the report. The standard refresh also includes current-release visible-ready startup; its historical tracing comparison remains separately labeled.


When using `-SkipBuild`, rebuild **all** benchmark targets first, not only the UI/DLL: `cargo build --release --offline -p winblaze-native -p winblaze-scanner -p winblaze-index --examples`, followed by `cargo build --release --offline -p winblaze-native`. Native DLL/CLI builds do not rebuild standalone Scanner/Index example executables. The default full refresh builds these targets automatically. Engine profiles have a validated allowlist, and generated-dataset observations must match manifest file/directory/byte totals with zero issues before publication.

## Repository speed and memory review

[200 ranked candidates](REPOSITORY_PERFORMANCE_IMPROVEMENTS.md) records 50 implemented source targets with overlapping savings and 150 profiling candidates. [Red/green evidence](TDD_REPOSITORY_PERFORMANCE_EVIDENCE.md) separates allocation budgets from process memory. Reproduce with `pwsh -NoProfile -File benchmarks/run-winblaze-refresh.ps1 -Runs 10`, then `node benchmarks/summarize-repository-performance.mjs`, `node benchmarks/render-report.mjs` and `node benchmarks/verify-repository-performance.mjs`. The original baseline is preserved under the path in `artifacts/repository-performance-baseline-path.txt`. The report lists every medal event and product value without dropdowns, and retains every before/after metric, including slower results.

## Detailed parity verification

The [44-capability audit](FEATURE_PARITY_AUDIT.md) uses installed-version evidence, explicit scoring scopes and per-product verification bases. Run `node benchmarks/verify-feature-parity.mjs` and `pwsh -NoProfile -File benchmarks/verify-parity-interop.ps1 -Runs 10`. Functional interop probes record output correctness rather than performance medals. Raw-MFT runtime validation requires a separately accepted administrator prompt; MTP runtime validation requires connected hardware.

## Complete current-build supplemental refresh

After the main all-product run, use pwsh -NoProfile -File benchmarks/finish-parity-refresh.ps1 -Runs 10 for the affected WinBlaze feature, seven-profile engine/GUI, full-cache reopen, incremental, index-memory and public-operation categories. It also reruns first-idle startup for all three products. Release builds and acceptance checks must finish before timed runs. Runners share a measurement mutex and preserve prior result files. Version/hash changes start separate identities; missing telemetry is never replaced by zero.

The expanded standard feature plan contains 34 WinBlaze workloads, four WinDirStat workloads and five WizTree workloads (430 n=10 observations). Detailed JSON/TSV/interchange, four verified-content hashes, regex/allocation search, storage costs, multi-root scans, owned NTFS maintenance and owned VHD compaction have explicit scopes and output checks. Missing three-product measurements or differing scopes earn no performance medal.

Startup resource changes are documented separately in STARTUP_RESOURCE_IMPROVEMENTS.md. Fifty concrete targets share five production changes; per-target savings cannot be added. Benchmark first-idle CPU/memory and visible-ready CPU/memory separately. Do not delay readiness, trim working sets or alter the measurement protocol to improve a score.

### Startup peak working set focus

[Focused report](STARTUP_PEAK_WORKING_SET.html) shows all four first-idle measurements, medals, and every peak-memory observation. [Qualification notes](STARTUP_PEAK_WORKING_SET_PLAN.md) distinguish tested loader changes from unproven memory savings. Run `python benchmarks/verify-startup-import-budget.py` after the Release UI build and `node benchmarks/verify-startup-peak-report.mjs` after recording a current n=10 batch. The focused results do not replace the complete 35-category refresh.

### Nonempty dense-folder timing variability focus

[Focused report](DENSE_FOLDER_VARIABILITY.html) shows all three product scores, every retained n=10 timing, and executable identities. [Qualification notes](DENSE_FOLDER_VARIABILITY_PLAN.md) distinguish the verified lazy-buffer allocation reduction from unproven timing savings. Validate exports with `python benchmarks/verify-empty-scale-exports.py fanout-large`, render with `node benchmarks/render-empty-scale-report.mjs benchmarks/dense-lazy-pipeline-confirmation.json fanout-large`, and check the current-binary gold result with `node benchmarks/verify-empty-scale-gold.mjs benchmarks/dense-lazy-pipeline-confirmation.json fanout-large`. Quick and Empty-file focused reports use the same batch with profiles `tiny` and `scale`.

### Standard-user full-drive private memory focus

[Focused report](FULL_DRIVE_PRIVATE_MEMORY.html) and [qualification notes](FULL_DRIVE_PRIVATE_MEMORY_PLAN.md) describe the completed n=10 comparison: WinBlaze 479.13 MiB (gold), WinDirStat 542.12 MiB, WizTree 913.07 MiB. All runs, including the cold first run, remain included. Four allocation changes passed red/green tests; the observed WinBlaze baseline reduction is 23.14%, with uncontrolled live-volume/cache differences.

Use `node benchmarks/render-full-private-memory-report.mjs benchmarks/full-private-memory-current.json --require-gold` to validate identities and render the focus. Current Quick/Empty/Dense reports use `full-private-memory-current.json`; they show the latest variability regressions rather than earlier gold results. `node benchmarks/verify-medal-sample-count.mjs` checks that each medaled metric has ten valid observations per product. Older UI/feature identities require a separate refresh and receive no fresh-build medals.

### Continuous results and competitor records

`render-report.mjs` keeps the original 35 categories visible throughout a refresh. `live-medal-results.json` retains each last validated ten-run value. WinBlaze updates each metric when ten accepted observations from the new batch are available; pending rows show their previous source and binary identity. Historical values cannot satisfy the completed all-gold gate.

`competitor-best-results.json` retains the lowest validated ten-run statistic independently for every WinDirStat and WizTree medal metric, including CPU, memory and timing variability. Slower results and partial batches cannot replace a record. Product version, file version and executable SHA-256 define separate baselines. Every selected metric preserves its ten observations and archived source; different metrics may select different batches. Raw latest observations remain separate. Both HTML and Markdown use these official values.

Check this policy with `node benchmarks/verify-live-medal-results.mjs`, `node benchmarks/verify-competitor-bests.mjs`, `node benchmarks/verify-live-report.mjs` and `node benchmarks/verify-best-report.mjs`.
# Iterative medal refresh policy

`run-comparison.ps1` and `artifacts/run-all-gold-iteration.ps1` now refresh WinBlaze
on every iteration. Complete WinDirStat and WizTree bests remain the official
numbers for their recorded version and executable hash. A changed identity or
missing, incomplete, or unavailable archived baseline selects that competitor for
a fresh baseline. Use `-RefreshCompetitors` only for an explicitly requested full
three-product rerun. All 35 report rows remain visible during collection.

The retained administrator worker reads the batch's selection when it calls its
fixed elevated competitor output, then continues to run WinBlaze's elevated
measurements. It does not need another UAC prompt to apply this policy. Competitor
exports used as inputs for WinBlaze import feature checks can still be generated;
those are fixture preparation, not new competitor benchmark observations.

Validate the selection with `powershell.exe -NoProfile -ExecutionPolicy Bypass
-File benchmarks/verify-baseline-reuse.ps1`.
