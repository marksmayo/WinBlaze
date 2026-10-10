# Remaining variability work after startup gold

Iteration32 supersedes this investigation ordering with the memory/CPU ranking
in ITERATION_32_RESOURCE_PLAN.md, following explicit user steering. Variability
is still reported under its unchanged definition. The supported console policy
replaces the separate-launcher proposal below; no extra launcher is implemented.

The completed iteration28 report has all35 rows and32 gold /3 silver.
All administrator categories completed and the final report validators passed.
Startup working set/peak are14.56640625MiB and first idle37.14ms, allgold.
Startup CPU retains its prior31.25ms official value with a pending label because
one zero reading leaves9 qualifying fresh observations. No favorable rerun or
sample replacement was performed. Bounded event reuse is implemented and
validated; see ITERATION_28_RECYCLED_EVENT_BATCHES.md. Subsequent isolated
startup experiments reprioritize the next25 investigations below.
The remaining silvers use unchanged IQR/median timing-variability definitions:

| Workflow | Latest percent | Retained gold target percent |
|---|---:|---:|
| Quick folder | 10.166261 | 2.962843 |
| Empty-file scale | 8.014005 | 4.014511 |
| Nonempty dense folder | 9.806043 | 5.397866 |

Full-C: variability remains gold at3.706530% against5.230405%. All slower
WinBlaze observations remain included; one cohort does not prove causation.

These are current observations, not claims of causal improvement. The original
100 hypotheses remain in ALL_GOLD_LOOP_ITERATION_11_PLAN.md. This ordering
selects the next 25 investigations from those hypotheses after startup adoption:

1. Investigate a supported unattended launcher that avoids hidden console
   creation while retaining the existing console CLI and all output/error
   contracts. Isolated subsystem diagnosis improves median54.18 to22.865ms,
   but changing the existing CLI breaks interactive CMD waiting/error status.
   Relative variability worsens; do not claim a medal gain or use an artifact-only
   benchmark tool. See ITERATION_30_CONSOLE_LAUNCH_DIAGNOSIS.md.
2. Batch worker-local file/byte counters with a bounded progress lag and exact
   cancellation/final totals, reducing shared atomic operations per file.
3. Reserve small ranges of file IDs per worker while preserving uniqueness and
   parent relationships through cancellation and interleaved producers.
4. Attribute supported runtime/import startup costs before further linkage
   changes. Static CRT removes six direct imports but shows no paired speed
   benefit and is not adopted. See ITERATION_29_STATIC_CRT_DIAGNOSIS.md.
5. Separate frequently written worker counters where measured cache contention
   shows different counters sharing the same cache line.
6. Consume bounded local task groups to reduce global queue lock traffic without
   starving narrow branches or stranding cancellation work.
7. Size hardlink identity sets from bounded, scope-valid advisory cardinality
   rather than repeated growth; invalid hints must never affect accounting.
8. Reduce duplicate directory path encoding during supported directory opens.
9. Reuse bounded worker-local path conversion storage without sharing mutation.
10. Cache supported volume facts with explicit identity validation/invalidation.
11. Reduce directory-open metadata queries when the same supported handle already
    exposes the needed facts, preserving permission/error behavior.
12. Reuse small task containers within a measured retained-storage ceiling.
13. Reduce avoidable cloning during single-owner traversal task handoffs.
14. Move queue wakeups outside critical sections where race tests prove equivalent
    completion and prompt delivery; do not repeat already removed leaf wakeups.
15. Reduce queue lock hold time during child-task publication.
16. Reuse snapshot serialization scratch without sharing it across concurrent
    snapshots or weakening atomic persistence.
17. Reduce repeated integer formatting work in complete CSV serialization.
18. Consolidate repeated directory lookup work while exporting derived file paths.
19. Avoid redundant index publication traversals while preserving orphan/root
    handling and exact rollups.
20. Release transient publication state before persistence where ownership permits.
21. Reduce issue-record conversion/allocation without losing complete failures.
22. Avoid repeated clearing of enumeration-buffer tails only after validating every
    decoder read; do not reintroduce rejected enumeration sizes.
23. Simplify successful EOF continuation bookkeeping without changing errors.
24. Avoid repeated attribute/timestamp conversions when equivalent validated values
    are already owned by the current layer.
25. Evaluate CPU/profile-guided changes only after fresh phase attribution establishes
    that the affected user-code phase materially contributes to workflow spread.

## First bounded resource slice

The existing public concurrent allocation tracker retains 34,816 bytes for 64
single-volume identities, including its inline structure. The new public test
sets a 16 KiB complete retained-storage ceiling; it fails against the old code.
It also checks duplicate claims, a second volume, and full-width IDs. A separate
concurrent regression interleaves both volumes and narrow/wide aliases.

The accepted candidate owns single-volume identity sets directly, then moves
them intact into the existing map when a second volume appears. Concurrent
shards have cache-line alignment. Complete retained storage is now 11,520 bytes.
Workspace tests, examples, formatting, strict clippy and 92.48% line coverage
passed. Synthetic four-worker single-volume claims improved 27.91%, while the
four-volume case remained 1.09% slower; this is not process or medal evidence.
The full Release host validation passed, including all three unattended PNG
exports. Staged native/CLI identities are 32FB8F72/BCCE0057; UI is ADD07FBD.
Fresh original35 observations are running in refresh-20261008T095554026Z.
Resource RED evidence is artifacts/all-gold-loop/iteration-27/tracker-budget-red.log.

Keep all ten observations, unchanged definitions and full output validation.
Never improve relative variability by adding deliberate delay, changing sampling
boundaries, choosing favorable samples, or slowing the median. Comparator bests
remain frozen for the installed identity; WinBlaze uses the latest qualifying
cohort even when its variability worsens.
