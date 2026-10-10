# Empty-file scale variability optimization

The target is the existing whole-process IQR / median medal, with n=10 per product. The current scores are WinBlaze18.61%, WinDirStat3.44%, WizTree13.20%. No runtime padding, cherry-picking or measurement changes.

These100 ranked candidates are hypotheses, not100 proven improvements. Ranking groups prioritizes repeated work and scheduling before fixed startup work; profiling may reorder them. Expected savings are bounded by each phase’s measured contribution, currently unknown. Existing optimizations must be marked as already present, not counted again.

| Rank | Phase | Candidate |
|---:|---|---|
| 1 | Worker scheduling | Worker-local directory counters |
| 2 | Worker scheduling | Worker-local byte counters |
| 3 | Worker scheduling | Publish counters once per directory |
| 4 | Worker scheduling | Notify only workers that can consume new tasks |
| 5 | Worker scheduling | Release queue lock before task conversion |
| 6 | Worker scheduling | Move queued task ownership instead of cloning |
| 7 | Worker scheduling | Reserve child-task storage from observed fanout |
| 8 | Worker scheduling | Reuse empty worker task vectors |
| 9 | Worker scheduling | Avoid redundant completion wakeups |
| 10 | Worker scheduling | Bound worker creation for known tiny roots |
| 11 | Event delivery | Reuse returned event batch buffers |
| 12 | Event delivery | Reserve batch capacity once per worker |
| 13 | Event delivery | Avoid allocating an empty final batch |
| 14 | Event delivery | Coalesce redundant progress snapshots |
| 15 | Event delivery | Send progress only after meaningful counter change |
| 16 | Event delivery | Avoid receiver wakeups for empty batches |
| 17 | Event delivery | Reduce extra event enum dispatch passes |
| 18 | Event delivery | Separate terminal delivery from live previews |
| 19 | Event delivery | Move record strings through every batch boundary |
| 20 | Event delivery | Bound pending batch memory without dropping events |
| 21 | Directory metadata | Reuse metadata scratch across directories |
| 22 | Directory metadata | Skip dot names before conversion |
| 23 | Directory metadata | Avoid repeat metadata-class negotiation |
| 24 | Directory metadata | Encode ordinary names once |
| 25 | Directory metadata | Avoid copying borrowed UTF16 record fields |
| 26 | Directory metadata | Reuse record-boundary validation results |
| 27 | Directory metadata | Avoid rereading buffer header fields |
| 28 | Directory metadata | Reserve filename capacity from exact encoding length |
| 29 | Directory metadata | Reduce zeroing of overwritten scratch bytes |
| 30 | Directory metadata | Keep error fallback off successful metadata path |
| 31 | Catalog construction | Arithmetic lookup for contiguous directory IDs |
| 32 | Catalog construction | In-place ordering for dense file IDs |
| 33 | Catalog construction | Bypass ordering for already sorted IDs |
| 34 | Catalog construction | Transfer record arenas without a second array |
| 35 | Catalog construction | Trim unused publication capacity |
| 36 | Catalog construction | Reserve exact child-index counts |
| 37 | Catalog construction | Reuse parent positions during rollup |
| 38 | Catalog construction | Skip sorting child lists with fewer than two items |
| 39 | Catalog construction | Avoid repeated child-size key extraction |
| 40 | Catalog construction | Release transient parent arrays immediately after grouping |
| 41 | CSV serialization | Reuse decimal formatting scratch |
| 42 | CSV serialization | Write ordinary unescaped fields in one call |
| 43 | CSV serialization | Borrow parent paths during file export |
| 44 | CSV serialization | Avoid allocating a complete joined file path |
| 45 | CSV serialization | Batch adjacent constant CSV delimiters |
| 46 | CSV serialization | Reserve escaping scratch from field size |
| 47 | CSV serialization | Avoid repeated directory lookup per file |
| 48 | CSV serialization | Write directory totals without intermediate Strings |
| 49 | CSV serialization | Reduce flush calls while retaining successful close |
| 50 | CSV serialization | Size output buffering for the scale fixture |
| 51 | Snapshot persistence | Borrow sorted record slices for persistence |
| 52 | Snapshot persistence | Reuse directory encoding scratch |
| 53 | Snapshot persistence | Reuse file encoding scratch |
| 54 | Snapshot persistence | Batch adjacent scalar encodings |
| 55 | Snapshot persistence | Avoid sorting already sorted session records |
| 56 | Snapshot persistence | Drop publication lookups before disk persistence |
| 57 | Snapshot persistence | Avoid duplicate snapshot writes at completion |
| 58 | Snapshot persistence | Avoid repeated backup existence queries |
| 59 | Snapshot persistence | Release temporary serializer indexes before rename |
| 60 | Snapshot persistence | Avoid allocating intermediate complete snapshot bytes |
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
