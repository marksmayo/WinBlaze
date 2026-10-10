# UI startup red/green evidence

## Routine tracing

Public logging interface: `TraceStartup`. Windows test `tests/native/startup-trace.cpp` executes 50 real calls and observes `GetProcessIoCounters`.

- RED: old behavior produced 50 writes with no diagnostic opt-in; test exited 1 (`Unexpected trace writes: 50`).
- GREEN: release code caches the process launch choice and returns before path resolution, UTF-8 conversion, file size/rotation queries, append opens or writes. Default test measured zero writes.
- Diagnostic compatibility: `WINBLAZE_STARTUP_TRACE=1` measured 50 writes for the same messages.
- Failure compatibility: `ReportFailure` still persisted the actual failure JSON when routine tracing was disabled.

## UI and output regressions

Release build passed after restoring the project's declared missing NuGet dependencies. Actual Windows UI tests passed: maximized window, directory guidance, offline 9,001-file CSV reopen/JSON/dense PNG/largest-file paging, and advanced filtered CSV export including invalid-input rejection. All 210 feature observations and negative output validators passed. No Rust source changed; the previous 93.29% coverage result remains applicable to the unchanged backend.

## Measurements

Ten adjacent before and after runs, all observations retained:

| Metric | Before median | After median | Change |
|---|---:|---:|---:|
| First idle, hidden-window proxy | 92.095 ms | 78.800 ms | -14.4% |
| Visible maximized window with enabled scan controls | 367.900 ms | 366.870 ms | -0.3% |

The latter is effectively unchanged at 10 ms polling resolution and includes UI Automation overhead. Both batches use uncontrolled caches; no causal or cold-start claim. Visible readiness uses empty isolated app data and excludes restored catalogs. Raw observations are under `artifacts/ui-startup-optimization/`, with current visible observations in `current-ui-ready-results.json`.

The 50-candidate audit is a review, not 50 implemented independent gains. Only one production change has qualified from this pass; hidden-pane construction, duplicate refreshes and other candidates need profiling and behavior evidence before changes are justified as low risk.

## Startup CPU / working-set resource targets — 4 October 2026

Four independent PE import regressions failed while VirtDisk, D3D11, D2D1 and DWrite were eager executable imports. Each DLL was moved to the supported first-use binding mechanism and rebuilt before its regression passed. The rendering framework can still load these DLLs independently; this is not a guarantee of absent framework module residency.

The public bounded color-resource regression failed with the cache absent, then passed: 100 identical ARGB requests construct once and share identity, alpha is distinguished, retention stops at capacity, and a throwing factory preserves previously created objects. The UI brush factory now uses that cache. Fifty concrete source targets are documented in STARTUP_RESOURCE_IMPROVEMENTS.md: four imports plus 46 direct/delegated shell color bindings, through five shared changes. They are not fifty independent or additive savings estimates.

Actual PNG rendering/grayscale dimensions, owned detached VHD compaction, complete search, native shell popup/cancel and maximized usable-window startup pass after the rebuild. The first search rerun passed its behavior checks but encountered a process-exit cleanup race; a subsequent complete rerun exited successfully. Rust logic did not change in this slice; the existing 92.39% final coverage result remains applicable.

All timing reruns use n=10. First-idle results are mixed: observed time 85.665 → 75.110 ms; working set 35,915,776 → 35,338,240 bytes; CPU 117.1875 → 125.0000 ms. No first-idle CPU saving is established. The benchmark protocol was not changed, delayed or trimmed. Separate visible-ready measurements remain distinct from the first-idle proxy.

## Adjacent startup validation and isolated output regression

Explicit experimental output arguments initially exposed a runner regression: the requested file was absent and the default published file was overwritten. Added real OutputPath/HistoryPath parameters, restored the published snapshot, then completed control/optimized first-idle and visible-ready runs (n=10 each). A wrapper verified published startup results and version-history hashes stayed unchanged.

Adjacent observed medians: first-idle CPU 132.8125 → 117.1875 ms, working set 36,890,624 → 34,852,864 bytes, latency 89.755 → 80.275 ms; visible-ready latency 433.605 → 414.970 ms, CPU 476.5625 → 453.125 ms, working set 128,833,536 → 127,696,896 bytes. Sequential batches and uncontrolled scheduling limit attribution. Retained all dated results, including the earlier visible-ready regression. Fifty source targets share five production changes; these are not fifty independently measured gains.
