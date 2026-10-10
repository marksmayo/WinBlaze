# Silver medal performance analysis - 9 October 2026

The completed iteration32 report has 29 gold, four silver and two bronze medals. All 35 categories have current qualifying WinBlaze evidence; all 70 retained competitor best records and official values remain unchanged. This analysis addresses the four silvers. It does not change the official report or adopt the unstaged iteration33 allocation candidate.

## What actually needs improving

| Silver category | Current WinBlaze | Retained competitor best | Required improvement |
|---|---:|---:|---|
| Startup working set at first idle | 16.7890625 MiB | WinDirStat 16.244140625 MiB | More than 558 KiB saved, about 3.25% of current memory |
| Startup peak working set at first idle | 16.7890625 MiB | WinDirStat 16.244140625 MiB | Same observed memory values and same gap |
| Empty-file scale timing variability | 11.1111111% | WizTree 4.0145112% | At the present 62.055 ms median, IQR below 2.4912 ms instead of 6.895 ms |
| Nonempty dense folder timing variability | 6.0640147% | WizTree 5.3978661% | At the present 48.895 ms median, IQR below 2.6393 ms instead of 2.965 ms |

The two startup categories currently have identical ten observations, so they describe one memory problem. Variability is the unchanged R7 interquartile range divided by the median. Its target moves when the median changes: a faster median with unchanged spread worsens the percentage. The largest observation is not itself the IQR; discarding a slow run would neither be valid nor diagnose the middle-half spread.

Startup memory observations range from 15.7383 to 20.0508 MiB. A separate twenty-pair old/current diagnostic found medians of 15.9297 and 15.9453 MiB. That does not establish that the focus repair caused the official memory increase. Those diagnostic observations cannot replace the current official cohort.

## Additional phase evidence

I collected ten diagnostic executions of each affected scan fixture using the shipped iteration32 CLI, SHA256 `3A9B6172B7AB67F34F637DD2758EC419EF8030BF8B8E3E112AC1253F63BC8109`. They used the existing optional CLI phase instrumentation, complete scan/index/persistence/CSV behavior, normal exit0 and verified complete CSV row counts. No competitor was launched and no official metric was replaced. These executions are diagnostic, with different observation conditions from the official cohort.

| Diagnostic phase | Empty-file scale median / IQR | Dense-folder median / IQR |
|---|---:|---:|
| Session setup | 0.107 ms / not used for ranking | 0.101 ms / not used for ranking |
| Scan plus indexing | 35.879 / 4.700 ms | 5.225 / 1.188 ms |
| Finalization tail after terminal callback | 2.310 / 0.092 ms | 1.518 / 0.194 ms |
| Complete CSV export | 6.882 / 0.745 ms | 5.515 / 0.774 ms |
| Whole diagnostic process | 71.489 / 7.169 ms | 41.040 / 3.676 ms |

Phase medians and IQRs cannot be added to reproduce a process median or IQR. Persistence can already run before terminal notification, so the finalization tail is not its complete CPU cost. Process time outside the instrumented CLI body includes launch, shutdown and measurement overhead; it cannot be attributed to a particular DLL without an execution trace. All observations, including the empty-workload 241.409 ms process observation and dense-workload 11.475 ms finalization tail, are retained.

The empty dataset contains 16,384 zero-byte files and 545 directories. Dense contains 8,192 nonempty files in two leaf directories plus the root. The empty scan/index phase has appreciably more spread than export; dense export takes about as much time as scanning/indexing. That supports prioritizing allocation and shared counters for empty files, and export plus worker scheduling for dense folders.

## Ranked ten improvements

The order reflects expected practical benefit toward these silver medals, considering the amount of removable work and confidence. Memory and milliseconds are different units; this is not a numerical sum across metrics. Ranges below are engineering planning estimates for a successful implementation, not measured improvements or confidence intervals. Actual benefit can be zero or negative. There is no defensible conversion from a CPU or elapsed saving to an IQR reduction, and overlapping estimates must not be added.

### 1. Construct initially hidden UI panels when the user opens them

**Target:** both startup memory silvers. **Planning potential:** approximately 0.5-1.5 MiB, with medium confidence in the opportunity and low confidence in its size.

`MainWindow.cpp:1544`, `:1916` and `:2117` construct the search, detail and runtime diagnostic cards. Defaults at `MainWindow.xaml.h:473-480` hide these panels; `ApplyViewVisibility` at `MainWindow.cpp:7237` applies those defaults. Hidden search includes numerous text boxes and combo boxes; diagnostics includes multiple wrapped text controls. Avoid activating those complete control subtrees until requested, while retaining their model state, navigation and accessibility behavior. All initially visible controls must remain ready under the unchanged startup protocol.

This has the clearest chance of saving the required 558 KiB. Measure per-panel effects rather than assuming the XAML runtime will release or avoid every byte. The five collapsed sidebar hosts are also eligible, but their contents are already built on navigation: only fifteen host controls are currently eager, so they should not be credited as five entire pages of savings.

**Red/green protection:** every hidden panel opens with the same filters, metrics, selection and focus behavior; startup visible-shell and accessibility tests remain unchanged. Measure first-open cost as well as startup memory.

### 2. Store short file names inline and decode into that storage

**Target:** empty-file variability, with a smaller dense-folder benefit. **Planning potential:** 1-4 ms for empty files; 0.2-1 ms for dense. **Confidence:** medium.

`directory_metadata.rs:174` constructs an `OsString` from the metadata's UTF-16 slice. `controller.rs:660` obtains the display string, and `:1130-1141` transfers it into `FileRecord`'s `Box<str>`. Both fixtures use ordinary 21-byte ASCII names such as `file-000-000-0000.bin`. A compact name type with approximately 23 inline bytes can eliminate a per-file heap allocation when metadata decoding fills that final inline storage directly. Long names retain an owned heap fallback; path filtering, directory descent, reparse behavior and reopening must preserve original on-disk UTF-16 semantics.

This targets allocator traffic for 16,384 names in the phase with the largest observed empty-file spread. It requires both inline retained storage and a metadata path that avoids creating an intermediate heap-backed name: merely copying an existing `OsString` into inline storage would still pay its allocation. Rust's [Windows string implementation](https://doc.rust-lang.org/src/std/os/windows/ffi.rs.html) uses WTF-8 internally; valid conversion into `String` must not be assumed to make another copy. The opportunity is eliminating the owned name allocation, not an unsupported claim of duplicate UTF-16/UTF-8 buffers. A larger inline record also changes stride/cache density, so verify whole-catalog memory and throughput before adoption.

**Red/green protection:** identical names and CSV bytes for ASCII, non-ASCII, surrogate pairs, unpaired surrogates and long names; unchanged filtering and accounting; demonstrate fewer transient allocations without increased retained catalog storage.

### 3. Encode CSV rows with reusable scratch storage and cached escaped parent prefixes

**Target:** both variability silvers, especially dense. **Planning potential:** 0.8-2 ms per process. **Confidence:** medium.

`export.rs:24` repeatedly looks up and escapes each parent path; `:259-312` emits a row through multiple write calls and generic integer formatting. Dense files have only two parent paths. Cache a bounded set of escaped parent prefixes and encode each row into a reusable buffer with direct integer formatting. Preserve the complete catalog, exact quoting, CRLF, timestamps and byte counts.

The measured export phase is 6.882 ms for empty and 5.515 ms for dense, so this has a larger dense opportunity than a broad claim about scanner optimization. Output is already buffered and file paths already stream without a per-file full-path string allocation; those are not new improvements.

**Red/green protection:** byte-identical CSV/TSV output, complete row counts, quote/comma/newline/Unicode fixtures, failing-writer propagation and bounded scratch memory. Separately measure encoding and file output to avoid blaming filesystem noise on formatting.

### 4. Accumulate scan totals per worker and publish them in batches

**Target:** empty-file variability and dense scanner cost. **Planning potential:** 0.5-2 ms for empty; 0.1-0.7 ms for dense. **Confidence:** medium.

`controller.rs:1112-1123` updates shared atomic counters for every file. Empty files skip the two byte-total increments but still increment file count and allocate an ID: at least 32,768 shared atomic operations. Dense nonzero files can update four counters per file. Worker-local count and byte totals, flushed at bounded batch boundaries and on every terminal path, reduce cache-line contention without losing exact totals.

**Red/green protection:** exact completed and cancelled counts, progress monotonicity with bounded lag, no lost totals across errors, and unchanged physical-byte deduplication. Do not postpone all reporting until completion.

### 5. Choose worker count from actual available directory branches

**Target:** dense-folder variability. **Planning potential:** 0.2-1 ms, primarily less scheduling and synchronization. **Confidence:** medium-low.

`controller.rs:896-905` starts the configured worker set after branching. The default is already capped at four, not twenty. Dense has only two leaf branches; two workers can do useful enumeration while the others add startup/wait/teardown. Limit initial parallelism to available independent work and support expansion when a larger tree reveals more branches. Test four versus two on the empty tree rather than assuming fewer is always better.

**Red/green protection:** the same records, cancellation behavior and liveness on two-leaf, deep-chain, wide and skewed trees. Verify wide-tree throughput does not regress; pure leaves and single chains already avoid extra workers.

### 6. Reserve file ID ranges per worker instead of one atomic ID increment per file

**Target:** mostly empty-file variability. **Planning potential:** 0.2-0.8 ms for empty; 0.1-0.4 ms for dense. **Confidence:** medium-low.

`controller.rs:1124` allocates each ID through a shared atomic. Reserve small ranges per worker and assign within them locally. This complements local totals but is a separate correctness problem.

The existing index ID-ordering fast path requires a dense range (`tree.rs:576`). Unused block slots can accidentally defeat it and increase sorting or lookup storage, cancelling the intended gain. Bound or eliminate holes, and evaluate the whole index/export path rather than just the allocator loop.

**Red/green protection:** unique IDs and valid parents under concurrency/cancellation, sparse-range limits, unchanged rollups, deterministic export membership and no regression in dense ID permutation handling.

### 7. Grow metadata enumeration batches further for demonstrably dense directories

**Target:** dense-folder variability. **Planning potential:** 0.1-0.7 ms. **Confidence:** low-medium.

`directory_metadata.rs:84-91` requests 16 KiB initially, while `:136-141` grows continuation storage to 32 KiB. A larger bounded continuation batch, for example 64/128 KiB only after density is established, can reduce kernel enumeration calls for the two 4,096-file leaves. Retain the small first batch for the 545-directory empty fixture.

This is a hypothesis about fewer calls, not an assertion that larger buffers always help. Establish enumeration-call counts and memory tradeoffs; don't multiply buffer sizes across every small directory.

**Red/green protection:** identical records and allocation identities across refill boundaries, truncated/error responses handled safely, long names and unsupported metadata fallback retained, and bounded per-worker cached memory.

### 8. Apply startup shell state once and avoid rebuilding unchanged diagnostic strings

**Target:** both startup memory silvers and startup processing. **Planning potential:** 0.5-2 ms startup work; 0-0.2 MiB retained memory. **Confidence:** medium for redundant work, low for retained savings.

`MainWindow.cpp:769-787` refreshes an empty search and repeatedly updates shell state, summary and runtime snapshot. `ApplyShellState:4927` itself updates summary and runtime; `UpdateRuntimeSnapshot:5311` rebuilds a long string and refreshes correctness/issues. Initialize backing state first, then perform one complete control update. Cache or skip text assignments when the value is unchanged.

This is smaller than removing hidden controls: temporary strings may already have been freed by first idle, so eliminating them does not guarantee a working-set medal.

**Red/green protection:** identical Ready/Idle/progress/empty-state controls by the original first-idle boundary, no delayed required initialization, unchanged focus and update behavior after real scans.

### 9. Let the terminal CLI consume/index scan events on its existing calling thread

**Target:** both variability workloads. **Planning potential:** 0.1-0.6 ms plus reduced thread resources. **Confidence:** low-medium.

`bridge.rs:1015-1060` creates an asynchronous producer and a forwarding/indexing thread, while `main.rs:69-73` waits for a terminal channel message. A supported synchronous terminal execution seam can let the CLI calling thread do the consuming/indexing, leaving the GUI's asynchronous ABI intact. This removes one thread and its terminal handoff while keeping bounded backpressure, persistence and complete export.

The measured session setup is only about 0.1 ms. Therefore the plausible benefit is small, and a claim of several milliseconds from thread creation alone would exceed the evidence. CLI polling was already removed.

**Red/green protection:** success/error/timeout/cancellation contracts, complete CSV/cache results, callback and destroy lifetime safety, and unchanged asynchronous GUI behavior. Validate CLI terminal exit codes and packaged artifacts.

### 10. Delay child display ordering until an ordered child query needs it

**Target:** empty/dense scan/index processing. **Planning potential:** 0.05-0.3 ms for these fixtures. **Confidence:** low; profile before investing.

`tree.rs:257` eagerly calls `sort_children`, whose implementation at `:334` orders children by physical allocation. Ordinary CSV iterates the ID-ordered record slices, not this display ordering. A thread-safe lazy ordered view can avoid work in terminal-only workflows while preserving the public largest-first child-query contract.

Both affected fixtures have largely uniform allocation values. The actual comparator has no filename tie-break, so modern equal-key sorting can already be very cheap. This is intentionally last: do not promise O(n log n) savings or a major medal change for these datasets.

**Red/green protection:** identical child-query ordering contract, concurrent first-query safety, unchanged rollups/export/cache reopening, and no repeated ordering allocation on subsequent queries.

## What should not be credited again

Already implemented: four-worker walker cap, leaf/chain traversal without idle workers, recycled bounded event arenas, streamed CSV paths and buffered output, compact CSR child storage, sorted-ID fast paths, terminal-only event delivery, deferred native/GPU initialization, cached theme brushes and single completed snapshot persistence. All control fields inspected already initialize to null; there is no evidence of accidental eager construction followed by replacement.

The iteration33 clustered-identity candidate targets large identity sets. These fixtures stay below the per-shard promotion threshold, and empty allocation identities are skipped. Its large synthetic memory reduction should not be presented as a solution to these four silvers. Similarly, the existing focus repair has no measured causal startup memory regression in the paired diagnostic.

The dense sampled CPU metric includes five zero observations. Sampling and clock quantization make it unsuitable for precise CPU attribution at this scale. Use allocation counts, phase timings and, if necessary, a CPU trace; retain the official unchanged sampling protocol for medals.

## Recommended sequence and acceptance

Start with hidden UI construction because one successful memory change could win two medals. Prioritize inline file names and local totals for empty files, and CSV encoding plus branch-aware scheduling for dense. The most informative next evidence is per-panel memory attribution and allocation/kernel-call counts.

Implement each chosen change through a meaningful failing behavior/resource test and a passing implementation, then run its relevant correctness/resource gates. Use all observations in a predeclared diagnostic comparison. Adopt only supported shipped artifacts; finally rerun the original qualifying WinBlaze tests and update the existing 35-row report. Continue retaining every lower-is-better competitor best for its version/hash. Neither these estimates nor a lower median alone establishes a gold variability medal.

Evidence: `CURRENT_COMPARISON.md`, `runs/2026-10-08T23-43-20-247Z/report-data.json`, `artifacts/all-gold-loop/iteration-32/final-validation.json`, `artifacts/all-gold-loop/iteration-33/startup-attribution-summary.json`, and the new `artifacts/all-gold-loop/iteration-33/silver-phases/{observations,summary}.json` with `silver-phase-analysis.ps1`.
