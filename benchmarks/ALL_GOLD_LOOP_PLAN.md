# All-gold loop, 6 October 2026

Baseline: fresh batch 20261005T120729846Z, 35 original categories, n=10 per product. WinBlaze: 27 gold, six silver, two bronze. Keep category names, scopes, metrics, sampling, fixtures and full output validation unchanged. No artificial delays, priority changes, working-set trimming or measurement-boundary changes.

Remaining deficits: startup CPU 0.1328 vs 0.09375 CPU-s; startup working set/peak 34.72 vs 19.42 MiB; full-drive private memory 590.92 vs 540.28 MiB; IQR/median variability tiny 12.86 vs 11.13%, scale 8.82 vs 8.19%, dense 25.39 vs 6.12%, full 19.16 vs 7.68%.

Ranking is an engineering hypothesis order, not a claim of measured savings. Re-rank from new evidence after each complete three-product rerun. Existing optimizations must not be counted as new implementations.

| Rank | Candidate | Target and falsifiable check |
|---:|---|---|
| 1 | Bound retained filename capacity after UTF-16 conversion | Private memory: public scan records must preserve names/timestamps/sizes with bounded spare string capacity. Measure baseline before changing conversion. |
| 2 | Reduce large file-arena growth overshoot | Private memory/variability: public transaction allocation and peak budgets across growth boundaries, exact IDs/replacements/removals. Compare realloc work against retained slack. |
| 3 | Avoid a second large arena during publication | Private memory: public tree handoff measures peak live bytes, not just cumulative allocations; preserve contiguous public records. |
| 4 | Release physical-identity bookkeeping as soon as traversal ends | Private memory: verify hardlink totals and cancellation while measuring peak before model publication. |
| 5 | Bound retained directory path/name capacities | Private memory: complete Unicode paths and imported explicit paths round-trip with tighter allocation budgets. |
| 6 | Avoid transient duplicate directories during event handoff | Private memory: public complete catalog and root rollups, measure live allocation at producer/consumer seam. |
| 7 | Stream final CSV with reusable path/escaping scratch | Full/dense variability: exact RFC4180 output, long Unicode names and escaping; allocations independent of file count. |
| 8 | Amortize per-file snapshot writes into bounded chunks | Full/dense variability: reopen after scan source removal, recovery and exact metadata; bounded buffer, unchanged durability. |
| 9 | Reserve event batches once and recycle bounded storage | Dense/scale variability: terminal ordering, cancellation, complete counts and bounded memory through public scan controller. |
| 10 | Batch child-directory queue submissions | Full/scale variability: exact traversal and cancellation, fewer queue acquisitions under owned fixture load. |
| 11 | Adapt idle worker wakeups to actual queued work | Dense/tiny variability: no missed wakeups or stranded completion; do not increase minimum elapsed time. |
| 12 | Avoid unnecessary parallel workers for one dense leaf | Dense variability: sequential and parallel policy output equivalence, cancellation and CPU budget. |
| 13 | Tune batch-size thresholds from producer/consumer profiles | Dense/scale variability: same event semantics and responsive cancellation; sweep one setting at a time. |
| 14 | Avoid repeated file-parent lookups during tree construction | Full private memory/variability: exact sparse, orphan and boundary-ID rollups; do not add a file-sized auxiliary array. |
| 15 | Skip child sorting when a range is already ordered | Dense/full variability: observable physical-size order and tie handling remain identical. |
| 16 | Fuse compatible tree construction passes | Full memory/variability: exact roots/rollups and measured temporary allocation budget; preserve cycle safeguards. |
| 17 | Inherit verified volume identity across ordinary directories | Full variability: preserve full 128-bit identity, mounted volumes, junction policy and hardlink correctness; measure metadata calls first. |
| 18 | Move shared progress counters into worker-local batches | Full variability: monotonic progress and exact final totals, cancellation and partial output. |
| 19 | Reduce repeated progress clock reads | Dense/tiny variability: preserve time/item thresholds and terminal flush, measure observable event behavior. |
| 20 | Reduce startup module imports still used only after first idle | Startup CPU/memory: use fresh module and phase inventory; verify full window/scan functionality and missing dependency failures. Already delay-loaded graphics imports are excluded. |
| 21 | Consolidate redundant activation-factory lookups before first idle | Startup CPU: first establish actual pre-idle calls; metadata-provider calls currently appear after first idle, so that particular cache is not a medal candidate. |
| 22 | Reduce executable startup relocation/import work | Startup CPU/memory: inspect actual PE imports and map sizes; verify launch and UI behavior, then same first-idle protocol. |
| 23 | Use Windows process heap configuration with lower fragmentation | Full private memory/startup: isolated public executable allocation budgets and identical complete CSV; benchmark actual process resources. Requires platform documentation verification. |
| 24 | Reduce eagerly initialized nonessential COM state | Startup CPU/memory: prove pre-idle initialization and preserve apartment/thread requirements and accessible window readiness. |
| 25 | Optimize UTF-16 decoding scratch without losing surrogate paths | Full/dense variability: real NTFS Unicode and unpaired-surrogate fixtures, preserved metadata and cancellation; avoid changing lossy display semantics. |

Implementation is vertical: one observed failing public behavior/resource test, the smallest implementation, passing regression, then the next candidate. Reject hypotheses that have no real deficit, do not affect a remaining category, change required behavior or worsen the measured product. Each iteration concludes with all 35 categories measured again for all three products, ten accepted samples each, including fresh administrator evidence. Maintain raw logs and before/after binaries. Stop only at 35 gold, or record a concrete external blocker without claiming success.
