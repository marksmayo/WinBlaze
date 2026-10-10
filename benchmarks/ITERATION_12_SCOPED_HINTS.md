# Scoped allocation advice and telemetry repair

Iteration 12 completes all 35 original categories with fresh ten-observation
WinBlaze evidence: **27 gold / 7 silver / 1 bronze**. All 490 feature
observations and report/freshness/definition/sample/publication checks pass.
All 70 official competitor metrics are unchanged from iteration 11. The
administrator worker explicitly skips both unchanged competitors; no new UAC
approval is needed. The all-gold gate remains RED.

Final validation initially fails because two short scan/CSV processes provide
no usable CPU samples; most managed memory snapshots also return zero after
exit. The original cohort and report are preserved under
iteration-12/initial-telemetry-incomplete. Missing observations are not filled
with zero and no individual favorable observations are selected.

A public owned-process regression deliberately waits for a short process to
exit before collecting counters. The managed process-list implementation
loses peak memory and fails. Querying its retained kernel handle passes:
GetProcessTimes supplies CPU and GetProcessMemoryInfo supplies real counters.
Unavailable counters stay nullable; zero CPU is accepted only when its API
query actually succeeds. Monotonic merging preserves earlier observations.
Private memory uses the largest observed current PrivateUsage, not the
different peak-commit counter. The observer captures immediately, at the
existing 10-ms live polling interval, and at completion. Timing boundaries,
category definitions and sample requirements are unchanged.

The independent replacement scan/CSV cohort has ten positive memory
observations and ten available CPU observations. Other completed rows are
preserved byte for byte. Resume/final validation passes. This is a measurement
repair, not evidence of a product speed improvement. Shipping UI still misses
the unchanged 2000-ms timing budget at 2445 ms.

## Returning-volume allocation hypothesis

The standard full-volume workflow's first observation is 24.775 s / 660.3 MiB;
later observations are approximately 12-14 s / 460-462 MiB. The CLI did not
change in the formatter slice. Existing capacity advice already includes 5%
growth room, but its single-scope file discards the previous root when another
folder is scanned. The variability definition remains IQR/median; a single
outlier alone does not explain every variation.

The public alternating-root test is actually RED: save advice for A, save B,
then reload A returns None instead of its bounded estimate. The implementation
now retains up to 32 exact root/rules scopes in the same advisory file.
Updating one scope preserves others; overflow evicts the oldest update.
Unchanged counts require no write. Each scope retains the original 5% growth
allowance and 512-MiB record-storage estimate limit.

Legacy WBCHNT01 files remain readable, and a single entry retains that format.
The cache upgrades to WBCHNT02 when it contains several scopes. Reading is
bounded at 131,724 bytes, including 32 maximum-length scope strings. Invalid
lengths/counts/UTF-8, duplicate scopes, overflow, truncation and trailing data
are cache misses. Invalid save inputs preserve the previous file. Advice
controls allocation only; every file is still scanned and exported anew.

Ten focused advice tests pass, including bounded retention, migration,
corruption and unchanged write-access behavior. The real FFI rescan/export
test also passes across A/B/A, a newly added file and hint write failure.
Workspace tests, examples, rustfmt, strict all-target/all-feature clippy and
**92.46% Rust line coverage** pass.

Native release and release examples pass. Current CLI is C5F0B100, native DLL
E8B72AAB (same DLL staged in the UI output), unchanged UI executable AA3F83A5.
Previous A3B44E52/F1C65B06/AA3F83A5 builds are preserved in
iteration-12/pre-scoped-hints-shipping. Local UI validation session 48380
finishes with exit 1: native contracts and UI functional checks pass, but the
unchanged tiny UI timing budget fails at 2,424 ms against 2,000 ms. The full
local gate remains RED. Process-memory/variability benefit is not yet proven.

Iteration 13 is running the original 35 categories for WinBlaze only, with
ten observations per category. Batch 20261007T231325824Z retains all 35
visible report rows while completed cohorts replace WinBlaze values. Both
competitor baselines remain frozen unless their installed identity changes
or a required baseline is incomplete or invalid. Root session 59906 owns
this run; do not rebuild or start a second measurement run while it is live.
