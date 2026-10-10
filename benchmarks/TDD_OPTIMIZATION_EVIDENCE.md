# Optimization TDD evidence

Each production change followed a public-operation resource or correctness regression: the test failed against the previous implementation, the implementation changed, and the test passed. Budgets count cumulative allocation requests on the measuring thread; they are not process RSS or end-to-end speed guarantees. Fixture setup is excluded.

| Change | RED observation | GREEN requirement |
|---|---:|---:|
| In-place tree ordering, 20k files | 6,000,716 allocated bytes | < 4,000,000 bytes, exact ordering/counts/rollup |
| Limited search clones only returned hits, 20k files | 4,301,388 allocated bytes | < 2,000,000 bytes, exact largest results |
| Streaming CSV, 20k files | 1,017,795 allocated bytes | < 4,096 bytes; CSV correctness suite preserved |
| Limit-zero search | 1,228,898 allocated bytes | zero allocations and empty results |
| Bounded largest-file cache, 100k files | 400,000 retained bytes | < 70,000 retained bytes and exact top ten |
| Partial textual top-K ordering, 20k files | 1,565,314 allocated bytes | < 1,500,000 bytes; same results as unlimited prefix |
| No selected record types | 103 allocated bytes | zero allocations and empty results |
| Stable incremental directory identity | 2 changes instead of 1; live tiny scan had 48 false moves | one real modification, unchanged rescan has zero changes; new/dangling parents remain correct |
| Short startup log lines | 20 Windows write operations for 10 lines | 10 operations, exact UTF-8/CRLF; long and empty lines preserved |
| Stable incremental directory identity | 2 changes instead of 1; live tiny scan had 48 false moves | one real modification, unchanged rescan has zero changes; new/dangling parents remain correct |
| Short startup log lines | 20 Windows write operations for 10 lines | 10 operations, exact UTF-8/CRLF; long and empty lines preserved |

A further regression checks case-folded ties, Unicode names, both textual sort fields, both directions, and mixed file/directory results. It passes. The identity regression separately failed when only the native DLL changed, then passed with combined executable/DLL identities and two history entries.

Validation: workspace Rust tests, example tests, rustfmt, strict all-target/all-feature clippy, public CSV regressions, allocation regressions, and relevant UI checks. Actual benchmark results and before/after medians are stored separately in `current-optimization-results.json`; their uncontrolled-cache limitations apply.

Measurement regressions were also checked against actual output: startup CPU/working-set and private-memory medals were absent (RED), then present in the regenerated report (GREEN). The active refresh now excludes a second process from the Windows measurement mutex; overlapping recovery attempts are archived with exclusion reasons and are not credited as performance results.
