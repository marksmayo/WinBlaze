# Engine scan performance: the read-throughput ceiling

**TL;DR — the raw-MFT engine scan of a full `C:\` is bounded by the volume's
raw-read throughput (~2 GB/s on the reference machine), not by CPU or code.
The practical warm-scan floor is ~2.2 s; a reliable sub-2 s is not achievable
in software on this hardware. This was profiled exhaustively — please don't
re-investigate without new (faster) storage or an algorithmic change to the
single-threaded path-resolution emit.**

## What the scan does

`winblaze-scanner`'s fast path reads the NTFS `$MFT` directly off the raw
volume (`\\.\C:`), fixes up each 1024-byte FILE record, parses it, and emits
directory/file events while resolving the directory tree. On the reference
`C:\`: ~4.7M MFT records, ~2.6M in use (~2.1M files + ~0.44M dirs), a ~4.8 GB
`$MFT`.

## Where the time goes (profiled)

Phase decomposition (`examples/mft_phase_bench.rs`, `profile_mft_phases`) and an
env-gated in-pipeline split (`WINBLAZE_PROFILE_STREAM`) show the scan is
**read-bound**: with a null event sink the full pipeline (`full_ms`) is within
noise of the raw read alone (`stream_read`), i.e. parse + fixup + emit hide
almost entirely behind the read.

| Stage | Cost (warm, full `C:\`) |
|---|---|
| Raw `$MFT` read | ~2.0–2.4 s (the wall) |
| Fixup + parse (fanned across workers) | hidden behind read |
| Emit / path resolution (single-threaded) | ~1.3–1.5 s, hidden behind read |

## The read ceiling (measured three ways)

Raw-volume reads top out at **~2 GB/s** and do **not** scale with more readers:

- Buffered reads, 1→32 reader threads: best ~2.0 GB/s at 2 threads, **degrades**
  beyond that.
- Full parallel reader/parser pipeline: no reliable gain.
- **`FILE_FLAG_NO_BUFFERING`** (DMA straight from the device, bypassing the
  system cache), 1→8 threads: **identical ~2 GB/s.** This proves the ceiling is
  the device/volume stack, not a cache-copy artifact.

The scan must read every in-use record — ~2.6 GB minimum. At 2 GB/s that alone
is ~1.3 s; reading *only* the interspersed in-use records means maximally
fragmented I/O (far below 2 GB/s), while coalescing to keep reads fast means
reading ~3 GB. That tension bottoms out at ~2.2 s.

## Optimizations that shipped (~15–17% faster)

Measured via interleaved A/B on the same volume/session (sparse beat a forced
full read in every pair); file/dir counts identical, unit-tested, fuzz-intact:

1. **Sparse MFT read** — read the `$MFT` `$BITMAP` (following the base record's
   `$ATTRIBUTE_LIST` to the extension record that holds it on a large,
   fragmented MFT) and skip free-record runs `>= 256 KiB`. Reads ~66% of the
   MFT. Cluster-aligned so a raw read never splits a record; falls back to a
   full read on any bitmap-read failure or odd cluster size. Disable with
   `WINBLAZE_NO_SPARSE_MFT`; tune the run threshold with `WINBLAZE_SKIP_KB`.
2. **3-stage pipeline** — reader thread → parser thread (fixup+parse fan-out) →
   main-thread emit, so the emit overlaps parse and read.
3. **Direct-read reader** — cluster-aligned sparse extents read straight into
   the caller buffer instead of through the aligned bounce chunk (drops a
   multi-GB double-memcpy).
4. **256 KiB skip threshold** — swept on a live MFT; skips the large free runs
   (most of the reclaimable bytes) while keeping kept extents few and large
   enough to sustain read throughput.
5. **Cheaper ingest** — skip the per-entry `pending_extensions` probe when empty.

Net warm full-`C:\`: median ~2.9–3.2 s → **~2.4–2.5 s (best ~2.2 s)**. The
sparse read helps **cold-cache first scans** (the common real case) even more,
since it reads a third less data.

## Approaches tried and rejected (no reliable gain)

- **Parallel read+parse workers** — noise-equivalent to single-stream; the
  device doesn't scale on fragmented reads and the emit is the next floor.
- **`NO_BUFFERING` I/O** — identical 2 GB/s (see above).
- **Dedicated parallel reader threads** — read-only throughput does improve to
  ~1.76 s, but the end-to-end wall stays ~2.3 s because the single-threaded,
  inherently sequential path-resolution emit (~1.3–1.5 s) plus pipeline
  coordination fills the non-read time once the read drops below it.

## Why sub-2 s is not reachable here

Two hard limits bracket the wall:

- **Read:** ~2 GB/s device ceiling; ~2.6–3 GB to read.
- **Emit:** single-threaded, stateful directory-tree resolution with orphan
  handling (~1.3–1.5 s) that cannot be parallelized without a different
  algorithm.

Overlapped, they floor the wall at **~2.2 s**. The last ~200 ms to 2.0 s sits
inside run-to-run noise and is not reliably closable in software. Only faster
storage — or a redesigned, parallelizable emit — would move it.

## If you want to push further

- **Faster storage** is the direct answer (the ceiling is I/O).
- A **parallelizable emit** (e.g. emit files lock-free per partition and resolve
  the directory tree in a separate pass) could let parallel reads pay off — a
  substantial redesign with uncertain, load-sensitive payoff.
- Dev knobs: `WINBLAZE_PROFILE_STREAM` (stage split), `WINBLAZE_SKIP_KB`
  (skip threshold), `WINBLAZE_NO_SPARSE_MFT` (force full read), and
  `examples/mft_phase_bench.rs` (phase decomposition).
