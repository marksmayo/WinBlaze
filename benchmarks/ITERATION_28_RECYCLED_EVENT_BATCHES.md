# Bounded reuse of consumed scan-event storage

The completed iteration27 report has32 gold,3 silver and0 bronze across the
unchanged35 categories. Quick/empty-file/dense variability remain silver;
full-C: variability is gold at3.517267%. Startup CPU has only9 positive fresh
observations and retains its previous official value with a pending label.
All70 competitor best records and official values are unchanged.

The ranked next25 are in ITERATION_27_VARIABILITY_PLAN.md, drawn from the
existing100 hypotheses. This iteration implements candidate1, bounded recycling
of consumed event vectors. Paired quick-workload diagnostics show most wall
time lies outside the measured scan/finalize/export phases, so this allocation
change is not assumed to solve all remaining variability.

## RED/GREEN

A public controller/receiver resource test scans2048 real files with32-event
batches and validates every file ID, parent relationship, terminal ordering,
file size and summary totals. Counting successful batch-sized allocations
across all scan threads gives65 allocations before recycling, failing the
unchanged32-allocation ceiling. The same test passes with3 allocations after
recycling. Each arena is3584 bytes. These are allocator observations, not
process-speed or medal results.

Evidence: artifacts/all-gold-loop/iteration-28/resource-red.log and
resource-green.log. The RED is a compiled behavioral/resource failure, not a
missing-method or compile failure. The initial receiver adapter used the
existing bounded sender and discarded batches normally.

The existing raw bounded/unbounded constructors remain available. The new
receiver consumes records through for_each_event and recycles each fully drained
batch. Producer acquisition remains lazy; idle workers reserve no event arena.
The pool retains at most4 empty vectors of at most64KiB each, plus metadata.
Oversized batches are released normally. Larger data arenas can displace tiny
control-only arenas so an initial control sequence cannot defeat later reuse.
The pool holds no channel sender, preserving disconnect and join behavior.

Native scan sessions now use this receiver with the same16-batch channel limit.
Callbacks, persistence, event order, completion, progress and record contents
retain their prior behavior. Old raw bounded callers remain unchanged.
Public tests cover rendezvous backpressure, exact completion, and a consumer
panic releasing cancelled blocked producers. An oversized1024-event scan
delivers all2048 files and retains34,120 additional Rust heap bytes at terminal
delivery, below64KiB; it does not retain its114,688-byte event arenas.
Workspace and example tests passed. The first strict-lint run found one unused
import after the native receiver change; it was removed. Final formatting,
strict Clippy and focused regressions pass. Coverage runs the complete workspace
again and passes92.40% against the unchanged92% requirement.

Evidence: gates.log preserves the initial lint failure; final-gates.log records
the corrected checks and resource observations. The full Release build/host
gate passed, including icons/window/focus and all three PNG modes with normal
exits. The candidate is staged as UI6D5ACFDE/native68EA02B6/CLI D576E8F1;
updater66C6 is unchanged. Prior binaries remain in
artifacts/ui-stage-backup-c3d3258ea3504fdb833bd418c1ecd9a0 and
artifacts/all-gold-loop/iteration-28/pre-change-release.
Fresh original35 n10 refresh-20261008T101753006Z is running with comparator
bests retained. All35 rows remain visible. Process/medal results are pending.

## Completed process validation

That cohort completed at10:27:13Z using retained worker request iteration29.
All four report validators pass. All35 rows remain visible and all70 competitor
best objects and official values match the frozen baseline exactly. Current
standings remain32 gold,3 silver,0 bronze, with34/35 fresh qualifying categories.
Startup CPU run2 recorded0ms and was rejected by the unchanged positive-value
rule; its prior official result remains labelled pending. No sample replacement
or favorable startup retry was performed.

Latest variability: quick10.166261%, empty-file8.014005%, dense9.806043%,
full-C:3.706530%. The first three remain silver; full-C: remains gold against
5.230405%. All observations, including slower values, remain included.
All400 feature observations passed, including30 PNG exports; UI readiness,
index-memory, public operations and ten administrator observations completed.
Final evidence: iteration28/final-validation.json and
benchmarks/runs/2026-10-08T10-27-13-517Z/report-data.json.

Two subsequent paired diagnostic cohorts are retained separately under
iteration28/quick-phases and quick-process-lifetime. They do not replace medal
observations. The first cohort's candidate internal median is9.2145ms versus
9.153ms baseline, with wall medians34.4119ms versus34.00425ms: no quick-workload
speedup is established. The second cohort uses phase-file write time only as
an approximate boundary. Candidate median pre-internal proxy is16.3851ms and
post-phase-file interval1.46705ms; baseline16.15355ms and1.8801ms. This points
the next diagnostic toward startup before the internal scan phases. Both
cohorts preserve all20 observations and validate the complete72-file fixture.
