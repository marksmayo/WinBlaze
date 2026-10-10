# Quick-folder timing variability

Target: the existing startup + complete scan/index/persistence + CSV workflow over72files in13directories, n=10 per product. Keep R-7 IQR/median×100 unchanged. Never pad runtime, omit slow observations or change output correctness.

These100 candidates are hypotheses. Fixed startup/finalization ranks first for this small workload. Existing improvements are identified in the JSON audit and do not count as new wins; phase evidence will reorder or reject candidates. Savings are unquantified and not additive.

| Rank | Phase | Candidate |
|---:|---|---|
| 1 | Process startup | Borrow command arguments instead of cloning |
| 2 | Process startup | Avoid formatting usage text on valid arguments |
| 3 | Process startup | Reduce native CLI import initialization |
| 4 | Process startup | Remove unused CLI linker dependencies |
| 5 | Process startup | Keep startup logging lazy until first event |
| 6 | Process startup | Avoid redundant cache-root normalization |
| 7 | Process startup | Resolve executable-independent config once |
| 8 | Process startup | Avoid loading previous snapshots for replacement scans |
| 9 | Process startup | Avoid CLI initialization of GUI preview state |
| 10 | Process startup | Reduce thread startup dependencies before first scan |
| 11 | Finalization and output | Wait on terminal channel instead of polling |
| 12 | Finalization and output | Join scan workers once |
| 13 | Finalization and output | Avoid duplicate terminal state publication |
| 14 | Finalization and output | Release completed pipelines before export |
| 15 | Finalization and output | Avoid redundant final progress logging |
| 16 | Finalization and output | Reuse JSON event formatting storage |
| 17 | Finalization and output | Keep event log handle open through scan |
| 18 | Finalization and output | Avoid flushing empty log buffers |
| 19 | Finalization and output | Move auxiliary snapshot state instead of cloning |
| 20 | Finalization and output | Drop temporary export owners before process exit |
| 21 | Worker scheduling | Worker-local directory counters |
| 22 | Worker scheduling | Worker-local byte counters |
| 23 | Worker scheduling | Publish counters once per directory |
| 24 | Worker scheduling | Notify only workers that can consume new tasks |
| 25 | Worker scheduling | Release queue lock before task conversion |
| 26 | Worker scheduling | Move queued task ownership instead of cloning |
| 27 | Worker scheduling | Reserve child-task storage from observed fanout |
| 28 | Worker scheduling | Reuse empty worker task vectors |
| 29 | Worker scheduling | Avoid redundant completion wakeups |
| 30 | Worker scheduling | Bound worker creation for known tiny roots |
| 31 | Event delivery | Reuse returned event batch buffers |
| 32 | Event delivery | Reserve batch capacity once per worker |
| 33 | Event delivery | Avoid allocating an empty final batch |
| 34 | Event delivery | Coalesce redundant progress snapshots |
| 35 | Event delivery | Send progress only after meaningful counter change |
| 36 | Event delivery | Avoid receiver wakeups for empty batches |
| 37 | Event delivery | Reduce extra event enum dispatch passes |
| 38 | Event delivery | Separate terminal delivery from live previews |
| 39 | Event delivery | Move record strings through every batch boundary |
| 40 | Event delivery | Bound pending batch memory without dropping events |
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
| 51 | CSV serialization | Reuse decimal formatting scratch |
| 52 | CSV serialization | Write ordinary unescaped fields in one call |
| 53 | CSV serialization | Borrow parent paths during file export |
| 54 | CSV serialization | Avoid allocating a complete joined file path |
| 55 | CSV serialization | Batch adjacent constant CSV delimiters |
| 56 | CSV serialization | Reserve escaping scratch from field size |
| 57 | CSV serialization | Avoid repeated directory lookup per file |
| 58 | CSV serialization | Write directory totals without intermediate Strings |
| 59 | CSV serialization | Reduce flush calls while retaining successful close |
| 60 | CSV serialization | Size output buffering for the scale fixture |
| 61 | Path and string handling | Reserve exact child directory path capacity |
| 62 | Path and string handling | Consume owned OsString in display conversion |
| 63 | Path and string handling | Avoid formatting already UTF8 directory paths |
| 64 | Path and string handling | Reuse root prefix during path assembly |
| 65 | Path and string handling | Borrow ancestors during cycle checks |
| 66 | Path and string handling | Avoid duplicate root normalization |
| 67 | Path and string handling | Reuse filesystem error path conversion |
| 68 | Path and string handling | Avoid duplicate extension extraction |
| 69 | Path and string handling | Avoid storing derived full file paths |
| 70 | Path and string handling | Bound temporary path capacity retained by workers |
| 71 | Directory metadata | Reuse metadata scratch across directories |
| 72 | Directory metadata | Skip dot names before conversion |
| 73 | Directory metadata | Avoid repeat metadata-class negotiation |
| 74 | Directory metadata | Encode ordinary names once |
| 75 | Directory metadata | Avoid copying borrowed UTF16 record fields |
| 76 | Directory metadata | Reuse record-boundary validation results |
| 77 | Directory metadata | Avoid rereading buffer header fields |
| 78 | Directory metadata | Reserve filename capacity from exact encoding length |
| 79 | Directory metadata | Reduce zeroing of overwritten scratch bytes |
| 80 | Directory metadata | Keep error fallback off successful metadata path |
| 81 | Catalog construction | Arithmetic lookup for contiguous directory IDs |
| 82 | Catalog construction | In-place ordering for dense file IDs |
| 83 | Catalog construction | Bypass ordering for already sorted IDs |
| 84 | Catalog construction | Transfer record arenas without a second array |
| 85 | Catalog construction | Trim unused publication capacity |
| 86 | Catalog construction | Reserve exact child-index counts |
| 87 | Catalog construction | Reuse parent positions during rollup |
| 88 | Catalog construction | Skip sorting child lists with fewer than two items |
| 89 | Catalog construction | Avoid repeated child-size key extraction |
| 90 | Catalog construction | Release transient parent arrays immediately after grouping |
| 91 | Zero-byte metadata work | Skip physical identity tracking for zero allocation |
| 92 | Zero-byte metadata work | Avoid compressed-size query when metadata proves zero allocation |
| 93 | Zero-byte metadata work | Avoid allocation rounding for zero logical size |
| 94 | Zero-byte metadata work | Avoid hardlink dedup lock for zero allocation |
| 95 | Zero-byte metadata work | Avoid repeated volume identity for ordinary entries |
| 96 | Zero-byte metadata work | Use already-open handles for fallback metadata |
| 97 | Zero-byte metadata work | Avoid reopening entries with sufficient batched metadata |
| 98 | Zero-byte metadata work | Keep sparse-file fallback separate from ordinary files |
| 99 | Zero-byte metadata work | Cache volume cluster size per safe identity |
| 100 | Zero-byte metadata work | Avoid redundant nonempty per-file progress byte bookkeeping |

Current verified changes: small snapshot and CSV write buffers scale with catalog size (roughly 22 KiB for Quick instead of 1 MiB each). Both have public allocation-budget RED/GREEN tests with output round trips; large exports retain their established disk-write budget. Full Rust checks passed with 92.40% line coverage. Timing savings remain unproven. The rotated n=10 worker experiment did not justify changing the four-worker default.
