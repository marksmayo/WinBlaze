# Real-World Scale Calibration — C:\ (2.2M files)

Evidence for the TODO item "run real-world scale calibration on large local
disks". All measurements taken 2026-07-05 on the primary development machine
(Windows 11 Pro 10.0.26200, NVMe system drive, ~465 GB volume, warm filesystem
cache unless noted), scanning the live `C:\` system volume through the UI.

## Dataset

| Metric | Value |
|---|---|
| Files | 2,258,536 |
| Directories | 543,404 |
| Logical bytes | 463.8 GB |
| Allocated bytes | 468.6 GB |

Counts cross-checked between the directory-walk backend and the raw-MFT
backend (elevated). Walk logical bytes run ~6% higher because each hardlink
is counted at every link; the MFT figure counts records once.

## Directory-walk backend (default, non-elevated), warm cache

| Metric | Start of cycle | End of cycle |
|---|---|---|
| Folder tree ready after Start scan | ~52 s | **~22 s** |
| Event-pipeline drain (session → summary) | 34.8 s | **17.1 s** |
| Post-scan index flush | 7.4 s (after removing a duplicate 14 s + 14 s pair) | **0.9 s** |
| Snapshot file on disk | 562 MB | **323 MB** |
| Working set, full model loaded | ~1.4 GB | **~1.03 GB** |
| Worst single UI-thread stall | 26 s observed | **0.9 s** (peak flush) |
| Live tree first folders visible | n/a (feature added) | ~2 s |

UI latency evidence comes from the app's own diagnostics counters
(`peak flush`, `peak frame` in the runtime metrics panel), which measure the
UI thread directly.

## Raw-MFT backend (elevated), warm cache

| Metric | Value |
|---|---|
| Session → summary | 88–130 s across three runs |
| Post-scan flush | 0.9–1.1 s |
| MFT size read | 4.8 GB (4,705,324 record slots) |

The MFT path is currently slower than the walk on a warm cache because both
backends share the same per-record event pipeline, and the MFT path
additionally reads the full 4.8 GB MFT and parses ~2.4M unused record slots.
Its advantages today are correctness (allocation sizes, MFT timestamps,
single-counted hardlinks) and cold-cache behavior (no per-directory
enumeration syscalls). Event-pipeline batching is the recorded next step for
bringing both backends toward WizTree-class times.

## Correctness notes

- Deleted MFT records (no in-use flag) initially inflated results by ~1.7M
  files / 300 GB; filtered.
- Named `$DATA` streams (`$BadClus:$Bad` spans the volume sparsely)
  initially added ~260 GB; only the unnamed default stream is counted.
- Sizes for heavily fragmented files live in MFT extension records; these
  are merged into their base records (previously reported as 0).
- Directory symlinks/junctions were previously miscounted as 0-byte files by
  the walk; they are now real directory rows, and reparse points are not
  descended by default (following the `Documents and Settings` junction
  would double-count `C:\Users`).
