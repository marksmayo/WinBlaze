# WinBlaze performance improvement audit

100-point candidate audit. Ordering after the nine qualified changes is a profiling backlog, not an evidence-backed ranking.

Nine production improvements were qualified and implemented. Other entries distinguish existing optimizations, a measurement fix, missing features, and candidates needing profiling. No numeric whole-drive savings are invented for unqualified entries.

## Baseline gaps

- First-idle startup proxy: 113.355 ms vs WinDirStat 102.925 ms
- Full C sampled peak working set: 914.629 MiB vs WinDirStat 769.162 MiB
- Full C sampled CPU: 150.3125 CPU-s vs WinDirStat 117.539065 CPU-s
- First-idle startup CPU: WinBlaze 0.141 CPU-s vs WinDirStat 0.062 CPU-s
- First-idle working set: WinBlaze 37.916 MiB vs WinDirStat 19.473 MiB
- First-idle peak working set: WinBlaze 37.916 MiB vs WinDirStat 19.473 MiB
- Full C sampled peak private bytes: WinBlaze 1150.654 MiB vs WinDirStat 725.668 MiB

## Candidate audit

| # | Candidate | Status | Estimated saving / qualification |
|---|---|---|---|
| 1 | Reorder tree records in place | implemented | Eliminate one full FileRecord/DirectoryRecord vector; local 20k fixture reduces cumulative allocations by at least 2,000,716 bytes. |
| 2 | Clone only returned search hits | implemented | Local 20k/limit10 fixture saves at least 2,301,388 allocated bytes; end-to-end CPU savings unmeasured. |
| 3 | Stream CSV escaping and paths | implemented | Eliminate at least 1,013,700 allocated bytes per local 20k export; reduces per-row allocation to zero. |
| 4 | Return immediately for search limit zero | implemented | Eliminate 1,228,898 allocated bytes in the local 20k fixture; skip the entire scan/sort. |
| 5 | Shrink retained largest-file order | implemented | Retained cache falls from 400,000 to under 70,000 bytes for 100k files; once-per-cache shrink may cost time. |
| 6 | Select and sort only requested textual search prefix | implemented | Local 20k fixture saves at least 65,314 allocated bytes; O(N + K log K) selection replaces full sorting for limited text queries. |
| 7 | Skip search when no record types are selected | implemented | Eliminate 103 allocated bytes plus catalog traversal in the local fixture. |
| 8 | Write short startup log lines in one operation | implemented | 10 short lines require 10 rather than 20 WriteFile calls (50% fewer); whole-app startup saving unmeasured. |
| 9 | Stabilize directory IDs before path-matched incremental diffs | implemented | Avoid a false move record and lineage for each unchanged file whose parent ID changes; the observed tiny rescan generated 48 false moves. Timing saving unmeasured. |
| 10 | Defer updater network check | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 11 | Defer drive enumeration | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 12 | Defer icon creation | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 13 | Cache font resources | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 14 | Defer diagnostics view construction | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 15 | Defer treemap GPU initialization | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 16 | Avoid hidden-window treemap layout | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 17 | Defer full cache decoding until requested | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 18 | Reuse warm shader bytecode | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 19 | Tune directory worker count | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 20 | Tune work-stealing queue capacity | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 21 | Batch directory scheduling | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 22 | Reduce per-file event frequency | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 23 | Reuse directory enumeration buffers | existing-optimization | No additional runtime saving credited. |
| 24 | Reduce allocation metadata calls | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 25 | Avoid path normalization per file | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 26 | Reduce extension parsing per file | existing-optimization | No additional runtime saving credited. |
| 27 | Reserve transaction maps from counts | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 28 | Reduce root/session record copies | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 29 | Tune MFT read batch size | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 30 | Reuse MFT fixup scratch storage | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 31 | Reduce record attribute temporary copies | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 32 | Avoid duplicate filename decoding | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 33 | Reduce non-resident run-list allocations | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 34 | Tune MFT parser worker count | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 35 | Reduce worker merge contention | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 36 | Reserve MFT result capacity from header count | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 37 | Reduce orphan resolution passes | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 38 | Reduce fallback directory walks | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 39 | Avoid transaction clone during persistence | existing-optimization | No additional runtime saving credited; already present or covered by an implemented change. |
| 40 | Reduce simultaneous catalog generations | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 41 | Materialize file paths only for returned queries | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 42 | Reduce duplicated directory aggregates | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 43 | Compact filename storage | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 44 | Reduce directory lookup map capacity | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 45 | Release ingest scratch before rendering | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 46 | Share immutable catalog across consumers | existing-optimization | No additional runtime saving credited. |
| 47 | Tune index lookup density | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 48 | Reduce rollup scratch capacity | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 49 | Avoid sorting snapshot rows twice | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 50 | Reuse serialization scratch across sections | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 51 | Tune snapshot writer buffer size | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 52 | Avoid duplicate persisted full paths | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 53 | Tune buffered snapshot read size | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 54 | Decode file names from borrowed input | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 55 | Reduce cache decode map reallocations | existing-optimization | No additional runtime saving credited; already present or covered by an implemented change. |
| 56 | Reduce cache-to-tree record copies | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 57 | Stream snapshot checksums | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 58 | Reduce snapshot flush frequency | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 59 | Partial-select numeric top-K results | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 60 | Reuse query scratch buffers | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 61 | Cache normalized extensions | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 62 | Index common suffix filters | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 63 | Bound directory aggregate copies | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 64 | Cache query results by generation | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 65 | Short-circuit empty patterns | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 66 | Avoid sorting already ordered results | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 67 | Share result paths across rows | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 68 | Reduce directory-only search materialization | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 69 | Tune CSV writer buffer size | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 70 | Reuse numeric formatting scratch | existing-optimization | No additional runtime saving credited; already present or covered by an implemented change. |
| 71 | Batch CSV rows into chunks | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 72 | Avoid scanning when cached export is requested | existing-optimization | No additional runtime saving credited; already present or covered by an implemented change. |
| 73 | Reduce extension summary merge allocations | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 74 | Cache extension summaries per catalog | existing-optimization | No additional runtime saving credited. |
| 75 | Parallelize duplicate hashing | feature-addition-required | Not applicable to an existing WinBlaze feature; no performance saving credited. |
| 76 | Avoid rehashing unchanged duplicate candidates | feature-addition-required | Not applicable to an existing WinBlaze feature; no performance saving credited. |
| 77 | Stream raw MFT dumps | feature-addition-required | Not applicable to an existing WinBlaze feature; no performance saving credited. |
| 78 | Reduce feature fixture process startup cost | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 79 | Reduce list item realization | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 80 | Reuse immutable tree row models | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 81 | Avoid rebuilding unchanged tree branches | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 82 | Coalesce progress updates | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 83 | Reduce UI thread event parsing | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 84 | Avoid formatting hidden diagnostics | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 85 | Reuse text brushes and style resources | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 86 | Avoid repeated largest-file sorts | existing-optimization | No additional runtime saving credited; already present or covered by an implemented change. |
| 87 | Reduce treemap label measurements | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 88 | Coalesce redundant treemap renders | existing-optimization | No additional runtime saving credited. |
| 89 | Skip unchanged directory rollups | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 90 | Reuse unchanged directory paths | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 91 | Bound historical change-set retention | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 92 | Avoid full cache rewrite for tiny changes | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 93 | Separate engine time from CSV export | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 94 | Increase sampling fidelity for short processes | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 95 | Separate cold from warmed measurements | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 96 | Measure visible usable window startup | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 97 | Attribute backend DLL changes in GUI baselines | measurement-fix | No additional runtime saving credited. |
| 98 | Tune allocator size classes | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 99 | Tune compiler link-time optimization | not-qualified | No defensible numeric estimate yet; no saving credited. |
| 100 | Remove redundant native result serialization | not-qualified | No defensible numeric estimate yet; no saving credited. |
