# Repository performance red/green evidence

Each group began with a failing budget or terminal-notification test, then received the production changes needed to pass. The 50 targets are source-level operations across shared groups, not 50 independent measured speedups. Allocation and behavioral regression tests cover these groups; tightening duplicate scratch sizing added another red/green cycle. Existing correctness suites check import interoperability, quoted/Unicode paths, integer totals, writer errors, hard links, changing duplicate files, rollups, history, filtering and CLI exports.

| Public operation | Observed red | Green budget |
|---|---:|---:|
| JSON reconstructed paths, 20k files | 588,894 allocated bytes | <4,096 |
| ASCII multi-pattern CSV, 20k files | 3,848,922 | <4,096 |
| Tree build, 20k files | 3,280,612 | <3,210,000 |
| Invalid catalog analysis, 20k files | 1,228,958 | <4,096 |
| Duplicate detection, 20 x 1 KiB files | 2,510,092; reuse-only 150,796 | <50,000 |
| CSV import, 5k deep-path files | 7,770,133 | <5,200,000 |
| No-match filtered search, 20k files | 4,210 | zero |
| Foreign CSV import, 2k dated files | 3,481,423 | <2,700,000 |
| Joined path, 100+100 characters | 700 | exactly 201 |
| Directory tree, 5k directories | 1,239,276 | <1,200,000 |
| Identical sorted unique snapshot diff, 20k files | 1,114,144 | zero |
| Unchanged incremental apply, 20k files | 1,114,240 | <4,096; history retained |
| Unicode multi-pattern CSV, 20k files | 7,105,116 | <2,300,000 |
| Permission/error CSV, 1k files | 405,091 | <250,000 |
| Existing-destination source protection, 20k files | New API failed to compile before implementation | <4,096 allocated bytes; legacy Unicode/prefix semantics retained |
| Terminal pipeline flush | 70,632 allocated bytes | <4,096 |
| Impossible search bounds | 384 filter-preparation bytes | zero |
| CLI terminal notification | Test failed to compile against polling State | Callback wakes bounded terminal channel; progress ignored, failures propagated |

Budgets measure cumulative allocations during the operation, excluding fixture construction. These are not process working-set measurements. Tests using sink output isolate serializer temporary work. All output/resource medians are validated separately by n=10 release benchmarks; uncontrolled live-disk/cache changes limit causal attribution.

Commands: `cargo test --workspace --offline`; `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`; `powershell.exe -NoProfile -File scripts/check-coverage.ps1`; `pwsh -NoProfile -File benchmarks/run-winblaze-refresh.ps1 -Runs 10`.

CSV stack batching passed its writer-call test but failed n=10 Release timing (20-24% slower). Production experiment and implementation-specific test were removed. Source protection is tested against the legacy comparison oracle, including Unicode and malformed imported path fragments.
