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
