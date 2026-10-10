# Compact retained file paths: first red/green slice

The published-catalog resource test exercises the public index transaction:
reserve 16,384 records, insert complete names/paths and extreme timestamps,
publish owned records, verify every ID/path/time, and count retained allocations.
It fails before implementation with **2,768,896 bytes** against **2,641,920**.
No paths or timestamps are removed to meet the budget.

`FileRecord.full_path` now uses `Box<str>`. Paths remain owned and immutable
between record replacements; explicit conversions preserve mutable String
results at search, display and lineage boundaries. Directory paths remain
String. Existing snapshot writers/readers retain the same format. Full paths
from imports and old snapshots remain available. C ABI structs are unchanged.

The resource test now passes, and compiled x64 FileRecord layout falls from
136 to **128 bytes**. This first slice saves eight bytes per record, around
13.7 MiB at the observed full-volume record count before allocator effects.
It does not establish a process-memory or medal improvement. The larger
24-byte/roughly-41-MiB hypothesis still needs lossless timestamp grouping.

The complete Rust workspace initially exposed a test-classifier collision:
ScanEvent is now 128 bytes, so 512-event arenas are exactly 64 KiB. The old
directory scratch test counted those as scratch (four 16-KiB allocations,
one 32-KiB allocation, six 64-KiB allocations; total 491,520 bytes).
Its test-only pipeline batch size now places event arenas above the measured
scratch range. The default directory worker budget and 262,144-byte limit
remain unchanged; no legitimate scratch size is excluded. The focused test
and full suite pass after this separation.

Validation completed: workspace tests, workspace example tests, rustfmt,
strict workspace Clippy, 92% coverage gate (**92.41% lines**), release native
build and Release WinUI build (zero warnings/errors). Logs and pre-change
shipping binaries are under `artifacts/all-gold-loop/iteration-11`.
Native/UI local compatibility checks and a WinBlaze-only original-35 rerun
follow. The official report retains all 35 earlier values until new samples
are published; unchanged competitor baselines stay frozen.

The first local-gate run passes native checks, all ten path/window UX cases,
smoke/incremental checks and duplicate/ACL/watcher/move/recycle parity. It then
fails in an outdated import test looking for `Open CSV scan...`. The unchanged
app now exposes `Open scan (CSV, TSV, JSON, MFT)...`, followed by a timestamp
options dialog; PNG export also has a dimensions dialog. The test now follows
those existing controls, preserving its 9,001-file count, allocation/date,
JSON/PNG and cleanup-pagination assertions. Its native dialog helper validates
the owned process, visible dialog and native class, and uses child-window
messages instead of global keyboard input. The focused complete import test
passes. The final local gate passes its native/UI correctness checks, then its
tiny timing budget fails twice (2440/2434 ms versus 2000 ms). The script returns
zero despite those child failures; this is not a passed local gate.

Invoke-Step now resets and checks the global native exit code. An initial local
variable check still swallowed an intentional exit-7 build child. The corrected
global check rejects that child through the real entrypoint, returning process
exit 1 before later steps (`local-failure-propagation-fixed.log`).

Serialized observer probes retain the original UI boundary and show the old
backend at 2460 ms/provider Invoke 2007 ms/native scan 4 ms, and the new backend
at 2415/2004/2 ms. These are diagnostic single runs, not a statistical speed
claim. The earlier old-backend probe overlapped the start of a validation test
and is kept separately; clean serialized probes drive this diagnosis.
Microsoft's [InvokePattern guidance](https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.invokepattern.invoke)
allows a helper thread for blocking providers. A fully joined helper-task probe
still takes 2175 ms and fails the unchanged 2000-ms budget. That experiment is
reverted; synchronous invocation and thresholds remain. Optional phase capture
is retained, and the UI budget issue remains unresolved.

New release identities: CLI A3B44E52, native DLL F1C65B06 (same DLL staged in
the UI output), unchanged UI executable 675F52F7. Full hashes are recorded in
`compact-path-shipping-identities.json`.

The authorized WinBlaze-only original-35 rerun is active in batch
`20261007T215303992Z`; competitors reuse complete matching baselines. All 35
rows remain visible during collection. Standard full-volume n=10 is complete:
median 12.29 s, CPU 43.12 s, peak working set 460.20 MiB, peak private 473.24
MiB. The latest peak working set is below retained WinDirStat 473.29 MiB;
full-volume timing variability also earns gold in this cohort. The provisional
report moves to 29 gold/5 silver/1 bronze. Feature/admin completion and final
freshness/sample/report verification are still required. First-run peak 660.9
MiB and all other observations are retained; no earlier WinBlaze best replaces
latest values. Sequential improvement does not establish a causal timing gain.

The batch is now complete: all 35 categories have fresh ten-observation
WinBlaze evidence, 490 feature observations pass, and freshness, original
definitions, retained bests, report, focus, publication and sample checks pass.
Final standings are **29 gold / 5 silver / 1 bronze**. Full-volume timing
variability is **4.59018%**, below WinDirStat's retained **5.23041%**. Startup
working set remains bronze at the latest **35.25586 MiB**; it is not replaced
with an earlier better WinBlaze observation. The all-gold gate remains RED.

The administrator competitor skip did not take effect in this batch. Its
relative output path was resolved against the CLR working directory rather
than PowerShell's location. The worker therefore collected both competitors
again. Comparing all 70 official competitor metrics with iteration 10 shows
**66 unchanged and 4 improved**, with no regressions. All raw observations
remain available.

A safe public-entrypoint regression uses an empty profile list, an independent
CLR working directory, and the worker's relative output path. It fails before
the fix without launching any competitor. Resolving the path against the
PowerShell location makes it pass in two consecutive runs, preserving the
existing elevated evidence byte for byte. The retained administrator worker
reads this script on its next request; no new UAC approval or worker restart
is required. Future loops reuse complete version-matched competitor bests.

Final evidence is archived in iteration-11/final-validation.json. The separate
local UI timing budget remains unresolved; benchmark validation does not make
that gate pass.
