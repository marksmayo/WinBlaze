# Iteration 11 ranked improvement candidates

Current prioritization (iteration32): the user directs memory and CPU reductions.
The current ranked100 and selected top25 are in ITERATION_32_RESOURCE_PLAN.md.
Historical rankings below remain evidence of earlier investigations. The
supported console manifest in ITERATION_31_CONSOLE_POLICY.md supersedes the
separate-launcher proposal. Exact clustered identity storage is the current
memory candidate; see ITERATION_32_COMPACT_IDENTITIES.md. Adoption gates and a
fresh original35 cohort are pending; the last official report remains32G/3S.

Current integration update (iteration28): Release/x64 stages the full OS XAML
host through the project build. Historical isolated-only statements below refer
to their original cohorts. Current UI6D5ACFDE/native68EA02B6/CLI D576E8F1 has
32 gold,3 variability silvers,0 bronze across all35 unchanged categories.
Fresh qualifying coverage is34/35; one zero startup-CPU reading leaves that
official value labelled pending. All70 competitor best objects and official
values are unchanged, and all final report validators pass.

Single-volume identity tracking reduces complete retained storage34,816 to11,520
bytes with RED/GREEN and mixed-volume/full128-bit fidelity. Bounded event reuse
reduces2048-file batch allocations65 to3 with public backpressure/unwind and
oversize-retention tests. Coverage92.40%, workspace/examples/strict lint and
Release UI checks pass. See ITERATION_27_SINGLE_VOLUME_TRACKER.md and
ITERATION_28_RECYCLED_EVENT_BATCHES.md. Full-C: variability remains gold at3.706530%.

The next25 priorities are updated in ITERATION_27_VARIABILITY_PLAN.md. Isolated
static CRT removes six direct imports but has no paired speed benefit and is
not adopted. A PE Windows-subsystem experiment improves hidden-launch median
54.18 to22.865ms with identical code/data, but worsens relative variability and
breaks interactive CMD waiting/error propagation. It is not adopted. Investigate
a fully supported unattended launcher preserving the existing console CLI;
do not use an artifact-only benchmark tool. See ITERATION_29_STATIC_CRT_DIAGNOSIS.md
and ITERATION_30_CONSOLE_LAUNCH_DIAGNOSIS.md. Measurement boundaries and all
slower WinBlaze observations remain unchanged.

The completed iteration-14 report has 27 gold, 7 silver, and 1 bronze across all 35 unchanged categories. Compact paths, names and aligned timestamp presence now give a104-byte record, down24bytes from128; standard-user peak andprivate memory earn gold (private432.06MiB vsWD444.84); latest full-volume and empty-file variability are silver. Startup working set at first idle is again the sole bronze and the user's priority; quick-folder variability is silver. Latest measured shipping is 32.857421875 MiB versus retained WinDirStat 16.244140625 MiB. The latest isolated full OS XAML candidate includes native window icons (8D0481F0/9FF4B2B): canonical ten-run startup working set16.455078125MiB remains RED. The newer bitmap-encoded small-icon candidate998370A1/9FF4B2B measures16.39453125MiB RED (154KiB gap); exact rendered pixels match all seven icon sizes and matched fullUI/DPI/shutdown checks pass. See ITERATION_17_BITMAP_ICON_STARTUP.md. The latest manifest-only SegmentHeap candidateC4EB3D77/9FF4B2B measures16.28125MiB RED (38KiB gap); matched control/DPI/shutdown and no-device/rescan checks pass. The same setting on a separate shipping WinUI copy measures33.783203125MiB and is not adopted. See ITERATION_18_SEGMENT_HEAP_STARTUP.md. The global-data packaging experiment4755DA1A measures16.95703125MiB, does not reduce executable size, and is rejected; its zero CPU observation is retained and rejected by the strict resource verifier. See ITERATION_19_GLOBAL_DATA_STARTUP.md. The latest supported legacy-OLE-DDE experiment5131414A measures16.255859375MiB RED (12KiB gap); all controls/icons/DPI/shutdown and six no-device workflows pass, but interactive keyboard prerequisites are absent for both candidate and shipping. See ITERATION_20_OLE_DDE_STARTUP.md. The latest candidate4A0F08C4 restores shipping first-use DLL linking omitted by the prototype command. Actual PE import RED/GREEN and the unchanged canonical resource verifier now pass:14.826171875MiB,1.41796875MiB below target. Controls/icons/DPI/normal-release, real PNG export and full file-operation checks pass. It remains isolated and not adopted pending runtime parity/integration. See ITERATION_21_FIRST_USE_STARTUP.md. Adoption audit22 confirms intentional320 directory floor: visible-folder behavioral RED180/GREEN320; all nine fresh feature checks pass on21. Latest23UI6C5732A8 restores startup failure logging/dialog/HRESULT with actual RED/GREEN; memory15.6171875MiB clears target, but one observed zero CPU leaves nine qualifying CPU samples and the unchanged complete-resource gate RED. Earlier21green is not substituted. See ITERATION_22_ADOPTION_AUDIT.md and ITERATION_23_STARTUP_FAILURE_PARITY.md. The earlier manifest-only 5CAADC98 candidate measured16.22265625MiB GREEN, but lacked window icons and was not adopted. Corrected icon parity has actual shipping GREEN/candidate RED/fix GREEN evidence. Further unchanged path/window/import and no-device tests pass. See ITERATION_16_STARTUP_ICON_PARITY.md. Whole-program and size optimization both failed to reduce first-idle memory in matched ten-pair cohorts. No runtime candidate adoption is claimed. Future original-35 medal iterations rerun WinBlaze only, retaining version-matched competitor bests; the elevated worker's relative-path skip regression is now fixed and passes twice.

Iteration21 clears the resource target with meaningful margin. Prioritize adoption parity and a reproducible supported product build before further startup-memory experiments; the official bronze remains until adoption and a complete fresh WinBlaze35 cohort. This reprioritizes the existing 100 hypotheses from fresh evidence. The first 25 are the startup investigation priorities; measure each before implementing it. Ownership, keyboard/accessibility/theme and functional parity are adoption prerequisites for the runtime candidate. A pending-dialog window-retention failure has now been observed with a weak-reference release assertion, so shutdown ownership is active prerequisite work.

1. Measure a supported OS Windows.UI.Xaml desktop-island host with representative controls to establish whether its initialized runtime can plausibly clear the startup resource gap. This is a separate diagnostic, never a replacement product or medal result. A migration requires complete feature/theme/accessibility parity and unchanged measurement boundaries.
2. Attribute unavoidable versus avoidable WinUI initialization before first idle with matched reference hosts; do not delay initialization past the measured boundary.
3. Narrow initial COM activation to required interfaces while preserving all initial UI controls.
4. Remove eager UI dependencies only when actual pre-idle resident pages and functionality prove they are avoidable.
5. Reduce resource/theme initialization only with equivalent visible UI and accessibility coverage.
6. Improve code/data page locality using actual residency evidence; mapped image size alone is insufficient.
7. Split optional rendering dependencies behind existing first-use boundaries without moving required startup work.
8. Deduplicate theme dictionary loading while retaining light/dark/high-contrast resources.
9. Reduce font-resource activation only where actual resident attribution identifies unused fonts.
10. Reduce repeated byte-label stream/locale allocation without changing UI text. Public allocation RED/GREEN passes (2,206,512 to 698,672 bytes across 23,560 labels); startup memory remains RED at 17.40039 MiB in the isolated candidate. Shipping functional UI checks pass; the separate tiny UI timing budget remains RED.
11. Reduce duplicate native-library loading while preserving mandatory ABI/version validation.
12. Narrow native eager imports and measured launcher startup costs while preserving supported behavior. Static CRT has no observed speed benefit and is not adopted. A Windows-subsystem launcher has a paired hidden-launch speed benefit but cannot replace the existing console CLI because interactive CMD wait/error propagation breaks; investigate a supported unattended companion with complete fidelity.
13. Avoid redundant installed-runtime discovery only where supported runtime identity remains checked.
14. Reuse application-level dispatcher access instead of repeated factory lookups.
15. Remove repeated window-class registration if startup traces establish duplication.
16. Reduce duplicate DPI-awareness negotiation without changing per-monitor behavior.
17. Simplify icon decoding with equivalent high-DPI rendering and accessibility.
18. Reduce initial bitmap intermediates without losing image quality or original assets.
19. Reduce avoidable framework metadata resolution through supported generated bindings.
20. Eliminate repeated resource-context construction while preserving language and theme resolution.
21. Reuse immutable initial toolbar state instead of repeated binding conversions.
22. Avoid duplicate settings-file reads with correctly invalidated cached settings.
23. Reduce settings parsing allocation without changing defaults or recovery.
24. Combine startup settings validation passes when observable error behavior stays identical.
25. Reduce redundant startup logging conversions while retaining useful diagnostic events.
26. Retain bounded advisory allocation estimates for recent root/rules scopes instead of overwriting one scope on every folder change. Public alternating-root RED/GREEN now passes; up to 32 scopes retain legacy advice, with unchanged 5% growth room. Rust gates and 92.46% coverage pass, release builds and functional UI checks pass; tiny UI timing remains RED. Iteration13 all ten full CLI runs peak around460-463 MiB, with no660 MiB first-scan spike; first elapsed worsens to43549ms and stays included. Variability remains silver6.64%; causal speed improvement is unproven. See ITERATION_12_SCOPED_HINTS.md and ITERATION_13_COMPACT_FILE_NAMES.md.
27. Reduce directory-open kernel work while preserving permissions, sharing, errors and reparse policy.
28. Cache supported volume facts with bounded storage and explicit invalidation; never infer identity from path prefixes.
29. Reduce repeated allocation-size queries where batched directory records already supply exact values.
30. Improve enumeration continuation sizing with deterministic resource evidence; do not repeat the rejected 6 KiB change without new evidence.
31. Bound worker-local traversal work to reduce shared-queue contention while preserving completion, cancellation and hierarchy.
32. Batch worker counters only where contention is measured; retain prompt progress and exact totals.
33. Recycle consumed pipeline vectors within the existing bounded queue/storage budgets.
34. Reduce fallback metadata conversions while preserving exact UTF-16 names and all timestamps.
35. Avoid repeated directory path construction where owned parent paths can safely be reused.
36. Reduce filename display allocation while preserving lossy conversion behavior.
37. Reduce full physical-identity hashing overhead without losing volume/full 128-bit identity or hardlink accounting.
38. Reduce duplicate finalization traversals with complete root/orphan/rollup semantics.
39. Release transient rollup storage before persistence without losing durable snapshot completeness.
40. Reduce retained file-record storage with lossless grouped timestamp presence and immutable owned names, retaining complete paths, snapshot/search/export/rename behavior. Boxed full paths now reduce x64 layout from 136 to 128 bytes with public memory/fidelity RED/GREEN. Boxed names and an aligned lossless timestamp group now pass separate actual resource RED/GREEN tests, all64 timestamp presence/extreme combinations, persisted snapshot fidelity, workspace/examples/fmt/strictclippy and92.47%coverage. Measuredx64layout104 saves24bytes below128, about41MiB across1.79Mfiles before allocator/process effects. Release/native/UI functional gates pass; separate2000ms tinyUIbudget remainsRED2425ms. Original35 iteration14 provesprivate432.06MiB nowGOLD withallmetadatafidelity and490featureobservations; timing/startup deficits remain. See ITERATION_14_COMPACT_RECORD_RESULTS.md.
41. Reduce CSV serialization allocation with identical complete output and propagated I/O errors.
42. Reduce small-scan durable setup while preserving reopenable catalogs and failure reporting.
43. Reuse durable capacity information only when it matches the observed catalog; retain corruption recovery.
44. Reduce issue persistence allocation while retaining exact issue counts and recovery.
45. Evaluate native PGO only after measured user-code share justifies it; kernel-dominated timing is not evidence for PGO.
46. Consolidate UI string conversion at initialization boundaries without changing localized text.
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
100. Evaluate storage-specific concurrency only with complete wall/CPU/resource evidence and explicit user overrides; do not adopt the old higher-worker CPU regression. Current rotating ten-scan cohorts reject two/three workers for the variability target: four is fastest, three has effectively unchanged variability, two is worse. These scanner-only diagnostics do not replace persisted/exported medals.

Apply one public behavior/resource RED-GREEN slice at a time. Keep all raw observations and unchanged measurement boundaries; no working-set trimming, priority changes, favorable sample selection, or postponed required initialization. After an accepted production improvement, run appropriate repository gates and refresh WinBlaze across all 35 categories at n=10. Reuse complete version/hash-matched competitor bests; refresh an individual competitor only when its identity changes or its baseline evidence is missing/invalid. The latest user instruction supersedes the older all-three-on-every-iteration cadence.
