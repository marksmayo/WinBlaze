# Iteration 26: integrate the measured startup improvement

The Release/x64 project now builds and stages the full OS XAML desktop host.
Debug remains WinUI, and `/p:WinBlazeUiHost=WinUI` selects the Release fallback.
The project currently builds the WinUI output before the OS host; this adds build
work but preserves the existing restore path. Previous binaries are backed up
before staging. `winblaze-host.json` records source and binary identities.

## Import policy RED/GREEN

The earlier isolated builder omitted COMCTL32 and VirtDisk from the project's
six delayed imports. The existing startup import budget failed against that
binary, as did the VirtDisk first-use check. The builder now reads the project's
single DelayLoadDLLs declaration instead of maintaining a second list. A fresh
full build passes actual PE import checks for SHELL32, COMCTL32, VirtDisk,
d3d11, d2d1 and DWrite, plus icons, restored controls, visible expansion and
the owned focus/message-routing contract.

Evidence: `artifacts/all-gold-loop/iteration-26/import-budget-red.log`,
`virtdisk-import-red.log`, and `project-import-build.log`.

## Runtime and package evidence

Nine real feature checks pass against the preceding B9C5A4BA candidate.
Its persisted 20,000-file catalog reopens through the UI. Idle close, close
during an observed scan, and cancel then close exit normally. Native caption
actions are accessible through MSAA on both the previous shipping UI and the
candidate. The new reusable check enumerates actual child IDs, accepts localized
names, and invokes minimize/maximize/restore/close with observed native states.
UIA still exposes no caption buttons; full Narrator coverage is not established.

The changed BD37B481 build passes the shell context-menu workflow, four-binary
portable package contract, and a real portable updater transition from the old
AA3 build. The updater waits while the old process is alive, copies all four
verified binaries, relaunches the new UI, and removes its own staging directory.
The isolated fixture does not exercise a network release download.

A Virtual Disk negative-path check observes VirtDisk loaded on first use,
unchanged fixture bytes, a reported Windows status and normal shutdown. Positive
VHD creation/compaction remains unverified: the standard-user creation attempt
returned access denied. Default-palette window captures were visually inspected;
full theme/high-contrast and physical keyboard coverage are not claimed.

## Startup evidence and production integration

The changed BD37B481 build's single canonical ten-run startup cohort passes the
unchanged resource verifier: median working set/peak 14.59765625 MiB, CPU
23.4375 ms, and first idle 33.755 ms. All ten CPU observations qualify. These
isolated measurements do not replace official product observations.

The subsequent project Release build stages UI SHA256
`D369A6EBE831F938B63BD971A735D86DB328522F802A7CA1372195669EBA2EC0`,
native `9FF4B2BAC5DAFD0160FF06874D21528044C7EDC2B9E1F2AC25996808834A9F62`,
CLI `D178578FE2024247A534B519107B481E3B170D8C83D988E7E276037C794C8215`,
and updater `66C6B59FD2BA257CBAE06C9202176A5DDC4C58A817F8A83C2FCD5F4B08A70799`.
Production icons, native caption accessibility and visible expansion pass.
The original-35 refresh started at 2026-10-08T08:58:29Z with ten observations
per cohort, unchanged definitions and frozen version-matched competitor bests.
Its live state is in `current-benchmark-batch.json` and
`artifacts/all-gold-loop/iteration-26/production-refresh.log`; no all-gold result
is claimed while it runs. All 35 report rows remain visible during refresh.

The old WinUI binaries are preserved under
`artifacts/ui-stage-backup-a6af1eb135d6477e8c4cba3f2659938a/`.
The default packaging path still includes existing WinUI sidecars. No new MSI
validation is claimed. The earlier full local gate's tiny-UI timing budget failure
remains recorded; it has not been erased by these narrower passing checks.

## Regression found by the production refresh

The first production batch stopped correctly on three failed unattended PNG jobs.
All three produced valid, distinct 1920x1080 images, then exited with access
violation `0xC0000005`. The regression test reproduces all three failures against
D369A6EB. Failed measurements and batch manifests remain archived; the stopped
batch is not treated as complete.

The OS host was closing its XAML island inline from a XAML dispatcher callback.
Close requests now post an owned WM_CLOSE, allowing the callback to return before
island teardown. The normal drain completes a pending close before draining owned
background work. The new candidate passes all three PNG modes with unchanged
image hashes and normal exit. An independent generated catalog fixture also
passes; the reusable PNG test is now part of the OS host build gate. Icons,
window/tree behavior, focus/message routing, idle close, active-scan close and
manual cancel followed by close pass.

The fixed UI `1F1E36D9818F2B4C982D29792907D02A425120BD1B607DA44CBD960C558614A1`
is staged in Release; native/CLI/updater identities remain as above. The failed
D369 build is preserved in `artifacts/ui-stage-backup-5d8ea9acc31f4371b3f4c3229cf3c1a2/`.
A fresh full WinBlaze-only refresh is running against the fixed build, with
`fixed-production-refresh.log` and `fixed-production-finalization.log` recording
progress. The finalizer dispatches administrator observations through the existing
worker only after the standard batch succeeds and frozen baselines validate.
No competitor rerun or new UAC is required. The provisional report remains
30 gold / 5 silver / 0 bronze across all 35 rows. The preceding cohort's startup
CPU contained one zero reading and remains pending with its previous official
value; it is not repaired by selecting replacement observations.

## Fixed-build cohort and supplemental recovery

The new fixed-build startup cohort has ten qualifying observations in every
startup category: working set/peak 14.57421875 MiB, CPU 31.25 ms, first idle
34.07 ms. All four startup medals are now gold. This is a new cohort against
changed product code, not replacement samples for the earlier zero CPU reading.
The live report is 31 gold / 4 silver / 0 bronze; the four remaining deficits
are workflow timing variability. Latest worse variability values remain official.

All thirty unattended PNG observations pass in the full feature batch. A separate
file-actions adapter run lacked completion output; the failed cohort is archived
and its cause is not claimed as established. Twenty diagnostic launches captured
matching managed/kernel zero exit statuses and completion output. The feature
runner now records terminal exit status explicitly and rejects unavailable status.
A selected-job repair replaces the entire affected ten-run cohort while preserving
other validated observations. All forty WinBlaze feature jobs now have ten
validated observations (400 total); raw MFT remains a separately labelled
administrator requirement. One restricted repair publication failed with access
denied; the approved repair completed normally and preserves that failed attempt.

The same original batch resumed at 09:24Z for GUI/cache/usable-ready and public
operation checks. Its logs are `resumed-production.log` and
`resumed-production-finalization.log`. Administrator dispatch still waits for
successful completion and uses the existing worker. This batch subsequently
completed: administrator iteration-27 finished at 09:41Z, and all four final
report validators passed. The final report has 31 gold, 4 silver and 0 bronze,
with all35 categories covered by fresh n10 WinBlaze observations. All70 complete
competitor best objects and official values match the frozen baseline exactly.
Evidence: artifacts/all-gold-loop/iteration-26/final-validation.json and
benchmarks/runs/2026-10-08T09-41-08-594Z/report-data.json.
The next scanner resource slice is separate from this measured cohort.
