# Iteration 5 candidates

Iteration 4 completed: batch `20261006T004939113Z`, archive `runs/2026-10-06T01-42-49-287Z/report-data.json`. All 35 original categories have fresh n=10 across all three products; all 490 feature observations and final evidence checks passed. WinBlaze has 27 gold, 6 silver and 2 bronze. Full workflow median, p90 and CPU are now gold; startup and variability remain incomplete.

Remaining values (WinBlaze / retained best competitor):

| Category | WinBlaze | Target |
| --- | ---: | ---: |
| Quick timing variability, percent | 12.271777 | 2.962843 |
| Scale timing variability, percent | 7.266327 | 4.014511 |
| Dense timing variability, percent | 7.165573 | 6.121188 |
| Full timing variability, percent | 7.036364 | 5.525888 |
| Startup first idle, ms | 100.86 | 99.725 |
| Startup CPU, CPU-s | 0.140625 | 0.09375 |
| Startup working set, MiB | 36.029297 | 19.216797 |
| Startup peak working set, MiB | 36.029297 | 19.216797 |

Ten fresh diagnostic traces put first idle before the main-window constructor. Shell construction changes therefore do not address these startup medals. Trace logging perturbs timings; no trace timing becomes a medal measurement. Fresh thirty module snapshots are diagnostic only; mapped image size does not establish resident memory.

Ranked candidates requiring evidence and public red/green tests:

1. Attribute actual resident pages at first idle to images/private allocations, then reduce the largest avoidable initialization source.
2. Remove redundant pre-idle runtime/factory activation established by fresh module and phase evidence.
3. Reduce pre-entry loader work from avoidable eager imports; preserve first-use behavior and errors. Existing shell/graphics delay loading and native dynamic binding are already present.
4. Reduce application-resource initialization only where cost is measured; preserve themes, resources and rendered behavior.
5. Evaluate supported runtime compatibility configuration against actual activation cost, preserving the same SDK and UI behavior.
6. Improve resident code/data layout using resident-page attribution, not mapped image sizes; existing release optimization flags remain the control.
7. Reduce startup heap fragmentation only with live-allocation evidence. Previously tested SegmentHeap is not an established improvement.
8. Reduce loader/runtime duplicate work found in actual imported-module dependency chains.
9. Improve short-scan allocation/persistence setup measured in launch-to-exit phase evidence; retain complete durable snapshots.
10. Recycle event vectors within a fixed resource bound to reduce allocation variability; exact events, cancellation and receiver-drop tests.
11. Batch worker-local accounting with exact progress and terminal totals; establish contention first.
12. Reduce shared queue contention using bounded worker-local tasks with complete wide/deep hierarchy tests.
13. Improve adaptive directory buffer behavior at density transitions; existing allocation limits and all metadata remain intact.
14. Reduce directory open setup proven by kernel CPU evidence; real identity and path-race safety remain required.
15. Avoid redundant volume identity queries only when actual handles prove identity, including mounted volumes and reparse roots.
16. Reduce filename conversion allocation with exact Unicode/unpaired-surrogate behavior.
17. Reduce physical identity hashing contention while preserving full 128-bit IDs and hardlink accounting.
18. Reduce terminal/progress arena retention while preserving ordered delivery and prompt progress.
19. Reduce indexing dispatch cost if measured; retain callbacks, complete catalog and reopen semantics.
20. Reduce CSV writer/formatting overhead; byte-identical output and error propagation.
21. Reduce redundant model-finalization traversals; exact rollups and root/orphan behavior.
22. Improve child ordering only where actual comparison profiles establish redundant cost; preserve tie rules.
23. Reduce catalog growth-boundary allocations without retaining oversized empty arenas.
24. Reduce issue/hint persistence overhead only where profiles justify it; no lost issues, weakened durability or skipped scanning.
25. Evaluate native PGO only after proving enough user-code cost remains; isolated training and complete output validation before adoption.

Prior local-SDK deployment, SDK 2.4, SegmentHeap and unsupported system composition experiments do not establish startup resource gains and are not adopted. Keep all source observations, original definitions, ten-run protocols and version-specific competitor bests. No delay insertion, priority changes, working-set trimming, shifting launch work beyond first idle or selecting favorable samples.

Next: resident-page attribution at the unchanged boundary. Choose one supported public/resource slice from the evidence, demonstrate RED then GREEN, complete applicable gates and controlled comparisons, and rerun all 35/all three products. Goal remains incomplete with eight deficits.
