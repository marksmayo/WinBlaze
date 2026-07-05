# Scan Performance Overhaul — C:\ Baselines

Release builds, warm filesystem cache, C:\ (~2.34M files / 547k dirs), 2026-07-06.
Machine: dev laptop, Windows 11 Pro. Re-measure on this machine only; numbers move
±20% with desktop load.

## Step 0 baselines (before any pipeline change)

| Scenario | Example | Time | Counts |
| --- | --- | --- | --- |
| Directory walk (forced) | `directory_walk_benchmark C:\` | 13.9 s / 14.4 s | 2,338,844 files / 546,926 dirs |
| Raw MFT producer | `mft_scan_repro C:\` | 143.7 s | 2,279,867 files / 545,558 dirs |
| FFI end-to-end (auto → MFT) | `ffi_scan_repro C:\` | 125.6 s | root mis-picked `C:\$Extend` (bug) |

Walk `first_event_ms` = 12; `summary_ms` == `completed_ms` (in-process drain keeps up).

## Findings that reshaped the plan

1. **`backend_hint()` dropped explicit backend requests** (`types.rs`): any non-empty
   root re-ran auto-selection, so *every* C:\ scan since the raw-volume MFT reader
   landed (`803fca1`) took the MFT path — including the "walk" benchmark. The
   apparent 10× walk regression (130–149 s) was the MFT path in disguise. Fixed:
   explicit `DirectoryWalk` now always wins.
2. **Raw volume reads succeed without elevation on this machine**, so the MFT path
   is not gated to elevated runs; its ~140 s cost (quadratic pending-entry rescan,
   Step 3 target) is the default C:\ experience until fixed.
3. **MFT-derived snapshots mis-root at `$Extend`**: root record 5 is missing from
   the persisted model, so `choose_root` falls back. Fix folded into Step 3.
4. MFT vs walk counts differ by ~59k files / ~1.4k dirs — reconcile during Step 3
   verification.

## Targets

- Walk: < 8 s warm (Steps 1, 2, 5)
- MFT: read-bound ~15–25 s and beating the walk (Step 3), correct root

## Results (after Steps 1-3 and 5; Step 4 skipped, gate not met)

| Scenario | Baseline | Final |
| --- | --- | --- |
| Raw MFT producer | 143.7 s | 8.3-9.0 s |
| FFI end-to-end (auto - MFT) | 125.6 s | 10.0-12.0 s |
| Directory walk (forced) | 13.9-14.4 s | 13.9 s best (noise-bound) |
| In-app C:\ scan (Debug UI, idle to idle) | 90-130 s | 39.7 s |

Step contributions to the MFT path: identity hashing + pre-sizing 143.7 s ->
58.3 s; emit rewrite (orphan buckets, files never stored, running summary)
58.3 s -> 8.3 s. Channel batching cut ~2.9M sends/allocs per scan but the
in-process drain was already keeping pace, so its wall-clock effect is
mostly on the FFI drain. Walk-side allocation cuts and the raw
FindFirstFileExW large-fetch enumerator were verified count-identical
against fs::read_dir; wall-clock deltas sat inside this machine's noise
band (desktop load varies runs by +/-40%).

Step 4 (persist Vec fast path) was skipped per its gate: after Steps 1-3
the FFI summary lands at ~7.5 s vs ~9 s for the producer alone, so persist
inserts fully overlap the producer, and the ~2.3 s summary-to-completed gap
is the final snapshot flush, which a Vec-backed transaction would not
touch.

Correctness fixes that fell out of the work: explicit DirectoryWalk backend
hints are honored; MFT-derived trees root at record 5 (was `$Extend`);
Win32 names win over DOS 8.3 aliases (was `PROGRA~1`); extension-record
size corrections re-emit after the base file already streamed.

Release UI medians re-recorded 2026-07-06 (`winblaze-release-medians.json`):
tiny 6 ms / fanout 11 ms / scale 36 ms median scan duration, peak frames
<=37 ms, working sets 157-184 MB.
