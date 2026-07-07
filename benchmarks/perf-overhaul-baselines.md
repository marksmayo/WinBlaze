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

## Round 2 (2026-07-06, same day): pipeline overlap + deferred persist

Phase decomposition (`mft_phase_bench` example) showed the producer fully
serial: 2.5 s raw volume read (the device/cache floor; 4-thread parallel
reads only gained 9%, so queue depth does not help this path), +0.7 s
bounce-chunk memcpy, +1.9 s parse, +0.7 s emit = 5.8 s.

Changes:
- VolumeMftReader serves large sector-aligned reads directly into the
  caller's buffer (bounce chunk only for small/unaligned tails).
- stream_ntfs_entries overlaps I/O with compute: a read-ahead thread fills
  pooled 64 MB blocks while the main thread runs fixups + parse + emit.
- Full-scan snapshots persist AFTER Completed reaches the UI, serialized
  from the already-built tree model (byte-identical format, parity test),
  gated so an immediate incremental rescan waits for the write.
- Model build aggregates extensions via borrowed keys (no per-file String).

| Scenario | Round 1 | Round 2 |
| --- | --- | --- |
| MFT producer, null sink (`full_ms`) | 5.8 s | **2.7-3.0 s** (= read floor) |
| Controller + channel (`mft_scan_repro`) | 8.3-9.0 s | **3.1 s** |
| FFI end-to-end to Completed | 10.0-12.0 s | **4.7-5.2 s** |
| In-app Release UI C:\ scan, idle to idle | 11.3 s | **5.2 s** |

Remaining structure at ~5 s end-to-end: ~3 s producer (I/O-bound), ~0.9 s
consumer lag (event log + UI forwarding + transaction inserts), ~1.1 s tree
model build at Completed. The snapshot write (~1.5 s) now lands after the
UI shows done.

## Round 3 (2026-07-07): squeeze the serial post-producer work

The producer is read-bound (Round 2 established the ~2.5 s device floor, and
4-thread parallel reads still only gain ~9 %), so Round 3 targeted the serial
work that runs *after* the producer: the drain, the tree-model build, and the
snapshot write. 17 changes landed; the load-bearing ones:

- Pre-size the index transaction maps from the first Progress event's total
  record count (was empty → ~20 rehashes of millions of entries mid-drain).
- Tree build sorts a decorated `(id, index)` array and gathers each record
  once, instead of pdqsort swapping ~100-byte records O(n log n) times.
- Extension aggregation (2.3M files) fanned across worker threads.
- Snapshot serialization batched to one `write` per record (was ~10).
- MFT read trimmed to the $DATA valid-data-length; sector fixups folded into
  the parse workers; unchecked LE reads for in-bounds header fields; UTF-16
  names decoded straight into the String (no intermediate `Vec<u16>`).
- Directory `CString`s built once at push; hot events dispatched to typed
  emitters; incremental remap rewrites ids in place (no second record clone).

UI: the treemap render used to rebuild the entire D3D device / DXGI factory /
swapchain / D2D device / DWrite factory+format **on every render** (every
dirty/resize tick). The stack is now cached and created once; only the
swapchain + target bitmap rebuild on resize, the panel binds once, and any
failure resets the stack for device-lost recovery. This removes resize jank.

| Scenario | Round 2 | Round 3 |
| --- | --- | --- |
| MFT producer, null sink (`full_ms`) | 2.7-3.0 s | **2.8 s** (unchanged; = read floor) |
| FFI end-to-end to Completed | 4.7-5.2 s | **3.4-3.6 s** |
| — producer + drain (`summary_ms`) | ~3.9 s | **2.8-3.0 s** |
| — model build (Completed − Summary) | ~1.1 s | **~0.6 s** |
| In-app Release UI C:\ scan, idle to idle | 5.2 s | **~4.0 s** |

In-app figure is the mean of 3 UIA-driven Release-UI runs (elevated, MFT
fast path): wall-clock click→idle 4.02/4.05/4.06 s, app-reported scan
duration ~3.5 s (the extra ~0.5 s is the post-scan treemap paint).

The producer number is flat by design — it was already read-bound, so the
parse-side changes (parallel fixups, unchecked reads, name decode) sit inside
the read floor's shadow; they cut CPU and allocations without moving the wall
clock. The end-to-end win is entirely from the drain + model-build shrink.

Evaluated and declined (recorded so they are not re-attempted): unbuffered
MFT reads (`FILE_FLAG_NO_BUFFERING` forfeits the warm page cache re-scans
rely on and needs aligned buffers); boxing large `ScanEvent` variants (the
batch moves by pointer through the channel, so boxing only adds allocations);
color-batching treemap tile fills (`SetColor` is near-free and reordering
breaks the nested draw order).

Numbers are warm-cache, best-of-3, and swing ±20-40 % with desktop load
(a mid-work run under ~20 % higher background load showed the same producer
at ~3.95 s) — compare rounds only from a quiet machine.

## Competitor comparison — live C:\ (2026-07-07)

Same volume and machine as above (C:\, ~2.9M MFT records / ~2.3M files /
547k dirs / 464 GB, warm cache, elevated). GUI tools were timed by a
CPU-plateau probe (`scratchpad\time_gui_scan.ps1`: launch → sustained
CPU-idle = scan finished and the view is populated); WinBlaze uses its own
reported scan duration plus UIA idle detection.

| Tool | Backend | Scan → interactive (warm C:\) | Notes |
| --- | --- | --- | --- |
| **WinBlaze** (engine) | NTFS MFT | **~2.7 s** | `mft_scan_repro`, scan → summary; 3 runs 2.73/2.73/2.75 s |
| **WinBlaze** (in-app) | NTFS MFT | **~4.0 s** | Release UI idle→idle incl. tree + treemap; app-reported ~3.5 s |
| WinDirStat 2.6.0 | directory walk (multithreaded) | ~10.5 s | 3 runs 9.9/10.6/10.6 s, ~60 s CPU across cores |
| WizTree 4.31 | NTFS MFT | raw scan ~2–3 s; scan→interactive ~14–55 s | see caveat |

**WizTree caveat:** its raw MFT read is fast and architecturally comparable
to WinBlaze's, but the GUI then materializes the full 2.9M-file list + treemap
up front, so scan-to-interactive is much higher and very noisy (CPU-plateau
runs: 13.6 / 39.1 / 56.6 s). Its CLI (`/export`) scan **+ CSV export** of all
files was 28–35 s, but that is dominated by writing a 437 MB / 2.9M-row CSV,
not scanning. WizTree's own status-bar scan time is the fair figure but is not
UIA-exposed, so a precise number needs a manual stopwatch — the repo recorder
accepts it via `record-competitor-baselines.ps1 -WizTreeElapsedMs`.

**Takeaway:** WinBlaze reaches a rendered, interactive view of C:\ in ~4 s —
faster than WinDirStat's ~10.5 s walk, and competitive with WizTree on the
raw MFT read while getting to interactive sooner because it pages/caps the UI
(8,192-entry catalog, paged tree, deferred snapshot) instead of materializing
every file. These are single-machine, warm-cache figures; broaden across
machines and cold-cache for release-grade competitor evidence.

## Stability soak — live C:\ (2026-07-07)

`soak_repro` (winblaze-native example) loops full scans in one process, with an
incremental rescan + snapshot read every 4th cycle, printing the working set
and handle count per cycle so a leak shows as an upward trend. 12 cycles on
live C:\ (~2.05M files):

- Handles: flat at **69** for all 12 cycles — no handle leak.
- Working set: ~594 MB (cycles 1-3 warmup) -> plateau ~640-650 MB from cycle 5
  on; first-third vs last-third mean +9.4% (all in the warmup ramp, then flat).
- Verdict: **stable** (no monotonic working-set climb, no handle growth).

This is a representative stand-in; the multi-hour release soak remains a gate.
Run: `soak_repro C:\ 12` (or pass more cycles).

Finding: an **incremental rescan of a full drive (~15 s) is ~3.5x slower than a
full MFT scan (~4.4 s)** now that the MFT path is so fast — the 2M x 2M
path-materialize + diff + merge dominates. For full-volume roots a plain
re-scan is cheaper than incremental; incremental still wins for small subtrees
and for its change reporting. Candidate future work: id/mtime-keyed fast path
in the diff, or skip incremental for volume roots.
