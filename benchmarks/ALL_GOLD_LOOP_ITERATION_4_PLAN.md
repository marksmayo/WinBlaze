# Iteration 4 performance candidates

Confirmed iteration 3 evidence: `runs/2026-10-06T00-18-02-056Z/report-data.json`; all 35 original definitions, fresh n=10 across all products, 490 validated feature observations. WinBlaze: 24 gold, 7 silver, 4 bronze. The all-gold gate remains RED. Competitor records retain their version-specific best validated statistics.

Remaining targets: quick/dense/scale/full timing variability; full workflow median and p90; standard-user full-drive CPU; startup first-idle time, CPU, working set and peak working set. No administrator deficits. Startup resource targets remain 19.216796875 MiB and 0.09375 CPU-s; full CPU target remains 89.49219 CPU-s. Fresh competitor startup target is 99.725 ms; full variability target is 5.525888421531474 percent.

Ranked hypotheses, subject to diagnostic evidence:

1. Evaluate original SDK 1.8 local deployment and supported eager registration-free initialization in an isolated UI build. Keep full launch behavior, verify loaded modules and functional UI, then compare unchanged first-idle resource cohorts.
2. Reduce directory open setup: all three completed CPU diagnostics show kernel work dominates. Preserve actual handle identity, errors, metadata and reparse behavior.
3. Optimize enumeration buffer size with measured I/O evidence. Exact names, identities, EOF/error recovery and bounded scratch.
4. Avoid redundant volume identity queries only with identity proven from real handles. Mounted volumes and concurrent path changes remain correct.
5. Recycle consumed event vectors under a fixed count/byte budget. Public slow-consumer, receiver-drop and complete-delivery resource tests.
6. Batch worker-local file/byte accounting before shared publication if later phase profiles support user-code contention. Public progress, exact completion and cancellation tests; no loss of pending counters.
7. Evaluate native PGO only if later profiles establish sufficient user-code benefit. Isolated generation/training/use builds, complete export/persistence validation and alternating comparisons before adopting flags.
8. Reduce UTF-16 conversion allocation while retaining exact on-disk names and lossy display semantics.
9. Reduce shared task queue contention through bounded local tasks. No stranded tasks; complete wide/deep and cancellation tests.
10. Reduce physical identity hashing cost if profiles show it matters. Full 128-bit IDs, volume separation and concurrent hardlinks.
11. Reduce pre-idle duplicate factory activation established by fresh phase traces. Preserve all launch behavior.
12. Remove unused startup resources only after actual resource-use and resident-memory attribution. Theme and rendered UI must remain equivalent.
13. Improve startup code/data layout using resident-page evidence. Existing O2/GL/REF/ICF are already enabled.
14. Reduce redundant startup runtime binding only if loaded module evidence proves duplication.
15. Reduce terminal/progress batch arena retention while preserving event ordering and prompt progress.
16. Specialize terminal-only indexing dispatch if it is a measured hot path. Live callbacks, full catalog and persisted reopening remain correct.
17. Reduce CSV formatting/writer overhead with byte-identical RFC4180 output, integer/date boundaries and propagated errors.
18. Reduce redundant tree finalization passes with exact rollups, root/orphan handling and complete validation.
19. Improve child ordering only where actual sorting profiles justify it. Preserve size/name/ID ties.
20. Reduce short-scan root/path normalization costs with Unicode, relative paths, volume roots and rules preserved.
21. Reduce allocation growth at large catalog boundaries; retain replacement/removal behavior and all identities.
22. Release temporary finalization storage before durable writes; public peak allocation and source-removed snapshot reopening tests.
23. Reduce issue persistence cost if full-scan profiles justify it; retain every issue and error/rotation behavior.
24. Reduce session-local allocation hint reload overhead without skipping scans, corruption recovery or durable updates.
25. Investigate short-workflow process initialization and shutdown cost with phase evidence. Preserve console behavior, persistence and complete output; do not alter benchmark launch protocol.

First action: three separate full traversal CPU decompositions, after the iteration-3 wrapper terminated successfully. These diagnostics do not update medals. No build or independent UI launch while diagnostics hold the measurement mutex. Then choose one public observable/resource slice, establish RED, implement GREEN, run applicable gates and controlled comparisons, and rerun all 35 across all three products.

Rejected experiments remain rejected: system composition returned false and produced no window; SDK 2.4 and SegmentHeap did not establish the required startup memory gains. No failed or selected observations become official measurements. No priority changes, added delays, working-set trimming, omitted metadata or weakened durability.

CPU diagnosis completed: `../artifacts/all-gold-loop/iteration-3/cpu-diagnostic-d75802ecbfca4b14b35f9509ca553dd6/results.json`. Three full traversals retained; user/kernel CPU seconds: 6.16/80.47, 7.19/83.08, 6.62/81.44 (rounded). Kernel fraction about 92-93 percent. This is separate diagnostic evidence, not medal data. The first attempt failed on an atomic-file-replacement sandbox permission after one published scan; its files remain retained. A fresh three-run diagnostic completed with authorized filesystem access. Ranking 2-7 updated accordingly; no product change inferred from these observations.
