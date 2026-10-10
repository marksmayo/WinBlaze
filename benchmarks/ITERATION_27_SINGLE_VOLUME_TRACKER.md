# Single-volume identity tracking with adaptive promotion

The completed prior Release cohort has 31 gold and four timing-variability
silvers, all35 fresh WinBlaze n=10, unchanged definitions and all70 retained
competitor metric records/values unchanged. See iteration26/final-validation.json.
Iteration27 is now staged separately and its fresh original35 refresh is running.

## Resource RED/GREEN and fidelity

The public concurrent tracker's retained-storage test fails against the previous
implementation: 34,816 bytes for64 unique single-volume claims, including inline
state. Its unchanged16KiB ceiling passes against the final candidate at11,520
bytes, about67% lower. The budget includes shard alignment and directly owned
sets; moving heap allocations onto the stack cannot alone satisfy it.

The tracker keeps its first volume's narrow/wide identity sets directly owned.
On a second volume it promotes all existing sets into the original map-based
lookup path. Zero-byte claims create no identity state. The full128-bit file
identity and volume remain distinct. Aligned shards separate neighboring mutable
state; their storage overhead is included in the resource test.

Tests verify duplicate claims before/after promotion, mixed volumes, wide IDs
sharing low bits with narrow IDs, zero-byte then nonzero claims, simultaneous
hardlink aliases, and eight workers interleaving narrow/wide IDs on two volumes.
Workspace tests and example targets pass. rustfmt corrected the new match-arm
layout; formatting and strict workspace Clippy then pass. Final-candidate line
coverage passes at92.48% against the unchanged92% requirement. The unchanged
Release UI resource budget also passes; that budget was measured on the completed
iteration26 host/native pair before the subsequent iteration27 rebuild.

Evidence under artifacts/all-gold-loop/iteration-27:

- tracker-budget-red.log: original34,816-byte failure.
- promoted-tracker-memory.log: actual11,520-byte GREEN.
- promoted-tracker-green.log: workspace/examples pass, followed by the recorded
  formatting failure; formatting/Clippy were subsequently corrected and passed.
- promoted-coverage-and-ui-budget.log: final-candidate coverage/UI gate.

## Paired cost diagnostics

A standalone optimized Rust diagnostic includes retained pre-change logic and
the candidate in one executable. Every scenario has20 observations per
implementation, alternating ABBA order,500,000 unique identities per trial,
64-/128-bit identity mixtures, one/four volumes and one/four workers.
Every trial verifies total physical allocation and repeated-alias deduplication.
All160 observations are retained. Timers include tracker allocation, claims,
worker startup and destruction. This is synthetic diagnosis, not medal evidence.

| Volumes | Workers | Prior median ms | Final median ms | Change |
|---:|---:|---:|---:|---:|
| 1 | 1 | 41.31725 | 34.11340 | -17.44% |
| 1 | 4 | 21.19750 | 15.28125 | -27.91% |
| 4 | 1 | 40.75035 | 39.19305 | -3.82% |
| 4 | 4 | 19.76640 | 19.98190 | +1.09% |

The first cached-volume candidate regressed the four-volume/four-worker case
by11.22%; alignment alone still regressed it9.52%. Those observations and
candidate sources are archived. Adaptive promotion brings that diagnostic close
to baseline, but its final median is still1.09% slower; no universal speedup is
claimed. Final single-volume/four-worker claims win all20 paired observations.
This does not establish an end-to-end C: workflow or variability improvement.

Initial/aligned/final summaries and immutable observation copies preserve these
comparisons. The benchmark reference reconstructs the captured pre-change tracker
logic; it does not execute the old shipping DLL. The candidate snapshot used by
the diagnostic differs from the final repository source only by rustfmt layout.
Release gates passed and the candidate was staged at 2026-10-08T09:54Z:
UI ADD07FBD, native 32FB8F72, CLI BCCE0057. The previous four binaries are
preserved in artifacts/ui-stage-backup-d026e80c76494305a86d5babc11c4211.
Fresh original35 n10 observations are running in refresh-20261008T095554026Z;
no new medal improvement is claimed yet. Comparator bests remain frozen.
The backend-only refresh includes all medal workflows, startup, engine and
400 feature observations, plus current UI readiness. Existing supplemental
GUI/cache/incremental rows are retained with their actual previous identities;
the full Release build gate validates current focus/window/PNG behavior.

## Completed refresh

The same batch completed at 2026-10-08T10:05:16Z after retained administrator
worker request iteration-28. All four report validators pass. All35 rows remain
visible and all70 competitor best objects and official values match the frozen
baseline exactly. Current standings are32 gold,3 silver,0 bronze.

Full-C: variability is now3.517267% (retained target5.230405%), while quick,
empty-file and dense variability remain silver at7.622565%,13.299442% and
10.873029%. All slower observations remain included. The change has not proved
a causal reduction in process variability from a single cohort.

Fresh qualifying coverage is34/35. Startup CPU run4 recorded0ms and is rejected
by the unchanged positive-value rule; the prior official value remains visibly
pending. No sample substitution or favorable startup rerun was performed.
The other startup values are14.5625MiB working set/peak and37.355ms idle, allgold.
All400 feature observations passed, including30 valid PNGs with normal exits;
current UI readiness, index-memory and public-operation checks completed.
Supplemental broad GUI/cache/incremental observations retain previous identities.

Evidence: final-validation.json and finalize-refresh.log under iteration27;
final aggregate benchmarks/runs/2026-10-08T10-05-16-883Z/report-data.json.

## Quick-workload phase diagnosis

After final publication, twenty alternating baseline/candidate observations
validated the exact72-file fixture. Every observation, including the initial
239ms baseline outlier, is retained under iteration27/quick-phases. The direct
Process.Start diagnostic differs from the official PowerShell launch protocol;
it never replaces medal evidence. Median wall time was40.216ms baseline and
37.540ms candidate; internal measured phases totaled9.379ms and9.614ms.
Candidate scan-to-terminal, finalization and CSV export medians were3.639ms,
2.558ms and3.006ms. Much of wall time lies outside those internal phases, so
buffer recycling alone cannot be assumed to solve quick-folder variability.
