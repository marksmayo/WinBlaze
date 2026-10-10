# Performance iteration evidence

## Iteration 1: bounded directory metadata scratch reuse

RED: the public read_directory allocation-budget regression failed: 24 allocations of at least 64 KiB for 24 sequential owned-fixture walks.

GREEN: reuse one idle initialized buffer per worker; active iterators retain exclusive ownership and teardown tolerates unavailable thread-local storage. The budget passes with at most one allocation. Populated/empty directories, exact names/sizes, sparse/compressed allocation and UTF-16 path tests pass.

Workspace/example tests, strict Clippy and rustfmt pass. Rust line coverage is 92.03% against the unchanged 92% gate. The n=10 iteration-one batch completed, including 690 validated current WinBlaze observations. Full-volume workflow median: 18.836485 s, 114.882815 CPU-s, 657.447265625 MiB sampled peak working set. All three dated batches varied; no causal CPU/time saving is claimed for this allocation reduction.

## Report provenance regression

RED: a replaced ffi_scan_repro executable was still baseline_current=true because its new binary had not yet entered tool-version-history. GREEN: rendering checks actual executable and companion DLL hashes with a per-render cache, in addition to version history. The report excludes replaced binaries before reruns. The Windows prompt for iteration-one administrator refresh was canceled; older positive tests remain historical.

## Iteration 2: transferable catalog storage

RED: consuming the public transaction into records allocated another 2,228,224-byte record array for 16,384 files. GREEN: compact ID-to-slot bookkeeping plus an owned transferable record vector eliminates that allocation. Public tests preserve sparse IDs, replacement, swap-removal, repeated removal and insertion-order-independent equality. Existing snapshot formats and public interfaces are unchanged.

## Iteration 2: late capacity estimates

RED: applying an equal/lower total record estimate to a populated transaction allocated 11,862,032 bytes unnecessarily. GREEN: reserve only the estimated total minus the number already stored. All records remain intact. Workspace/example tests, formatting and strict Clippy pass; unchanged coverage gate passes at 92.10%. Full-volume memory/time effects await n=10 measurement.

## Controlled worker experiment instrumentation

RED: the engine example silently ignored an invalid explicit worker count of zero and returned success. GREEN: optional counts 1..64 are accepted and recorded; invalid counts exit 2. Owned-fixture checks for 1, 4 and 8 workers preserve one file/three bytes and successful completion. Default worker selection and timing scope remain unchanged. The separate experiment runner rotates 4/8/20-worker order over ten full-volume repetitions each and records aggregate CPU, elapsed time and memory. Experimental rows do not enter medals.

## Iteration 3: bounded directory-worker default

Controlled full-drive experiment, n=10 elapsed/memory for each setting:

| Workers | Process median | CPU median | Working-set median |
|---:|---:|---:|---:|
| 4 | 22.3818 s | 81.3984 CPU-s (n=10) | 52.8848 MiB |
| 8 | 18.9173 s | 103.4844 CPU-s (n=9) | 56.0605 MiB |
| 20 | 17.8384 s | 114.0000 CPU-s (n=10) | 60.2598 MiB |

One eight-worker CPU exit-race observation is missing, not zero usage. Four versus twenty workers reduces CPU 28.6% and working set 12.2%, with 25.5% slower engine elapsed time. This is an explicit trade-off, not a speed improvement. The whole-workflow n=10 comparison determines whether it remains appropriate.

RED: public ScanController owned-fixture resource test allocated 20 initialized worker scratch buffers against a four-buffer default budget. GREEN: separate directory_walk_max_parallelism default of four, bounded by max_parallelism. Raw MFT concurrency retains the original maximum; explicit directory overrides remain available. All 256 files, 257 directories and 768 logical bytes remain exact. Existing 1/20-worker hard-link reference tests explicitly select their directory concurrency and pass.

## Iteration 3: discard dot records before name allocation

RED: public empty-directory iteration allocated two discarded entry names. GREEN: validate and advance structural dot records without constructing names; zero iteration allocations. Populated/empty metadata and sparse/compressed, hard-link, UTF-16 and scan-rule checks pass.

## CPU telemetry regression

RED: cumulative observations 0, 15.625, 31.25, 0 ms ended at zero. GREEN: preserve the greatest observed cumulative CPU value, retaining 31.25 ms after the exit race. Shared helper applies to whole workflows, engine, feature, elevated and GUI samplers. This accuracy correction is not a product performance win.

Iteration-three workspace/example tests, strict Clippy and formatting pass. Coverage remains 92.10% against the unchanged 92% gate. Release export/filter and current import/search GUI checks pass, as do invalid-root checks with previous results retained. The older import-scan.ps1 targets a removed menu label; current parity-import-search.ps1 verifies the shipped import UI. Release native DLL identity matches the DLL beside the UI executable. The cross-product n=10 batch and supplemental checks completed: 690 current WinBlaze observations validated, 490 feature measurements passed, and the report shows 26 gold/4 silver/2 bronze over 32 eligible events. Three administrator categories await current-build reruns. The all-gold acceptance test remains RED.

## Iteration 4: ordered catalog construction

RED: building a public TreeIndex from 16,384 already ordered file records allocated 262,144 bytes of volume-sized temporary storage. GREEN: check existing ID order before allocating a permutation array. The public budget now records zero large temporary bytes while verifying every file ID, logical/allocated size and root rollup. All Index tests pass, including unordered and empty trees, snapshot round trips, import/export and search behavior. Public interfaces and snapshot formats are unchanged. The extra linear order check is allocation-free; unordered input still uses the existing permutation algorithm. Whole-workflow CPU, peak-memory and runtime effects await n=10 measurement; this is not yet a medal improvement claim.

## Iteration 5: directory storage handoff (validation pending)

RED: publishing 16,384 directory records through BufferedIndexTransaction allocated 1,703,936 bytes for a second record array. Every directory ID and direct-byte value survived publication. The implementation now keeps compact ID-to-position bookkeeping alongside owned records and transfers the record vector. The GREEN check is queued behind the active benchmark measurement lock; it has not yet run. No runtime, memory, or medal improvement is claimed before validation and n=10 measurement.

Directory handoff GREEN: zero large temporary bytes, with public sparse IDs including zero and u64::MAX, replacement, cloning and insertion-independent equality checked. Complete Index suite passes. The first GREEN attempt found a missing lookup method at compile time; using the compact lookup directly resolved it. The successful allocation test is recorded in all-gold-directory-handoff-green.log.

## Iteration 5: dense shuffled catalog construction

RED: a public TreeIndex built from 16,384 shuffled dense file IDs allocated 262,144 bytes for temporary keys. GREEN: validate the dense ID range and apply its permutation in place; zero large temporary bytes. Exact IDs, logical/allocated values and root rollups are checked. Separate public regressions cover zero-based ranges, ranges ending at u64::MAX, sparse overflow-spanning IDs, replacements and empty input. Sparse IDs use the existing general sort. A duplicate guard preserves termination. Whole-workflow effects await n=10 measurements.

## Next iteration: compact dense-ID lookup (GREEN pending)

Public reserve-plus-fill regression for 16,384 shuffled dense IDs: RED 2,785,296 large-allocation bytes against a record-plus-lookup budget of 2,293,764 bytes. Every ID and logical size survives. FileRecords now has a bounded dense u32 slot lookup with zero reserved for holes, plus the original sparse-ID map fallback. The fallback retains the requested reserve hint. Replacement, removal/swap bookkeeping, cloning, equality and persistence must remain exact. GREEN session30056 is queued under the measurement mutex; this source change has not been validated or Release-built. No whole-process saving is claimed.

Diagnostic module inventory completed n=10 per product. All 30 run numbers, nonempty module sets and snapshot timestamp ordering validate. WinBlaze's Rust DLL is absent in all ten post-idle snapshots. Module enumeration begins after the idle return and completes later, so it does not locate exact first-idle loader boundaries. Mapped image sizes are not resident-memory attribution. The separate phase-timestamp RED first hit MSBuild's PATH/Path environment duplication; a Python-normalized retry is queued. No successful phase RED/GREEN claim yet.

## Iterations 6–7: compact lookup and publication capacity

Compact lookup GREEN passes the public allocation budget, exact values, removals, dense-to-sparse transition, cloning and insertion-independent equality. Publication capacity RED retained 32,768 slots for 20,000 exact files; GREEN shrinks unused capacity after dropping lookup storage. All six public catalog budget regressions pass. Workspace/examples, strict Clippy and rustfmt pass; unchanged coverage gate reports 92.17%. No whole-process performance improvement is claimed before n=10 results.

Startup phase tracing was separately red/green tested with disabled tracing producing zero writes. Ten diagnostic startup runs locate first idle before the MainWindow constructor body; custom shell construction occurs later. Ten-run module inventory capture-delay medians are WinBlaze 9.452 ms, WinDirStat 9.695 ms and WizTree 10.070 ms. These diagnostics do not change medal measurements.

## Report direction accuracy

Extracted the existing percentage wording without changing behavior. Public formatter regression RED: -40.5% lower observed time for a slowdown; GREEN: 40.5% higher observed time. Positive12.34 still formats12.3% lower. Summarizer imports the formatter; node syntax check passes. Measurement values, medal definitions and app binaries unchanged. This is a reporting correction, not a product performance win.

## Terminal-only unattended delivery — RED proven, GREEN pending

Public isolated child-process scan/export test RED: complete CSV contains all three files, but callback sequence includes SessionStarted, DirectoryBatch, Progress, FileFound, ExtensionStats and Summary alongside Completed. Log: artifacts/all-gold-terminal-delivery-red.log. Added private Live/Terminal delivery mode on the same scanner, event log, transaction, model publication and persistence path; terminal mode avoids constructing UiEventForwarder and converts only terminal events. Existing GUI entry points retain Live. Shipped CLI now selects additive wb_scan_session_start_terminal; public C header declares it. Source implementation is not validated or Release-built yet; protected GREEN session94867 queues terminal_delivery and shipped CLI regressions behind supplemental measurements13415. No runtime saving claimed.

Terminal-only GREEN: public complete-catalog and exactly-one-Completed callback regression passes. Shipped CLI scans/CSV, duplicates and permissions exports and invalid-command behavior pass. Log all-gold-terminal-delivery-green.log. Source formatted; full workspace/examples/Clippy/coverage session74569 now running. This eleventh product change is not Release-built or benchmarked yet.

## Directory publication capacity

Public RED: 20,000 exact directory records retained32,768 slots after publication. GREEN: drop private ID lookup before shrinking unused record capacity, preserving all IDs/direct_bytes. All three directory handoff regressions pass, including no-second-array allocation budget and sparse/replacement/clone/equality behavior. This twelfth product change is not Release-built or rebenchmarked. Shrinking may move storage; whole-process peak and CPU/time must be checked at n=10.

Terminal delivery broader gate passed workspace/examples/strictClippy/coverage92.17%. Additional fresh-process regression removes source fixture before reopening persisted catalog and verifies exact three filenames/sizes; passes. This proves persistence remains complete after session destruction.

## Original35categorycompletionguard

Located archivedbaseline benchmarks/runs/2026-10-04T11-07-01-169Z/report-data.json predatinggoalcreation, with25gold6silver4bronze over35events. Snapshotnames/units/scopes/comparability in original-medal-events.json. REDcount-onlyguard acceptsrenamedevent; GREENrejects. NextREDacceptschangedunit; GREENchecksunit,scope,comparable/observationalflags. Missing/duplicate/scopechangedcasesreject; displayreorderingaccepted. Completionverifiernowrequiresoriginaldefinitionsbeforeallgoldassertion. Noeventdefinition/measurement/medalchanges. Baseline’sadmincomparatorshadn3; futurecurrentrerunsremainrequiredn10.

Iteration 3 traversal diagnostic completed twenty complete full scans/CSV exports in alternating AB/BA order. Source collections verified by artifacts/all-gold-loop/iteration-3/finalize-traversal.mjs. Control median 35,574.715 ms / 118,023.440 CPU-ms; candidate 32,654.540 ms / 110,554.685 CPU-ms (approximately 8.2% lower wall, 6.3% lower sampled CPU). Median private bytes 504,170,496 -> 499,777,536. All observations retained, including first-run and later outliers; uncontrolled full-drive fixture changes remain explicit. These diagnostic values do not enter official medals. Resource RED 2,330,715 transient bytes -> GREEN 421,160 before strengthening exact hierarchy assertions. Retain the scheduler change pending full workspace/release checks and original all35/all3 rerun.

Iteration 5: public standalone-progress delivery/allocation regression RED at 1,114,112 bytes for sixteen default-config events; GREEN with <=16,384-byte bound after reserving one slot for standalone immediately flushed control events. Existing queued data arena/order is preserved. Workspace and examples tests and strict Clippy passed. Coverage/build and fresh all-35 medals pending; no medal improvement claimed.

Iteration5 gates: workspace/examples/strictClippy/fmt PASS; linecoverage92.41% PASS. Release native and directory_walk_benchmark rebuild PASS, primary UI Release build PASS; copied native DLL identical SHA256 DB7E5BDBFE487A24899B94C2EE9D549B6EEEB3F73BB67E9F56100315D9E3D306. Owned public UI export/import36221 active. Original medal thresholds unchanged; latest official27G6S2B.

Iteration5 final evidence RED: Quick folder peak-memory WinBlaze6/10 actualpositiveobservations (four unavailable; neverzero-scores), verify-medal-rerun rejected. Collector now samples immediatelybeforefirstwait whilekeepingperiodic25ms; separatepublicall3tiny n10 collector-check.json GREEN30positiveobservations, existingmemory/CPUexit-racechecksPASS. Nativeexamples rebuiltsoffi_scan_repro containscurrentpipeline. No definitions/thresholds/samples weakened. Fullfreshiteration6recollectionrequired; priorrejectedrawarchivespreserved.

Iteration7 completewidehierarchyresourceRED229781bytesvs192KiB; naive8KiBRED287587revealedtoo-eagercontinuationgrowth; refined8KiBinitial/32KiBaftersecondsuccessfulcontinuationGREEN191755bytes. All10101directorycounts,10000uniqueleafnamesandparentsverified. Longname/dense/identity/cancel/fullworkspacegatesandcontrolledcompletecomparisonpending. Originalmedaldefinitionsunchanged; noadoptionorallgoldclaim.

Iteration7 final20comparison: controlmedian37672.1msCPU131867.185/private503289856; candidatem38244.315msCPU131312.5/private502280192. All20completeobservations retained, nofavorableexclusion. Resourcefixturegain185468vs229781didnotestablishwall/CPUbenefit; rejectcandidate. Experimental192KiBtest/sourcearchivedrejected-source; productionrestoredoriginal256KiBexistinggateand16KiB/32KiBenumeration. Pipelinecontrol-eventimprovementremains. No medalclaim.
