# Scan and CSV performance review

Full standard-user startup + scan + complete durable CSV export; 100 candidate review, seven implemented changes. Remaining ordering is profiling priority, not a measured savings ranking.

Do not add overlapping savings. Current n=10 results determine total improvement; cold cache is uncontrolled. Candidates without evidence are not called safe wins. The existing earlier 100-point audit is separate historical work.

## Implemented changes

### 1. Batch allocation, timestamps and file identities per directory

Replace roughly 1.65 million file opens/metadata queries with directory-sized batches; exploratory n=10 engine median 16.8 s versus prior 60.8 s. This includes cache differences, not an isolated causal estimate.
Evidence: `directory_metadata.rs; allocation.rs; existing scanner fanout/cancellation tests`. Source: `src/WinBlaze.Scanner/src/directory_metadata.rs`.

### 2. Decode names directly from the aligned directory buffer

Remove one temporary UTF-16 vector per name; 3000-name cumulative allocation 435009 -> 261003 bytes.
Evidence: `performance.rs::batched_names_use_one_owned_utf16_copy_per_entry`. Source: `src/WinBlaze.Scanner/src/directory_metadata.rs`.

### 3. Store volume identifiers once per identity set

100k identities: cumulative allocation 8650876 -> approximately 4456880 bytes before the next optimization.
Evidence: `performance.rs::physical_identity_budget_stores_volume_once_instead_of_per_file`. Source: `src/WinBlaze.Scanner/src/allocation.rs`.

### 4. Store common 64-bit identities compactly with full 128-bit fallback

100k identities: cumulative allocation falls from 4456880 to below 2600000 bytes; wide identities remain distinct.
Evidence: `performance.rs::common_64_bit_file_identities_fit_a_compact_budget_without_truncating_wide_ids`. Source: `src/WinBlaze.Scanner/src/allocation.rs`.

### 5. Shard parallel physical-storage accounting

Replace one scan-wide identity lock with 64 independent locks. CPU/time savings require final n=10 measurement; cannot be attributed separately from batching.
Evidence: `allocation_tracker.rs::simultaneous_hardlink_claims_allocate_storage_exactly_once`. Source: `src/WinBlaze.Scanner/src/allocation.rs`.

### 6. Skip identity storage and locking for zero allocation

Avoid a hash insertion/lock for every zero-allocation record. Count and time benefit depend on the volume; no logical file is skipped.
Evidence: `allocation_tracker.rs::physical_storage_claims_preserve_volume_and_full_128_bit_identity`. Source: `src/WinBlaze.Scanner/src/allocation.rs`.

### 7. Batch atomic export writes in a 1 MiB buffer

32 MiB output: 4096 -> 32 OS disk write operations; +approximately 1 MiB transient memory per active export. Durability and atomic replacement retained.
Evidence: `output_performance.rs::large_atomic_exports_batch_disk_writes_and_preserve_the_complete_output`. Source: `src/WinBlaze.Native/src/output.rs`.

## Remaining candidates

These candidates have no established independent savings. They are not classified as zero/low risk or counted as implemented improvements. Their component limits and required validation are recorded in the JSON audit.

| Priority | Candidate | Area | Status |
|---:|---|---|---|
| 8 | Reuse the fallback UTF-16 directory prefix | Directory fallback | not qualified |
| 9 | Reuse fallback file-path capacity across entries | Directory fallback | not qualified |
| 10 | Query compression metadata using the already-open file handle | Directory fallback | not qualified |
| 11 | Combine fallback link count and short identity queries | Directory fallback | not qualified |
| 12 | Avoid joining paths for fallback directory entries | Directory fallback | not qualified |
| 13 | Use a smaller first metadata buffer for tiny directories | Directory fallback | not qualified |
| 14 | Grow directory buffers only when the filesystem requests it | Directory fallback | not qualified |
| 15 | Pool directory metadata buffers per worker | Directory fallback | not qualified |
| 16 | Avoid clearing metadata buffers between directory opens | Directory fallback | not qualified |
| 17 | Select supported metadata classes once per volume | Directory fallback | not qualified |
| 18 | Reserve child-directory path capacity from exact name length | Path handling | not qualified |
| 19 | Borrow ancestor paths for ordinary child checks | Path handling | not qualified |
| 20 | Avoid duplicate normalized-root conversions | Path handling | not qualified |
| 21 | Avoid duplicate volume-root discovery | Path handling | not qualified |
| 22 | Cache volume information for reparse targets | Path handling | not qualified |
| 23 | Reduce fallback error-path display conversions | Path handling | not qualified |
| 24 | Reuse canonicalization scratch space for followed reparses | Path handling | not qualified |
| 25 | Avoid converting display names twice in logs | Path handling | not qualified |
| 26 | Borrow exact file names during issue classification | Path handling | not qualified |
| 27 | Avoid constructing paths for skipped reparse files | Path handling | not qualified |
| 28 | Pre-reserve identity sets using a bounded directory estimate | Identity accounting | not qualified |
| 29 | Batch identity claims while holding one shard lock | Identity accounting | not qualified |
| 30 | Select shard count from measured worker count | Identity accounting | not qualified |
| 31 | Use worker-local identity prefilters without false positives | Identity accounting | not qualified |
| 32 | Avoid repeated volume map lookup within one directory batch | Identity accounting | not qualified |
| 33 | Release identity sets immediately after producer completion | Identity accounting | not qualified |
| 34 | Shrink sparse identity shards before long idle periods | Identity accounting | not qualified |
| 35 | Use a measured identity-specific hasher while retaining collision correctness | Identity accounting | not qualified |
| 36 | Choose directory worker count from a controlled n=10 sweep | Scheduling | not qualified |
| 37 | Avoid waking all workers for one newly queued directory | Scheduling | not qualified |
| 38 | Queue child directories in batches | Scheduling | not qualified |
| 39 | Amortize queue locking across local child discoveries | Scheduling | not qualified |
| 40 | Use a bounded worker-local directory queue | Scheduling | not qualified |
| 41 | Reduce empty work-queue polling | Scheduling | not qualified |
| 42 | Move non-shared directory counters to workers | Scheduling | not qualified |
| 43 | Merge worker byte totals at progress boundaries | Scheduling | not qualified |
| 44 | Avoid work-stealing for shallow single-directory scans | Scheduling | not qualified |
| 45 | Reuse worker pipeline buffers across directories | Scheduling | not qualified |
| 46 | Tune event batch size with a controlled sweep | Events | not qualified |
| 47 | Reuse emptied event batches via a bounded pool | Events | not qualified |
| 48 | Use bounded channel capacity derived from byte budgets | Events | not qualified |
| 49 | Coalesce superseded non-terminal progress updates | Events | not qualified |
| 50 | Reserve issue batches from observed issue density | Events | not qualified |
| 51 | Avoid calculating progress intervals until item thresholds pass | Events | not qualified |
| 52 | Batch file IDs and shared counter increments per directory | Events | not qualified |
| 53 | Avoid copying progress state for unchanged values | Events | not qualified |
| 54 | Separate summary delivery from unused per-file CLI callbacks | Events | not qualified |
| 55 | Reuse log formatting scratch for bounded diagnostic events | Events | not qualified |
| 56 | Skip record permutation when IDs are already sorted | Index construction | not qualified |
| 57 | Reserve transaction maps from bounded walk estimates | Index construction | not qualified |
| 58 | Avoid allocating child-count arrays for empty catalogs | Index construction | not qualified |
| 59 | Reuse child-count storage as child-fill cursors | Index construction | not qualified |
| 60 | Pre-reserve root traversal order from directory count | Index construction | not qualified |
| 61 | Skip child sorting for zero/one-child ranges | Index construction | not qualified |
| 62 | Cache physical size comparison keys for large child sorts | Index construction | not qualified |
| 63 | Fuse file rollups and extension aggregation when measured useful | Index construction | not qualified |
| 64 | Reuse directory lookup results during file attachment | Index construction | not qualified |
| 65 | Avoid building display-only ordering for unattended export | Index construction | not qualified |
| 66 | Batch adjacent snapshot scalar writes | Persistence | not qualified |
| 67 | Reuse string-length encoding scratch | Persistence | not qualified |
| 68 | Avoid sorting already-sorted auxiliary records | Persistence | not qualified |
| 69 | Reserve snapshot serializer vectors from exact collection sizes | Persistence | not qualified |
| 70 | Borrow auxiliary state when lifetimes permit | Persistence | not qualified |
| 71 | Avoid opening an empty repository before a replacement scan | Persistence | not qualified |
| 72 | Release repository state before CSV output begins | Persistence | not qualified |
| 73 | Reduce redundant backup metadata queries | Persistence | not qualified |
| 74 | Size snapshot read buffers from validated header metadata | Persistence | not qualified |
| 75 | Avoid duplicate checksum passes if a compatible streaming checksum exists | Persistence | not qualified |
| 76 | Benchmark a stack decimal encoder against standard formatting | CSV export | not qualified |
| 77 | Batch one CSV row into reusable scratch space | CSV export | not qualified |
| 78 | Scan quote-free names with a measured fast byte search | CSV export | not qualified |
| 79 | Write constant CSV suffixes in fewer calls | CSV export | not qualified |
| 80 | Borrow parent-path separators consistently | CSV export | not qualified |
| 81 | Reuse parent path lookup across adjacent sibling records | CSV export | not qualified |
| 82 | Buffer large escaped path fragments in fewer writes | CSV export | not qualified |
| 83 | Avoid dynamic filter dispatch for unfiltered CSV | CSV export | not qualified |
| 84 | Avoid evaluating directory selection predicates for full exports | CSV export | not qualified |
| 85 | Estimate output size for optional file preallocation | CSV export | not qualified |
| 86 | Wait on terminal notification instead of 5 ms polling | CLI startup and completion | not qualified |
| 87 | Parse CLI arguments without collecting an intermediate vector | CLI startup and completion | not qualified |
| 88 | Avoid allocating live UI forwarding state for a quiet CLI | CLI startup and completion | not qualified |
| 89 | Suppress transient extension snapshots in unattended mode | CLI startup and completion | not qualified |
| 90 | Avoid temporary C strings for unused live directory callbacks | CLI startup and completion | not qualified |
| 91 | Avoid capped live file path materialization in unattended mode | CLI startup and completion | not qualified |
| 92 | Avoid repeated current-time reads on a running scan | CLI startup and completion | not qualified |
| 93 | Expose phase telemetry without enabling per-file logging | CLI startup and completion | not qualified |
| 94 | Validate synthetic CSV totals against independent fixture manifests | Measurement integrity | not qualified |
| 95 | Interleave product order across ten paired rounds | Measurement integrity | not qualified |
| 96 | Track process I/O operation counts separately from time | Measurement integrity | not qualified |
| 97 | Record scan/index/persist/export phase durations | Measurement integrity | not qualified |
| 98 | Reject stale example binaries before benchmarking | Measurement integrity | not qualified |
| 99 | Use bounded report publication frequency outside timed work | Measurement integrity | not qualified |
| 100 | Detect background load and annotate affected rounds | Measurement integrity | not qualified |

## Final n=10 observation

Full WinBlaze workflow: 67.56 s → 17.61 s; sampled CPU: 411.87 → 114.01 CPU-s; peak working set: 650.63 → 647.15 MiB. Fresh competitor medians: WinDirStat 26.90 s, WizTree 38.10 s. Dated live-volume coverage and caches differ. See `current-scan-export-validation.json` for exact values and identity checks.
