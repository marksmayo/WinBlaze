# Iteration 9 candidates

Iteration 8 completed all 35 unchanged categories with fresh n=10 observations for all three products. Fresh-evidence, report, focus, sample-count, publication and 490 feature observations/negative controls passed. WinBlaze has 24 gold, 3 silver and 8 bronze medals. Competitor official values remain version/hash-specific best validated cohort statistics, including variability; WinBlaze remains latest. The visible-ready display verifier previously assumed milliseconds even when the report correctly displayed seconds; its regression now verifies the measured median in either displayed unit and passes.

Remaining categories: four timing-variability measures; full-drive workflow median, 90th percentile and CPU; startup first idle, CPU, working set and peak working set. Current startup working set is 35.44 MiB versus WinDirStat's retained 19.21 MiB. Three earlier full-directory CPU diagnostics attributed 92–93% of CPU to kernel work. A smaller enumeration-buffer candidate was rejected by a complete alternating comparison. Older 4/8/20-worker evidence shows faster wall time with more workers but substantially higher CPU; blindly increasing concurrency is not aligned with the remaining CPU target.

Ranked hypotheses (measure before adoption):

1. Measure a supported OS Windows.UI.Xaml desktop-island host with representative controls to establish whether its initialized runtime can plausibly clear the startup resource gap. This is a separate diagnostic, never a replacement product or medal result. A migration requires complete feature/theme/accessibility parity and unchanged measurement boundaries.
2. Attribute unavoidable versus avoidable WinUI initialization before first idle with matched reference hosts; do not delay initialization past the measured boundary.
3. Reduce redundant directory-handle metadata queries only with handle-established volume identity and mounted-volume/path-race correctness.
4. Reduce directory-open kernel work while preserving permissions, sharing, errors and reparse policy.
5. Cache supported volume facts with bounded storage and explicit invalidation; never infer identity from path prefixes.
6. Reduce repeated allocation-size queries where batched directory records already supply exact values.
7. Improve enumeration continuation sizing with deterministic resource evidence; do not repeat the rejected 6 KiB change without new evidence.
8. Bound worker-local traversal work to reduce shared-queue contention while preserving completion, cancellation and hierarchy.
9. Batch worker counters only where contention is measured; retain prompt progress and exact totals.
10. Recycle consumed pipeline vectors within the existing bounded queue/storage budgets.
11. Reduce fallback metadata conversions while preserving exact UTF-16 names and all timestamps.
12. Avoid repeated directory path construction where owned parent paths can safely be reused.
13. Reduce filename display allocation while preserving lossy conversion behavior.
14. Reduce full physical-identity hashing overhead without losing volume/full 128-bit identity or hardlink accounting.
15. Reduce duplicate finalization traversals with complete root/orphan/rollup semantics.
16. Release transient rollup storage before persistence without losing durable snapshot completeness.
17. Improve catalog growth allocation while retaining compact small/empty scans.
18. Reduce CSV serialization allocation with identical complete output and propagated I/O errors.
19. Reduce small-scan durable setup while preserving reopenable catalogs and failure reporting.
20. Reuse durable capacity information only when it matches the observed catalog; retain corruption recovery.
21. Reduce issue persistence allocation while retaining exact issue counts and recovery.
22. Remove eager UI dependencies only when actual pre-idle resident pages and functionality prove they are avoidable.
23. Reduce resource/theme initialization only with equivalent visible UI and accessibility coverage.
24. Improve code/data page locality using actual residency evidence; mapped image size alone is insufficient.
25. Evaluate native PGO only after measured user-code share justifies it; kernel-dominated timing is not evidence for PGO.
26. Remove redundant locale initialization if traces show repeated equivalent setup before first idle.
27. Consolidate UI string conversion at initialization boundaries without changing localized text.
28. Deduplicate theme dictionary loading while retaining light/dark/high-contrast resources.
29. Reduce font-resource activation only where actual resident attribution identifies unused fonts.
30. Narrow initial COM activation to required interfaces while preserving all initial UI controls.
31. Reuse application-level dispatcher access instead of repeated factory lookups.
32. Remove repeated window-class registration if startup traces establish duplication.
33. Reduce duplicate DPI-awareness negotiation without changing per-monitor behavior.
34. Simplify icon decoding with equivalent high-DPI rendering and accessibility.
35. Reduce initial bitmap intermediates without losing image quality or original assets.
36. Reduce avoidable framework metadata resolution through supported generated bindings.
37. Eliminate repeated resource-context construction while preserving language and theme resolution.
38. Reuse immutable initial toolbar state instead of repeated binding conversions.
39. Avoid duplicate settings-file reads with correctly invalidated cached settings.
40. Reduce settings parsing allocation without changing defaults or recovery.
41. Combine startup settings validation passes when observable error behavior stays identical.
42. Avoid redundant installed-runtime discovery only where supported runtime identity remains checked.
43. Reduce duplicate native-library loading while preserving mandatory ABI/version validation.
44. Narrow native DLL eager imports only where startup residency proves an avoidable dependency.
45. Split optional rendering dependencies behind existing first-use boundaries without moving required startup work.
46. Reduce redundant startup logging conversions while retaining useful diagnostic events.
47. Bound diagnostic logging buffers without losing failures or changing benchmark boundaries.
48. Remove empty cache-restore temporary objects while preserving empty-catalog behavior.
49. Reuse initial navigation model storage without retaining obsolete scan models.
50. Reduce initial search-state construction while preserving immediately enabled search controls.
51. Avoid duplicate empty-tree layout passes when first visible content remains correct.
52. Reduce initial layout invalidations caused by repeated equivalent property assignments.
53. Set equivalent initial properties before control attachment to reduce repeated layout work.
54. Consolidate resize notifications while preserving final bounds and usable-window readiness.
55. Reduce redundant accessibility property updates while retaining complete names and enabled states.
56. Reuse immutable command descriptions without changing accelerator or automation behavior.
57. Reduce process-lifetime registration objects while preserving event subscription lifetimes.
58. Reduce temporary startup exception-context allocation on the successful path.
59. Simplify native-to-UI initial empty snapshot conversion with identical public state.
60. Reduce empty collection wrapper allocation without changing collection notifications.
61. Avoid duplicate directory-entry attribute decoding across scanner layers.
62. Avoid repeated UTF-16 length scans when the kernel record already supplies a validated length.
63. Reuse bounded path-encoding scratch space for directory opens without sharing mutable buffers across workers.
64. Reduce extended-path prefix reconstruction with correct UNC, relative and long-path handling.
65. Cache immutable root normalization per request without hiding path-resolution errors.
66. Reduce repeated parent-ID lookups where ownership already supplies the correct parent.
67. Improve child-ID allocation synchronization while retaining unique IDs and exact hierarchy.
68. Move immutable traversal policy checks out of per-entry branches without changing followed-reparse behavior.
69. Reduce lock hold time when assigning queued directories while preserving task ownership.
70. Separate queue wakeups from counter updates if contention evidence justifies it.
71. Avoid redundant condition-variable wakeups when no new consumable work was published.
72. Reduce redundant queue-empty checks without changing terminal completion races.
73. Reuse small task containers only within an explicit bounded memory budget.
74. Reduce task path cloning for single-owner handoffs without changing lifetime safety.
75. Pack worker-local task bookkeeping while preserving cancellation/error context.
76. Remove duplicate zero-initialization of reused enumeration storage where safe decoding proves initialization.
77. Reduce clearing of unused enumeration buffer tails while preserving record bounds and invalid-record rejection.
78. Separate the EOF continuation path from growth bookkeeping to avoid unnecessary successful-path work.
79. Avoid allocating names for structural records beyond the existing dot-entry optimization.
80. Preserve kernel-provided timestamp values through the pipeline instead of equivalent repeated conversions.
81. Reduce repeated file-attribute mapping with a validated lookup or bit mapping.
82. Improve hardlink set capacity estimation using measured cardinality without oversized idle storage.
83. Reduce physical-identity map rehashes while preserving bounded small-scan memory.
84. Reduce duplicate hardlink accounting lookups with one atomic entry operation.
85. Avoid allocating issue strings until an actual reportable error occurs.
86. Reuse error category lookup results while preserving exact error-code reporting.
87. Reduce successful fallback probing only where supported capability can be safely cached per volume.
88. Bound cached capability lifetime so mount/remount changes cannot select an invalid backend.
89. Avoid repeated root validation queries only if the same live handle establishes equivalent facts.
90. Reduce redundant scanner-to-index record transformations with unchanged public event shapes.
91. Reserve event capacity from observed batch sizes without restoring oversized standalone control events.
92. Reduce consumer batch iteration overhead while retaining ordered callbacks and cancellation.
93. Consolidate index record updates with equivalent transaction and visibility boundaries.
94. Reduce index key encoding allocation with identical persisted bytes and lookup results.
95. Stream complete CSV rows through reusable bounded formatting storage.
96. Avoid repeated CSV header/schema construction per operation while preserving compatibility formats.
97. Reduce export timestamp formatting duplication using per-operation validated timezone context.
98. Reuse durable-write serialization scratch buffers without sharing them across independent snapshots.
99. Reduce duplicate flushes only where durability contracts and injected-failure tests establish equivalence.
100. Evaluate storage-specific concurrency only with complete wall/CPU/resource evidence and explicit user overrides; do not adopt the old higher-worker CPU regression.

Use red/green public behavior/resource tests for production changes, followed by workspace/examples/formatting/strict Clippy/coverage, relevant UI checks, a complete controlled comparison and then the original all-35/all-three n=10 rerun. Keep every raw observation. Do not stop unrelated work, choose favorable samples, alter timing boundaries, trim working sets, change priorities or relax medal definitions.

The restarted administrator cohort completed successfully. Its retained worker later disappeared from the current process view; its prior ready status alone is not proof of a live token. No current measurements are running. Do useful development before another approval is needed; never launch individually elevated competitor repetitions as a fallback.
