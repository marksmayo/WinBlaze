# Real-World Scale Calibration — C:\ (2.3M files)

Evidence for the TODO item "run real-world scale calibration on large local
disks". Measurements taken 2026-07-05/06 on the primary development machine
(Windows 11 Pro 10.0.26200, NVMe system drive, ~465 GB volume, warm filesystem
cache unless noted), scanning the live `C:\` system volume.

## Dataset

| Metric | Value |
|---|---|
| Files | 2,270,819 (MFT) / 2,338,844 (walk) |
| Directories | ~546,300 |
| Logical bytes | 464.3 GB |
| Allocated bytes | 468.8 GB |

Counts cross-checked between the directory-walk backend and the raw-MFT
backend. Walk file counts run ~3% higher because each hardlink is counted at
every link; the MFT figure counts records once.

## Scan performance overhaul (2026-07-06)

Backend-only release measurements on the same volume, before and after the
overhaul (channel batching, identity hashing, MFT emit rewrite, walk
enumeration work — see `benchmarks/perf-overhaul-baselines.md`):

| Scenario | Before | After |
|---|---|---|
| Raw MFT producer (`mft_scan_repro`) | 143.7 s | **8.3–9.0 s** |
| FFI end-to-end incl. persist + tree (`ffi_scan_repro`) | 125.6 s | **10.0–12.0 s** |
| Directory walk (`directory_walk_benchmark`) | 13.9–14.4 s | **13.9 s (noise-bound)** |
| In-app C:\ scan, Debug UI, idle-to-idle | 90–130 s | **39.7 s** |

The MFT read itself (4.8 GB, 4,705,324 record slots) is now the dominant
cost of the fast path; the walk is enumeration-syscall-bound. The raw-MFT
backend is the default for volume roots and beats the walk by ~40%; the walk
covers subtree scans and non-NTFS volumes.

Two long-standing issues surfaced and fixed during the overhaul:

- `backend_hint()` silently re-ran backend auto-selection for any non-empty
  root, so explicit `DirectoryWalk` requests were upgraded to the MFT path.
- The streaming MFT path never emitted root record 5, so persisted models
  mis-rooted at whichever top-level directory sorted first (`$Extend`), and
  it picked DOS 8.3 aliases (`PROGRA~1`) over Win32 names for directories.

## In-app UI behavior (Debug build, full C:\ scan)

| Metric | Start of cycle | End of overhaul |
|---|---|---|
| Idle-to-idle scan duration | ~52 s tree-ready; 90–130 s total | **39.7 s total** |
| Post-scan index flush | 7.4 s | **&lt;1 s** |
| Working set, full model loaded | ~1.4 GB | **~0.92 GB (peak 1.0 GB)** |
| Worst single UI-thread stall | 26 s observed | **&lt;1 s** (peak frame 996 ms at completion reload) |
| Correctness counters | — | issues=0, files=2,270,819, directories=546,315 |

UI latency evidence comes from the app's own diagnostics counters
(`peak flush`, `peak frame` in the runtime metrics panel), which measure the
UI thread directly.

## Correctness notes

- Deleted MFT records (no in-use flag) initially inflated results by ~1.7M
  files / 300 GB; filtered.
- Named `$DATA` streams (`$BadClus:$Bad` spans the volume sparsely)
  initially added ~260 GB; only the unnamed default stream is counted.
- Sizes for heavily fragmented files live in MFT extension records; these
  are merged into their base records (previously reported as 0), including
  extension records that arrive after the base already streamed (the file
  re-emits with corrected sizes).
- Directory symlinks/junctions were previously miscounted as 0-byte files by
  the walk; they are now real directory rows, and reparse points are not
  descended by default (following the `Documents and Settings` junction
  would double-count `C:\Users`).
- MFT directory names prefer the Win32 `$FILE_NAME` namespace; DOS 8.3
  aliases no longer surface in the tree.
- Raw walk enumeration (`FindFirstFileExW` + large fetch) verified
  count-identical against `fs::read_dir` on the full volume.
