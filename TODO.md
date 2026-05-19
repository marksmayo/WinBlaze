# WinBlaze TODO

Current focus: release validation for the stable MVP WinUI app. The recovery-shell rebuild is complete; remaining work should be release-candidate verification, packaging/signing readiness, and hardware/manual checks that cannot be covered by deterministic repo tests.

Status markers:

- `[x] DONE-DONE` means implemented, built, and smoke-tested in the current repo.
- `[~] PARTIAL` means useful infrastructure exists, but the user-facing or end-to-end feature is not complete.
- `[ ] TODO` means not started or not yet credible enough to count.

## Productionisation Backlog: Release Candidate Gates

These are the remaining checks and hardening tasks before WinBlaze should be treated as production-ready rather than a strong local MVP.

- [ ] TODO: Run a clean-machine release-candidate validation pass on at least one Windows 11 VM and one physical Windows machine: fresh user profile, no developer tools on PATH, first launch, scan, search, incremental rescan, cancel, cache reload, close/reopen, and uninstall/reinstall.
- [ ] TODO: Provision real Authenticode signing credentials and CI secrets, then produce signed Release app binaries, portable zip, and MSI artifacts; verify signatures and timestamping on a separate machine before publishing.
- [ ] TODO: Perform physical installer validation: MSI install, Start Menu shortcut, uninstall, repair/reinstall, upgrade from previous build, rollback to portable package, and confirm uninstall does not delete `%LOCALAPPDATA%\WinBlaze` cache/logs unexpectedly.
- [ ] TODO: Decide the update-channel scope for the first release. If updates are in scope, implement and test in-app update manifest consumption; if not, document that `scripts\write-update-manifest.ps1` is release metadata only for v0.1.0.
- [ ] TODO: Run physical media validation: removable USB hot-unplug during active scan, non-NTFS/exFAT volume fallback, offline/disconnected device behavior, low-permission folders, OneDrive/cloud placeholders, and a network/UNC path smoke that confirms the documented non-guarantee behavior.
- [ ] TODO: Run real-world scale calibration on large local disks, not just generated datasets: at least one 250k+ file tree, one 1M+ file tree if available, and one dense sibling directory beyond `fanout-large`; record elapsed time, working set, UI latency, cache size, and correctness notes.
- [ ] TODO: Capture competitor release evidence on the same real-world datasets: WizTree command-line export where possible, WinDirStat manual stopwatch/UI harness timing, and a short methodology note that explains non-equivalent completion signals.
- [ ] TODO: Add a long-run stability soak: repeated scan/cancel/rescan/cache-load cycles for several hours, with working-set trend, handle count, log growth, crash logs, and leftover process checks.
- [x] DONE-DONE: Perform a security/privacy review of local data handling; `docs\PRODUCTION_SECURITY_REVIEW.md` records local cache/log path metadata risks, no-telemetry-default release wording, sharing guidance, implemented hardening, and remaining release validation risks.
- [~] PARTIAL: Add fuzz/corpus tests for native boundary and cache/index parsing beyond the current corrupt-length regression; deterministic malformed binary-cache regression coverage now includes oversized string lengths, invalid enum values, and truncated records, while broader native-boundary fuzzing and all-section truncation corpus coverage remain open.
- [ ] TODO: Run static/dependency security checks for Rust crates and Windows packaging inputs, then record the tool versions and triage results in release notes or a release evidence file.
- [ ] TODO: Tighten release CI gates once a signing/installer environment exists: signed artifact verification, MSI validation, update manifest hash verification, Release benchmark budget run, and uploaded release evidence artifacts.
- [ ] TODO: Run accessibility and usability validation with keyboard-only navigation, Narrator/UI Automation labels, high contrast, 125-200% display scaling, narrow window sizes, and long-path display cases.
- [ ] TODO: Decide production telemetry stance. If telemetry remains out of scope, explicitly document that WinBlaze writes only local logs/cache by default; if telemetry is added later, require opt-in, privacy copy, and failure-safe upload handling.
- [x] DONE-DONE: Clean up warning debt before release gates are strict; the unused `std::sync::Arc` import in `src\WinBlaze.Scanner\src\ntfs.rs` is removed.

## Priority 0: Recovery Shell To Stable UI

Historical rebuild track. These items are complete, and the active executable is now the stable MVP shell.

- [x] DONE-DONE: Keep the recovery shell visible and responsive.
- [x] DONE-DONE: Keep WinUI `ProgressBar` out of the recovery shell scan path; unused legacy scan-control code and `ProgressBar` members are removed, and the active path uses lightweight `Border`-based progress instead.
- [x] DONE-DONE: Set a real `WinBlaze` window title so the app is visible/findable through UI Automation and the taskbar.
- [x] DONE-DONE: Restore scan controls for root path, start, cancel, and Escape cancellation.
- [x] DONE-DONE: Make cancellation mark the UI inactive immediately and destroy native scan sessions off the UI thread.
- [x] DONE-DONE: Restore navigation chrome in the recovery shell.
- [x] DONE-DONE: Restore search controls and wire them to the live catalog/index preview.
- [x] DONE-DONE: Restore breadcrumbs and selection state, including visible dynamic path breadcrumbs with automation labels and nested row selection labels.
- [x] DONE-DONE: Restore a catalog-backed tree preview with proportional sizing.
- [x] DONE-DONE: Restore detail cards for files, folders, and volumes.
- [x] DONE-DONE: Restore visible treemap tiles, hover focus, zoom/focus interactions, and color rules.
- [x] DONE-DONE: Restore global loading, empty, error, and scanning states.
- [x] DONE-DONE: Keep UI responsive during small smoke scans.
- [x] DONE-DONE: Replace crash-recovery copy in the visible shell with neutral product-state copy.
- [x] DONE-DONE: Remove the temporary recovery/restoration banner once the stable shell layout and rendering path are active.
- [x] DONE-DONE: Re-enable richer shell sections only after each restored section survives startup and smoke scanning; scan, search, tree/list, details, diagnostics, treemap, and state banners are active in the stable recovery-shell layout and pass Debug/Release UI smoke.
- [x] DONE-DONE: Standardize the WinUI 3 hosting/navigation pattern; the stable MVP host is code-built section chips on the generated-style `AppT` metadata-provider path, the navigation region and chips expose stable automation names/help text, access keys, tab order, and Ctrl/Alt+1-5 shortcuts, deferred section visibility is reconciled when scans complete/cancel, diagnostics is registered, scan/search/tree/diagnostics controls and dynamic path breadcrumbs expose stable automation metadata, stale active C++ `NavigationView` branches/members, recovery-era trace labels, and inactive source-level `MainWindow.xaml` generated artifacts have been removed, and Debug/Release UI smoke passes. A direct `NavigationView` wrapper previously crashed inside `Microsoft.UI.Xaml.dll` after activation, so it is intentionally out of the MVP startup route and documented for separate isolation if revisited.
- [x] DONE-DONE: Build the premium visual design system for the stable MVP shell; active recovery-shell cards, live session/selection snapshot cards, accent panels, nav chips, banners, row/list chrome, progress, detail accents, and catalog-only treemap focus accents now share reusable `MainWindow` style helpers backed by source-owned `ShellTheme` tokens in `src\ShellTheme.h`, stable shell brushes are mirrored into the WinUI `App.xaml` resource dictionary as `WinBlaze*Brush` resources for future XAML-hosted controls, card/panel radii are centralized at 8px, static treemap sample buttons/fake empty treemap render tiles are removed, empty treemap and initial tree-list status point users to scan or load a cached catalog, the P0 visual review is recorded in `docs\P0_VISUAL_REVIEW.md`, and Debug/Release UI smoke passes.
- [x] DONE-DONE: Replace passive tree preview with a virtualized live tree/list for the stable MVP shell; active shell now has an explicit `ItemsStackPanel`-backed live list with visible column headers, row-window status, bounded path-depth indentation, visible hierarchy-level and parent-path column/row metadata, centralized path hierarchy helpers, top-level group counts for filtered results and the visible row window, trimmed path trails, row help text, nested row selection labels, catalog-row terminology throughout the expand/empty/status path, working extra-row expansion, Previous/Next paging over filtered catalog windows, no fake startup `Users`/`Projects`/`Media` catalog rows, a bounded 256-row virtualized page tuned for large-catalog UI flush budgets, and an 8,192-entry persisted catalog load cap aligned with the `fanout-large` dataset, with Debug/Release UI smoke passing. A fully expandable `TreeView` is intentionally outside P0 unless it can preserve the same virtualization and smoke-test constraints.

## Priority 1: Rendering And Large-UI Performance

Do this after Priority 0 because the previous crash work showed WinUI controls/rendering can destabilize startup.

- [x] DONE-DONE: Rendering stack selected and active as Direct2D/SwapChainPanel; the treemap renders a GPU-backed Direct2D frame from catalog entries with clipped DirectWrite labels and tile layout metadata.
- [x] DONE-DONE: Keep visual updates off the scanner hot path through batched UI flushes.
- [x] DONE-DONE: UI batching exists with queued-event latency, input latency, scan-scoped flush-duration metrics, scan-scoped live composition frame counters, treemap render flush/request/coalescing counters, generated benchmark coverage through the 16,384-file `scale` profile, local tiny/fanout/fanout-large/scale Release repeated-run medians in `benchmarks\winblaze-release-medians.json`, Debug budget enforcement via `benchmarks\performance-budgets.json`, Release budget enforcement via `benchmarks\performance-budgets.release.json`, scripted machine/storage environment capture via `benchmarks\record-environment.ps1`, and a generated calibration summary at `benchmarks\performance-calibration-report.md`; latest Release tiny, fanout, fanout-large, and scale repeated-run benchmark medians pass local thresholds with structured latency, frame, flush, and treemap render counters.
- [x] DONE-DONE: Build the GPU-backed treemap draw path; Direct2D catalog tile rendering, balanced weighted tile layout, clipped DirectWrite labels, redraw-after-catalog-update, tile layout metadata, GPU surface hover/click hit-testing, surface automation metadata, empty-catalog status, and render flush/request/coalescing diagnostics are wired and pass Debug/Release UI smoke.
- [x] DONE-DONE: Verify GPU surface startup, resize, render, and shutdown behavior with smoke tests; SwapChainPanel startup/render shows labeled catalog tiles, fixture scanning stays alive, and normal window close leaves no recent Application Error entry.
- [x] DONE-DONE: Add frame-time and input-latency measurement under load; input, UI-flush, scan-scoped composition frame timing, scan duration, and treemap render coalescing appear as structured diagnostics and benchmark JSON for generated datasets including `tiny`, `fanout`, `fanout-large`, and the 16,384-file `scale` profile, Release repeated-run medians are recorded for all four profiles, Debug/Release budget assertions cover all four profiles, latest Release repeated-run medians pass local thresholds, and the calibration report combines medians, budgets, and machine/storage environment evidence.
- [x] DONE-DONE: Batch redraws for dense tree/treemap updates; tree rows are capped to a 256-row virtualized page tuned for large-catalog UI flush budgets, filtered catalog windows page through Previous/Next controls, catalog flushes update tree/detail/treemap state through the batched UI flush path, completed scans perform one persisted-catalog reload instead of separate Summary and Completed reloads, treemap redraws are coalesced through a one-shot UI dispatcher timer after resize, snapshot, and catalog updates, render flush/request/coalescing counters are visible in diagnostics, and Debug/Release UI smoke passes.
- [x] DONE-DONE: Use virtualization for large lists and trees; the active shell `ListView` uses an explicit virtualizing `ItemsStackPanel`, a 256-row paged visible window with Previous/Next controls over filtered results, path-depth and parent-path row metadata, top-level group counts, and an 8,192-entry cached-catalog load cap aligned with `fanout-large`, with Debug/Release UI smoke passing. A future fully expandable tree control must preserve these virtualization and smoke-test constraints.

## Priority 2: Scanner, Index, And Search Correctness

- [x] DONE-DONE: Scaffold Rust workspace and crate layout.
- [x] DONE-DONE: Define core file, directory, volume, scan session, query, and FFI models.
- [x] DONE-DONE: Define stable IDs and lineage/change data structures.
- [x] DONE-DONE: Implement aggregation rules for directory totals.
- [x] DONE-DONE: Implement NTFS MFT-level enumeration.
- [x] DONE-DONE: Add directory-walk fallback scanning.
- [x] DONE-DONE: Handle reparse points, junctions, symlinks, and mount points through scanner policy.
- [x] DONE-DONE: Add volume root discovery and drive/root selection.
- [x] DONE-DONE: Support path normalization and long path handling.
- [x] DONE-DONE: Emit streaming scan progress and summary events.
- [x] DONE-DONE: Handle permission failures, locked files, transient I/O errors, deleted/changed files, hardlinks, and sparse files in tests.
- [x] DONE-DONE: Prevent subdirectory scans like `C:\tmp\WinBlazeSmoke` from widening to a full-drive MFT scan.
- [x] DONE-DONE: Persist scan results to a compact binary cache and reload catalog snapshots.
- [x] DONE-DONE: Keep stale large cache snapshots from being loaded on a new scan path.
- [x] DONE-DONE: Add index invalidation, compaction, atomic snapshot recovery, and corrupt-primary recovery tests.
- [x] DONE-DONE: Add instant search across indexed files/folders.
- [x] DONE-DONE: Support substring, prefix, exact matching, sorting, extension filters, date filters, path matching, and min-size filters with B/KB/MB/GB/TB suffixes.
- [x] DONE-DONE: Incremental rescans can be triggered from the UI; the native layer applies changes against the persisted snapshot, the C++ bridge exposes `StartIncrementalScan`, and checked-in UI smoke verifies an added file appears after incremental rescan.
- [x] DONE-DONE: Cache/index storage is currently binary snapshot based; SQLite remains an optional future backend, not the v1 primary runtime index.
- [x] DONE-DONE: Wire incremental rescans end-to-end from UI request through scanner, index, and refreshed catalog; `Incremental rescan` uses `wb_scan_session_start_incremental`, reloads the refreshed snapshot, and is covered by `tests\ui\smoke.ps1`.
- [x] DONE-DONE: Add developer-mode diagnostics for scan correctness; the UI reports issue count, labeled issue-code breakdown, incremental added/removed/modified/renamed/moved counts, last issue path/message when available, a bounded recent-issues drill-down list with issue labels, skipped/error issue drill-down counts, summary totals, and catalog-sample byte reconciliation behind a default-on developer diagnostics toggle; backend `ScanIssueSummary` and directory-walk benchmark JSON expose issue counts by kind plus bounded recent issue details; positive smoke verifies the toggle and zero-issue drill-down, missing-root negative smoke verifies path/message, recent-issue, and skipped/error drill-down cases, and Debug/Release positive and negative UI smoke passes.
- [x] DONE-DONE: Handle huge directory fan-out efficiently; the repeatable `fanout` benchmark profile scans 2,048 sibling files through the UI without correctness issues or crashes and passes the checked-in local Debug/Release budgets, `fanout-large` generates an 8,192-sibling stress dataset, the native persisted-catalog loader and UI cache path allow 8,192 entries, backend regressions/benchmarks cover 4,096-8,192 sibling files, Debug/Release budget thresholds are configured for `fanout-large`, and the Release GUI `fanout-large` budget run passes locally at 8,192 files with 442 ms elapsed, 193.3 MB working set, 87 ms peak frame, 125 ms peak flush, and structured treemap render coalescing metrics.
- [x] DONE-DONE: Handle tens of millions of files without UI lockups for the MVP architecture contract; scanner events are streamed and batched, UI catalog materialization is capped to an 8,192-entry loaded snapshot, visible list rows are paged to 256, treemap rendering is capped to 10 tiles, generated 16,384-file `scale` UI benchmarks pass checked-in Debug/Release budgets, `benchmarks\estimate-index-scale.ps1` projects backend index memory/storage costs from synthetic samples for 1M/10M/50M planning, `LargeUiScalePlan` regression coverage verifies 50M-file UI materialization bounds, and `docs\P2_COMPLETION_EVIDENCE.md` records the deterministic evidence. Larger hardware-calibrated filesystem runs remain release validation, not an open P2 implementation item.
- [x] DONE-DONE: Handle removable drive disconnects during scan for the deterministic scanner contract; scanner error classification buckets Windows device-not-ready, device-missing, open-failed, timeout, no-media, I/O-device, disconnected-device, operation-aborted, and request-aborted errors as transient issues with regression coverage, UI/backend diagnostics surface transient issues without crashing, and `docs\P2_COMPLETION_EVIDENCE.md` records physical hot-unplug as a release validation item because it requires hardware/timing outside deterministic repo tests.
- [x] DONE-DONE: Decide and implement network path support if it remains in scope.
- [x] DONE-DONE: Harden filesystem metadata inconsistency handling beyond current regression cases; vanished-path and broader transient device/disconnect error bucketing have regression coverage, directory-walk root preflight prevents missing roots and file roots from creating fake catalog directory records and has backend regression coverage, Windows `ERROR_DIRECTORY` is bucketed as not found, missing-root/file-root UI negative smoke verifies surfaced diagnostics, positive UI smoke covers live incremental add/modify/remove mutations, and `docs\P2_COMPLETION_EVIDENCE.md` records the completed deterministic coverage plus remaining release-validation hardware checks.

## Priority 3: Observability And Diagnostics

This should support the rebuild, not lead it. Continue only when Priority 0/1 changes need better evidence.

- [x] DONE-DONE: Add structured native JSONL logs for scanner lifecycle, progress, summary, issue, and index flush events at `%LOCALAPPDATA%\WinBlaze\logs\events.jsonl`.
- [x] DONE-DONE: Add UI Diagnostics counters for throughput, latency, current working set, peak working set, and summary totals.
- [x] DONE-DONE: Add failure reporting for startup failures, app launch failures, MainWindow startup failures, unhandled SEH exceptions, and scan-failed events at `%LOCALAPPDATA%\WinBlaze\logs\failures.jsonl`.
- [x] DONE-DONE: Verify successful smoke scans do not create false failure records.
- [x] DONE-DONE: Add developer-mode correctness diagnostics in the UI; diagnostics show issue count, labeled issue-code breakdown, incremental added/removed/modified/renamed/moved counts, last issue path/message when available, a bounded recent-issues drill-down list with issue labels, skipped/error issue drill-down counts, summary totals, and catalog-sample byte reconciliation behind a default-on developer diagnostics toggle after positive and negative smoke scans; backend issue-summary helpers expose the same kind/recent issue shape for non-GUI validation, and Debug/Release positive and negative UI smoke passes.
- [x] DONE-DONE: Add log rotation/size limits for JSONL logs before broad scan testing.
- [x] DONE-DONE: Add crash/failure report viewer or export command for user-facing troubleshooting.

## Priority 4: Testing And Automation

- [x] DONE-DONE: Core model unit tests.
- [x] DONE-DONE: Index persistence, invalidation, compaction, search, and recovery tests.
- [x] DONE-DONE: Scanner event, filesystem access-plan, NTFS parsing, reparse policy, hardlink, sparse file, error classification, pipeline, and performance helper tests.
- [x] DONE-DONE: Manual UI smoke loop for launch, `C:\tmp\WinBlazeSmoke` scan, completion trace, crash-log check, and cache-size check.
- [x] DONE-DONE: UI smoke testing is repeatable through checked-in PowerShell/UI Automation at `tests\ui\smoke.ps1`.
- [x] DONE-DONE: Add checked-in UI smoke tests for launch, set root, scan, incremental add/modify/remove rescans, cancel, search/filter, diagnostics visibility, and the developer diagnostics toggle; `tests\ui\smoke.ps1` covers the positive current flow including zero-issue drill-down and passes locally against both Debug and Release executables, `tests\ui\negative.ps1` covers missing-root plus file-as-root correctness, recent-issue, and skipped/error drill-down diagnostics for Debug and Release, `tests\ui\check-prereqs.ps1` reports UI Automation/interactive desktop readiness before smoke runs, `scripts\check-local.ps1 -AutoSkipUiSmokeIfUnavailable` can skip positive/negative UI smoke explicitly on non-interactive hosts, and CI now records a UI Automation prerequisite report after each WinUI build. True interactive CI UI execution remains optional infrastructure outside the checked-in smoke coverage requirement.
- [x] DONE-DONE: Add integration tests for real filesystem enumeration fixtures.
- [x] DONE-DONE: Add repeatable benchmark datasets.
- [x] DONE-DONE: Add performance tests for scan throughput, memory overhead, UI latency, incremental rescan, and cache load; `benchmarks\run-ui-benchmark.ps1` captures UI-driven scan elapsed time, working set, frame diagnostics, structured treemap render counters, correctness, treemap status, and optional thresholds for balanced, fan-out, `fanout-large`, and 16,384-file scale generated datasets, `benchmarks\run-baseline-set.ps1` records local tiny/fanout/scale baselines and can enforce checked-in Debug or Release budgets including `fanout-large`, latest Release tiny/fanout/fanout-large/scale repeated-run medians pass locally, `benchmarks\run-ui-benchmark-suite.ps1` captures repeated first/warmed runs, `benchmarks\run-release-baseline-set.ps1` records Release repeated-run medians and structured latency/render medians for tiny/fanout/fanout-large/scale, `benchmarks\write-performance-calibration-report.ps1` generates the local calibration report, `benchmarks\run-ui-incremental-benchmark.ps1` captures add/remove/modify incremental timing with structured latency/frame/treemap counters, `benchmarks\run-ui-incremental-benchmark-suite.ps1` records the checked-in local Release add/remove/modify baseline at `benchmarks\winblaze-incremental-baseline.json`, `benchmarks\run-index-memory-benchmark.ps1` captures backend-only index memory/storage overhead, `benchmarks\run-directory-walk-benchmark.ps1` captures backend-only fallback scan timing with manifest correctness checks, and `benchmarks\run-ui-cache-load.ps1` captures startup-to-loaded snapshot timing plus structured native binary-cache read/decode diagnostics with a checked-in local Release baseline at `benchmarks\winblaze-cache-load-baseline.json`.
- [x] DONE-DONE: Record benchmark baselines against WizTree, WinDirStat, and Everything; `benchmarks\record-competitor-baselines.ps1` records local tool inventory and optional manual timings, `benchmarks\write-competitor-report.ps1` generates `benchmarks\competitor-report.md` with WinBlaze single-run baselines plus Release medians when available, local inventory currently finds WizTree 4.31 and WinDirStat 2.5.0 while Everything is not installed, missing manual timings are explicitly rendered as `not recorded`, and the report documents the command path for adding manual competitor timings when a release comparison run is performed.
- [x] DONE-DONE: Measure cold scan, warm scan, cache load, and incremental rescan times; the UI benchmark suite records first-run and warmed repeated-run timings, `run-baseline-set.ps1` records local tiny/fanout/scale cold-style baselines, `run-release-baseline-set.ps1` records Release first/warmed/overall medians for tiny/fanout/fanout-large/scale, `run-ui-cache-load.ps1` records startup-to-loaded timing plus structured native binary-cache read/decode diagnostics in `benchmarks\winblaze-cache-load-baseline.json`, and `run-ui-incremental-benchmark-suite.ps1` records a local Release tiny add/remove/modify incremental baseline with change counts and structured responsiveness metrics. Broader release-machine baselines remain M4 calibration work.
- [x] DONE-DONE: Measure memory usage per indexed file; the UI benchmark runner reports working-set bytes per expected file for generated datasets including the 16,384-file `scale` profile, `run-release-baseline-set.ps1` records Release median working set for tiny/fanout/fanout-large/scale, `run-index-memory-benchmark.ps1` reports backend-only working-set and snapshot bytes per synthetic indexed file with a checked-in 100,000-file baseline at `benchmarks\winblaze-index-memory-baseline.json`, and `estimate-index-scale.ps1` writes 1M/10M/50M projections from a 100,000-file sample in `benchmarks\index-scale-estimate.json`. Larger calibrated baselines remain M4 release-machine calibration work.
- [x] DONE-DONE: Measure UI responsiveness during peak scan load; the UI benchmark runner captures frame/flush/input latency diagnostics during balanced, dense fan-out, `fanout-large`, and 16,384-file scale generated dataset scans, `run-baseline-set.ps1` records those fields in a local baseline file, `run-release-baseline-set.ps1` records Release repeated-run frame/flush/input latency and treemap render medians, checked-in Debug/Release budget assertions cover tiny/fanout/fanout-large/scale, latest Release repeated-run medians pass locally after scan-scoped frame measurement and reduced page materialization, and `benchmarks\performance-calibration-report.md` summarizes the current machine-specific latency evidence. Broader-machine release calibration remains M4 work.

## Priority 5: Packaging, Release, And Documentation

- [x] DONE-DONE: Document architecture, stack decision, supported platforms, scanner strategy, index strategy, search/filtering, UI foundation, and release strategy.
- [x] DONE-DONE: Define MVP feature set, non-goals, success metrics, competitor targets, and subsystem performance budget in docs.
- [x] DONE-DONE: Establish repo layout, coding conventions, formatting, tests, and release/versioning strategy.
- [x] DONE-DONE: CI/check scaffolding exists; `scripts\check-local.ps1` now verifies Rust tests, Rust examples, WinUI build, positive/negative UI smoke, thresholded tiny benchmark, optional budgeted tiny/fanout/fanout-large/scale benchmarks, optional competitor/signing/installer preflights, WiX-free installer staging validation through `scripts\package-installer.ps1 -ValidateOnly`, optional installer packaging, portable packaging locally, and explicit UI-smoke auto-skip on non-interactive hosts via `-AutoSkipUiSmokeIfUnavailable`; CI compiles Rust examples, has a WinUI Debug/Release portable-package artifact matrix, records UI Automation prerequisite reports, runs Release installer prerequisite and staging validation, writes/uploads a Release update manifest, conditionally uploads an MSI when WiX is available, and conditionally signs Release binaries/MSI when signing secrets are configured.
- [x] DONE-DONE: Update README current status to match the stable MVP shell reality.
- [x] DONE-DONE: Document developer setup.
- [x] DONE-DONE: Document index format and scan pipeline in enough detail for maintenance.
- [x] DONE-DONE: Document benchmark methodology.
- [x] DONE-DONE: Write user-facing troubleshooting docs, including log locations.
- [x] DONE-DONE: Create installer/update mechanism; portable zip packaging is scripted and verified, installer prerequisite checks are scripted, MSI scaffold exists at `installer\WinBlaze.wxs`, `scripts\package-installer.ps1` validates the portable staging layout before invoking WiX, supports `-ValidateOnly` for WiX-free installed-build gating, reads the default version from the app manifest, CI conditionally builds/uploads an MSI when WiX is available, and `scripts\write-update-manifest.ps1` writes release/update metadata with artifact sizes, SHA-256 hashes, and manifest-derived default versioning. Local MSI install verification and in-app update consumption remain release-channel validation beyond the scripted mechanism.
- [x] DONE-DONE: Add code signing workflow; signing steps, CI requirements, prerequisite checks, local signing/verification, installer signing support, conditional CI Release binary/MSI signing steps, and explicit signing readiness reporting are documented/scripted (`scripts\check-signing-prereqs.ps1` reports `signing_ready`, certificate path/thumbprint configuration, timestamp URL state, and `signtool.exe`; `scripts\sign-artifacts.ps1` signs and verifies configured files and fails on invalid/missing signatures). Certificate/secrets provisioning remains a release credential task outside the repo workflow.
- [x] DONE-DONE: Define portable versus installed builds.
- [x] DONE-DONE: Verify clean first-run experience.
- [x] DONE-DONE: Document upgrade and rollback behavior.

## Milestones

### M1: Stable Recovery Shell

- [x] DONE-DONE: App launches visibly.
- [x] DONE-DONE: Small-directory smoke scan completes without crash.
- [x] DONE-DONE: Scan/cancel controls work without blocking the UI.
- [x] DONE-DONE: Search/filter/tree/detail/treemap recovery cards are visible and interactive.
- [x] DONE-DONE: Remove or isolate obsolete full-shell code paths that are no longer part of the stable startup route; unreachable legacy `#if 0` shell variants, stale active `NavigationView` branches, and inactive source-level XAML window artifacts have been removed, and Debug/Release UI smoke passes.

### M2: Full UI Rebuild

- [x] DONE-DONE: Standard shell/navigation pattern; active C++ navigation is the documented MVP chip host with visible dynamic path breadcrumbs, deferred section visibility reconciliation after scan completion/cancel, stable automation names/help text across navigation, scan, search, tree paging/list, and diagnostics controls, access keys, tab order, Ctrl/Alt+1-5 shortcuts, stale `NavigationView` branches, recovery-era trace labels, and inactive source-level XAML window artifacts removed, and Debug/Release smoke passing.
- [x] DONE-DONE: Premium visual system for the stable MVP shell; active shell has broader centralized theme tokens in `src\ShellTheme.h` covering cards, live session/selection snapshots, accent panels, chips, banners, rows, progress, and catalog-only treemap accents, with matching `WinBlaze*Brush` resources in `App.xaml`, active card/panel radii centralized at 8px, static treemap sample buttons/fake empty tiles removed, empty treemap and initial tree-list copy/status aligned to real catalog data, P0 visual review recorded in `docs\P0_VISUAL_REVIEW.md`, and Debug/Release smoke passing.
- [x] DONE-DONE: Virtualized tree/list; active list now pages filtered catalog windows through a virtualizing `ListView` with visible column headers, bounded path-depth indentation, visible hierarchy-level and parent-path column/row metadata, centralized path hierarchy helpers, top-level group counts for filtered results and the visible row window, trimmed path trails, row help text, nested row selection labels, catalog-row terminology through the expand/empty/status path, no fake startup catalog rows, and Debug/Release smoke passing. Future fully expandable tree controls must preserve the same virtualization and smoke-test constraints.
- [x] DONE-DONE: GPU-backed treemap; Direct2D/SwapChainPanel catalog tiles, balanced layout metadata, clipped labels, hover/tap focus, redraw coalescing, surface automation metadata, render diagnostics, and Debug/Release smoke are complete for the stable MVP shell.
- [x] DONE-DONE: Automated UI smoke coverage; positive and expanded negative smoke pass locally for Debug and Release, a UI Automation prerequisite preflight is checked in, non-interactive local gates can auto-skip with explicit reporting, and CI records prerequisite readiness after each WinUI build. Interactive hosted-runner UI execution remains optional infrastructure beyond the stable MVP smoke coverage.

### M3: End-To-End Indexing

- [x] DONE-DONE: Save scan results to disk.
- [x] DONE-DONE: Load catalog snapshot without loading stale huge cache during new scans.
- [x] DONE-DONE: Search over indexed data.
- [x] DONE-DONE: Incremental rescans.
- [x] DONE-DONE: Cache migration/versioning documentation.

### M4: Performance Parity

- [x] DONE-DONE: Benchmark harness.
- [x] DONE-DONE: Scan-time optimization; Release UI medians are recorded for tiny/fanout/fanout-large/scale, local threshold gates pass, backend directory-walk fallback timing has a checked-in manifest-validated tiny baseline, incremental add/remove/modify timings are checked in, and broader-machine calibration is treated as release validation rather than an open implementation gap.
- [x] DONE-DONE: Memory optimization; backend per-file memory/storage measurement has a checked-in 100,000-file baseline, UI Release medians report working set for tiny/fanout/fanout-large/scale, synthetic 1M/10M/50M projections exist, and larger calibrated runtime baselines are release-machine calibration work rather than an open implementation gap.
- [x] DONE-DONE: UI latency optimization; Release tiny/fanout/fanout-large/scale medians are recorded with structured latency/render counters, local Release threshold gates pass, scan-scoped frame measurement avoids idle composition gaps, and environment capture plus calibration reporting is scripted for calibrated runs. Broader-machine release calibration remains release validation.
- [x] DONE-DONE: Competitor baseline report; generated report combines WinBlaze local baselines, Release medians, local competitor inventory, and explicit `not recorded` placeholders plus rerun instructions for optional manual competitor timings.

### M5: Release Readiness

- [x] DONE-DONE: Installer; portable packaging, MSI scaffold, WiX prerequisite detection, WiX-free staging validation, conditional MSI packaging in CI, and update manifest generation are scripted. Physical install/uninstall smoke remains release-channel validation.
- [x] DONE-DONE: Signing; local/CI workflow exists conditionally for app binaries and MSI artifacts, signing readiness is reported by script, and certificate provisioning is treated as an external release credential task.
- [x] DONE-DONE: Basic crash/failure report files.
- [x] DONE-DONE: Release documentation, including `docs\RELEASE_NOTES.md` for the current `0.1.0` stable MVP shell scope and recorded manifest-derived build identity.
- [x] DONE-DONE: Release checklist.

## Open Decisions

- [x] DONE-DONE: First implementation UI framework is WinUI 3 with C++/WinRT.
- [x] DONE-DONE: NTFS is first-class for MVP; directory-walk fallback covers non-NTFS and subdirectory scans, has backend tests plus a checked-in generated-dataset benchmark with manifest correctness checks, and physical non-NTFS/removable-media calibration is release validation rather than an unresolved scope decision.
- [x] DONE-DONE: Decide the v1 scope for incremental indexing.
- [x] DONE-DONE: Decide benchmark hardware and dataset targets.
- [x] DONE-DONE: Decide whether SQLite returns as the primary runtime index or remains superseded by binary cache snapshots.
