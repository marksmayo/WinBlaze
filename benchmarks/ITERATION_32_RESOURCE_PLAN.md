# Memory and CPU priorities after iteration 28

The latest complete report has 35 rows, 32 gold and 3 silver, with startup CPU
pending because one of ten observations was zero. The three silvers are timing
variability. Following the user's direction, target memory and processing work;
variability remains an outcome under the original definitions.

These 100 hypotheses are ranked by estimated impact and feasibility, not proven
benefit. The first 25 are the selected implementation/investigation queue. Each
requires a public behavior/resource RED, GREEN, relevant gates and current
WinBlaze measurements before an improvement is claimed. Previous investigations
and rejected candidates remain documented in the historical iteration plans.

1. Compact clustered hardlink identities exactly, retaining sparse/full-width fallback. Implemented candidate: million-file retained storage 18,883,584 to 8,400,896 bytes; adoption gates pending.
2. Attribute and reduce the newly observed startup-memory increase while preserving the corrected keyboard-focus contract; candidate32 partial WS16.789MiB exceeds the retained16.244MiB gold target.
3. Batch worker-local file and byte progress counters, preserving final/cancel totals.
4. Allocate bounded file-ID ranges per worker, preserving parent relationships and uniqueness.
5. Avoid redundant path materialization between enumeration, records and export.
6. Reuse bounded worker-local UTF-16 conversion storage.
7. Reduce duplicate volume-fact queries using validated handle identity.
8. Avoid repeated directory metadata queries already answered by enumeration.
9. Remove redundant index publication traversals while preserving orphan handling.
10. Serialize file paths directly from shared ancestry without temporary full paths.
11. Reduce duplicate owned names in index construction.
12. Bound snapshot serialization scratch across concurrent snapshots.
13. Reuse traversal task containers within explicit retained-memory ceilings.
14. Batch bounded queue publication to reduce lock acquisitions.
15. Consume bounded local task groups without starving narrow branches.
16. Shorten queue critical sections around child-task publication.
17. Eliminate clones at single-owner task handoffs.
18. Separate frequently written counters only where measured cache contention exists.
19. Reuse CSV integer-formatting scratch while preserving complete exports.
20. Consolidate directory ancestry lookups during export.
21. Reduce temporary index result vectors in public operations.
22. Share immutable scan/filter configuration across workers without per-file cloning.
23. Reuse bounded duplicate-content buffers across comparison groups.
24. Stream duplicate candidate grouping to reduce peak simultaneous ownership.
25. Avoid duplicate rollup work on unchanged subtrees without stale results.
26. Compact timestamp presence metadata without losing precision.
27. Reduce retained task-path capacity after exceptionally deep branches.
28. Release finished worker scratch promptly at scan completion.
29. Reserve index node storage from validated observed counts.
30. Reserve event storage only when the first event arrives; audit remaining paths.
31. Reuse bounded terminal-event arenas without retaining oversized batches.
32. Reduce repeated error-context formatting on successful paths.
33. Avoid unnecessary path normalization when an existing invariant applies.
34. Preserve native directory names through the scan pipeline before conversion.
35. Reduce repeated extension extraction in grouping queries.
36. Store common extensions once per scan where measured worthwhile.
37. Reduce repeated case-folding in search query execution.
38. Precompute query constants once per public request.
39. Avoid cloning result records for read-only filtering.
40. Stream bounded query results to consumers.
41. Cache immutable rollup facts within an index generation.
42. Reduce duplicate rollup maps during publication.
43. Reuse bounded rollup traversal stacks.
44. Release replaced index generations when readers finish.
45. Avoid indexing unused presentation fields eagerly.
46. Build secondary indexes lazily on first supported query.
47. Compact secondary-index row references with checked bounds.
48. Reduce temporary sorting keys for largest-file queries.
49. Use bounded selection for top-N results instead of full sorting.
50. Reuse bounded sort scratch across independent public requests.
51. Avoid unnecessary snapshot decompression copies.
52. Stream validated snapshot sections into final ownership.
53. Reduce snapshot checksum passes without weakening validation.
54. Batch snapshot writes without changing atomic persistence.
55. Avoid duplicate snapshot directory-name encoding.
56. Compact snapshot in-memory headers after validation.
57. Release import buffers before index publication.
58. Reduce repeated FFI record conversions.
59. Batch FFI transfer with unchanged cancellation/backpressure.
60. Reuse bounded FFI callback scratch.
61. Avoid duplicate string allocation across FFI ownership transfer.
62. Share immutable UI display records within a generation.
63. Limit tree realization to visible branches while preserving accessibility.
64. Recycle UI row containers without stale bindings or focus.
65. Reduce repeated byte formatting in unchanged UI cells.
66. Cache validated display text by record/version.
67. Avoid rebuilding unchanged treemap geometry.
68. Bound treemap scratch after an exceptionally large view.
69. Batch UI updates with unchanged readiness and completion boundaries.
70. Avoid duplicate UI sorting on already ordered results.
71. Cancel obsolete background search promptly.
72. Release superseded search buffers after cancellation.
73. Avoid repeated filesystem access for unchanged details panels.
74. Share read-only icon metadata without leaking shell handles.
75. Reduce redundant native-to-managed display conversions.
76. Reuse bounded PNG encoder scratch across exports.
77. Avoid duplicate pixel buffers during PNG export.
78. Stream image encoding where fidelity tests permit.
79. Release exported image storage before normal process shutdown.
80. Reduce duplicate history serialization buffers.
81. Batch history persistence without changing durability semantics.
82. Bound history query materialization.
83. Avoid cloning full scan state for history summaries.
84. Reduce duplicate rule compilation across scan roots.
85. Order cheap exact filter predicates before expensive path matching.
86. Reuse rule-matching scratch within one worker.
87. Avoid redundant device enumeration for disconnected-device workflows.
88. Release disconnected-device COM state promptly.
89. Cache supported device facts with explicit reconnect invalidation.
90. Avoid constructing portable-device buffers before a device exists.
91. Reduce duplicate VHD metadata buffers without weakening validation.
92. Release detached-volume bookkeeping promptly.
93. Reuse bounded file-operation progress storage.
94. Avoid duplicate metadata reads during rename validation.
95. Reduce per-file allocation in permission reporting.
96. Stream storage-cost calculations from existing index summaries.
97. Avoid redundant update-manifest parsing within one update check.
98. Release updater download buffers before launch handoff.
99. Attribute remaining eager DLL initialization before changing imports.
100. Attribute runtime shutdown work before considering further startup changes.

All 70 competitor bests remain frozen for their recorded identities, including
variability. WinBlaze uses its latest qualifying cohort even when it worsens.
No sleeps, priority changes, favorable resampling or benchmark-definition changes.
