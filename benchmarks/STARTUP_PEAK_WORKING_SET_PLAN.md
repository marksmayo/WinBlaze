# Startup peak working set at first idle

Use the original hidden-process `WaitForInputIdle` observation and process lifetime peak working-set counter. Every comparison uses all ten observations per product. Do not trim working sets, insert idle delays, suppress required work, or count later control construction as a first-idle improvement.

## Verified opportunities

| Priority | Change | Red/green evidence | Measured limit |
|---:|---|---|---|
| 1 | First-use common-controls linkage for Windows menu hooks | Shipped PE eagerly imported COMCTL32 (RED); delay descriptor retained the dependency without eager import (GREEN). Real shell popup opens and cancels without changing content. | n10 median peak: 33.64 → 33.55 MiB. This small difference does not establish a reliable saving. |
| 2 | First-use shell-library linkage | Shipped PE still eagerly imported SHELL32 (RED); first-use descriptor passes (GREEN). Command-line parsing remains linked; maximized window and shell popup pass. | n10 median peak after both changes: 33.51 MiB. No meaningful causal reduction demonstrated. |

Release builds pass. Ten fresh-appdata launches reach a visible maximized window with enabled scan controls. Rust code was unchanged in this iteration; its previous full validation passed with 92.40% line coverage.

## Rejected or outstanding candidates

Ten opt-in startup traces show zero metadata-provider calls before first idle. Provider caching, brush reuse, directory controls, treemap construction, and native catalog/cache optimizations occur later and cannot be credited here. Existing first-use graphics/virtual-disk linkage and disabled routine tracing are already implemented.

Investigate resident memory attributable to the Windows App SDK/WinUI initialization next. Module image sizes are mapped virtual ranges, not resident-memory savings. Framework replacement or forcing software rendering has wider compatibility/performance risks and does not qualify as a proven low-risk edit. There is insufficient evidence for 100 distinct low-risk first-idle fixes; do not manufacture counts or estimated savings.

## Reproduction

Run `benchmarks/run-startup-comparison.ps1 -Runs 10` inside the shared `Enter-BenchmarkMeasurementLock`/release wrapper. Build/test outside that lock interval. Preserve versions and hashes for all three products. Run `python benchmarks/verify-startup-import-budget.py`, then `node benchmarks/render-startup-peak-report.mjs <results.json>` to validate current identities and generate the focused report. Add `--require-gold` for the unchanged peak-memory medal predicate.

The focused report is not a complete refresh of the original 35 categories. Gold remains outstanding.
