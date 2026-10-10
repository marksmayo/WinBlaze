# Nonempty dense-folder timing variability

Use the existing 8192-file / 3-directory fixture and startup + complete scan/index/persistence + CSV workflow, n=10 per product. Retain all runs and the original R-7 IQR/median×100 metric.

The baseline already earned gold (10.39%, versus 21.01% and 23.11%). A public resource test qualified one reduction: unused worker event buffers allocate zero rather than 69632 bytes each. First-batch ordering and bounded growth pass. The existing terminal-only allocation guard failed during development and passed after limiting an initial terminal event to one slot. Workspace/examples/fmt/strict Clippy/92.40% line coverage and Release build pass. Baseline gold predates this change; do not claim the change caused gold.

Allocation validation independently checked every fixture file: Windows standard allocation is 256 bytes; all streams have no nonresident extents. WizTree exports 0 allocated bytes; WinBlaze/WinDirStat export 256. Every path and logical size must match; differing allocation semantics remain explicit in the report.

These 100 candidates are carried-forward hypotheses provisionally reordered for this workload. They are not 100 verified new wins. Most require phase evidence or are already implemented; zero-byte counters do not help these nonempty files. Avoid replacing randomized identity hashing with a dense-id hasher: sharding and external file identities need distribution/security checks. Savings are not additive.

| Rank | Phase | Candidate |
|---:|---|---|
| 1 | Event delivery | Reserve event-batch storage only when a worker emits its first event |
| 2 | Event delivery | Reuse returned event batch buffers |
| 3 | Event delivery | Avoid allocating an empty final batch |
| 4 | Event delivery | Coalesce redundant progress snapshots |
| 5 | Event delivery | Send progress only after meaningful counter change |
| 6 | Event delivery | Avoid receiver wakeups for empty batches |
| 7 | Event delivery | Reduce extra event enum dispatch passes |
| 8 | Event delivery | Separate terminal delivery from live previews |
| 9 | Event delivery | Move record strings through every batch boundary |
| 10 | Event delivery | Bound pending batch memory without dropping events |
| 11 | Directory metadata | Reuse metadata scratch across directories |
| 12 | Directory metadata | Skip dot names before conversion |
| 13 | Directory metadata | Avoid repeat metadata-class negotiation |
| 14 | Directory metadata | Encode ordinary names once |
| 15 | Directory metadata | Avoid copying borrowed UTF16 record fields |
| 16 | Directory metadata | Reuse record-boundary validation results |
| 17 | Directory metadata | Avoid rereading buffer header fields |
| 18 | Directory metadata | Reserve filename capacity from exact encoding length |
| 19 | Directory metadata | Reduce zeroing of overwritten scratch bytes |
| 20 | Directory metadata | Keep error fallback off successful metadata path |
| 21 | Catalog construction | Arithmetic lookup for contiguous directory IDs |
| 22 | Catalog construction | In-place ordering for dense file IDs |
| 23 | Catalog construction | Bypass ordering for already sorted IDs |
| 24 | Catalog construction | Transfer record arenas without a second array |
| 25 | Catalog construction | Trim unused publication capacity |
| 26 | Catalog construction | Reserve exact child-index counts |
| 27 | Catalog construction | Reuse parent positions during rollup |
| 28 | Catalog construction | Skip sorting child lists with fewer than two items |
| 29 | Catalog construction | Avoid repeated child-size key extraction |
| 30 | Catalog construction | Release transient parent arrays immediately after grouping |
| 31 | CSV serialization | Reuse decimal formatting scratch |
| 32 | CSV serialization | Write ordinary unescaped fields in one call |
| 33 | CSV serialization | Borrow parent paths during file export |
| 34 | CSV serialization | Avoid allocating a complete joined file path |
| 35 | CSV serialization | Batch adjacent constant CSV delimiters |
| 36 | CSV serialization | Reserve escaping scratch from field size |
| 37 | CSV serialization | Avoid repeated directory lookup per file |
| 38 | CSV serialization | Write directory totals without intermediate Strings |
| 39 | CSV serialization | Reduce flush calls while retaining successful close |
| 40 | CSV serialization | Size output buffering for the scale fixture |
| 41 | Snapshot persistence | Borrow sorted record slices for persistence |
| 42 | Snapshot persistence | Reuse directory encoding scratch |
| 43 | Snapshot persistence | Reuse file encoding scratch |
| 44 | Snapshot persistence | Batch adjacent scalar encodings |
| 45 | Snapshot persistence | Avoid sorting already sorted session records |
| 46 | Snapshot persistence | Drop publication lookups before disk persistence |
| 47 | Snapshot persistence | Avoid duplicate snapshot writes at completion |
| 48 | Snapshot persistence | Avoid repeated backup existence queries |
| 49 | Snapshot persistence | Release temporary serializer indexes before rename |
| 50 | Snapshot persistence | Avoid allocating intermediate complete snapshot bytes |
| 51 | Worker scheduling | Worker-local directory counters |
| 52 | Worker scheduling | Worker-local byte counters |
| 53 | Worker scheduling | Publish counters once per directory |
| 54 | Worker scheduling | Notify only workers that can consume new tasks |
| 55 | Worker scheduling | Release queue lock before task conversion |
| 56 | Worker scheduling | Move queued task ownership instead of cloning |
| 57 | Worker scheduling | Reserve child-task storage from observed fanout |
| 58 | Worker scheduling | Reuse empty worker task vectors |
| 59 | Worker scheduling | Avoid redundant completion wakeups |
| 60 | Worker scheduling | Bound worker creation for known tiny roots |
| 61 | Process startup | Borrow command arguments instead of cloning |
| 62 | Process startup | Avoid formatting usage text on valid arguments |
| 63 | Process startup | Reduce native CLI import initialization |
| 64 | Process startup | Remove unused CLI linker dependencies |
| 65 | Process startup | Keep startup logging lazy until first event |
| 66 | Process startup | Avoid redundant cache-root normalization |
| 67 | Process startup | Resolve executable-independent config once |
| 68 | Process startup | Avoid loading previous snapshots for replacement scans |
| 69 | Process startup | Avoid CLI initialization of GUI preview state |
| 70 | Process startup | Reduce thread startup dependencies before first scan |
| 71 | Path and string handling | Reserve exact child directory path capacity |
| 72 | Path and string handling | Consume owned OsString in display conversion |
| 73 | Path and string handling | Avoid formatting already UTF8 directory paths |
| 74 | Path and string handling | Reuse root prefix during path assembly |
| 75 | Path and string handling | Borrow ancestors during cycle checks |
| 76 | Path and string handling | Avoid duplicate root normalization |
| 77 | Path and string handling | Reuse filesystem error path conversion |
| 78 | Path and string handling | Avoid duplicate extension extraction |
| 79 | Path and string handling | Avoid storing derived full file paths |
| 80 | Path and string handling | Bound temporary path capacity retained by workers |
| 81 | Zero-byte metadata work | Skip physical identity tracking for zero allocation |
| 82 | Zero-byte metadata work | Avoid compressed-size query when metadata proves zero allocation |
| 83 | Zero-byte metadata work | Avoid allocation rounding for zero logical size |
| 84 | Zero-byte metadata work | Avoid hardlink dedup lock for zero allocation |
| 85 | Zero-byte metadata work | Avoid repeated volume identity for ordinary entries |
| 86 | Zero-byte metadata work | Use already-open handles for fallback metadata |
| 87 | Zero-byte metadata work | Avoid reopening entries with sufficient batched metadata |
| 88 | Zero-byte metadata work | Keep sparse-file fallback separate from ordinary files |
| 89 | Zero-byte metadata work | Cache volume cluster size per safe identity |
| 90 | Zero-byte metadata work | Avoid unnecessary zero-byte counter atomic updates |
| 91 | Finalization and output | Wait on terminal channel instead of polling |
| 92 | Finalization and output | Join scan workers once |
| 93 | Finalization and output | Avoid duplicate terminal state publication |
| 94 | Finalization and output | Release completed pipelines before export |
| 95 | Finalization and output | Avoid redundant final progress logging |
| 96 | Finalization and output | Reuse JSON event formatting storage |
| 97 | Finalization and output | Keep event log handle open through scan |
| 98 | Finalization and output | Avoid flushing empty log buffers |
| 99 | Finalization and output | Move auxiliary snapshot state instead of cloning |
| 100 | Finalization and output | Drop temporary export owners before process exit |

## Current-build confirmation

Two independent n=10 batches per product earned WinBlaze gold. The latest dense-folder result is 11.93% variability (132.16 ms median), compared with WinDirStat 13.46% (790.78 ms) and WizTree 32.97% (1407.18 ms). Every run is retained, including the 256.42 ms first WinBlaze run. All 90 CSV outputs across dense, Quick and Empty-file workloads were validated. Quick variability is 7.84% gold; Empty-file variability is 24.32% gold. These are focused results, not a refreshed claim for all 35 categories.
