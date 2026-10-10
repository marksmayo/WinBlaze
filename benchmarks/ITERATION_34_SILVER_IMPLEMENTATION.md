# Iteration 34: silver-target implementation

All ten changes in SILVER_TOP_10_ANALYSIS.md are implemented in source, pending full validation and adoption. The official report remains the completed iteration 32 cohort (35 results, 29 gold, 4 silver, 2 bronze) until fresh qualifying observations replace its rows.

1. Hidden search, details, runtime panels and secondary page hosts build on first use. Real XAML regression failed against the old build for eager search construction, then passed for readiness, deferred controls, reveal and preserved search state. Included in the full build gate.
2. Short file display names use 22 inline UTF-8 bytes. A 16,384-name allocation regression fell from 344,064 bytes to zero. Directory metadata decodes short ASCII names directly without an intermediate allocation; Unicode and unpaired UTF-16 filesystem names remain exact.
3. CSV reuses row storage and a bounded escaped parent prefix, writing each complete row once. The writer-call regression fell from 11,014 calls for 1,002 rows to at most 1,002; complete export and escaping checks pass.
4. File totals accumulate locally and publish at bounded progress boundaries. Existing accounting tests and a new uneven-branch test verify final counts and sizes. No independent pre-change failing test is claimed for this counter-only optimization.
5. Scanner workers grow with available branches, bounded by the configured limit. The new policy test initially failed to compile before the API existed, then passed.
6. File IDs reserve exact ranges for batches of at most 64, including partial batches. An uneven 65/129-file branch fixture verifies 194 consecutive IDs and exact totals; no unused ranges.
7. Dense short-name metadata continuation batches grow to 64 KiB. The request-count regression measured 20 old requests versus 12 new requests for 4,096 files. The initial predicted <=10 target was corrected to <=12 after measurement; this is an internal resource test, not a changed medal definition. Long-name directories retain 32 KiB continuation storage after an existing allocation test exposed a regression with unconditional growth. All 15 scanner resource tests now pass.
8. Startup shell state applies once; summary/runtime refresh is suppressed while construction is incomplete, and unchanged runtime text is not reassigned. Runtime counters initialize when their panel opens. Visible readiness remains inside the measured startup boundary.
9. Terminal CLI scans consume and index events on the caller, removing the extra forwarding thread. The integration test initially failed to compile before the API existed; it now checks callback thread identity, complete CSV export and expired-deadline cancellation. The GUI retains asynchronous delivery.
10. Tree child display storage and sorting initialize on demand. A 16,384-file publication allocation regression fell from 65,728 bytes to below 4,096. Repeated child queries allocate no new storage. Full-suite testing caught lost orphan-to-root handling; both rollups and deferred file order now retain the original behavior.

Validation in progress: full workspace, examples, coverage, current Release build, UI/package/device/resource gates. Strict clippy passed after correcting test placement and grouping index scope arguments. Builds made before final repairs are diagnostic only and will not be staged.

Benchmark policy: original 35 tests, ten observations each; retain all outliers and unchanged R7 IQR/median definition. Keep latest qualifying WinBlaze values, retain every lower-is-better competitor best including variability, and do not rerun unchanged competitors. The previous administrator worker exited; renewal requires actual Windows UAC consent when the full-volume refresh is ready.

Further resource validation: the original delivery retention budget exposed cached 64 KiB scratch at terminal completion. Large continuation buffers are now released at iterator drop, preserving the established 32 KiB idle cache bound; the delivery regression passes. The short-name allocation test warms the active iterator through its first continuation before measuring remaining names, separating per-name allocation from intentionally bounded kernel scratch; it still measures more than half the fixture and verifies all 1,024 entries. Retained administrator worker renewed after two preflight invocations (each with two child-token checks), PID 28536; no competitor launched.

Final Rust gates: full workspace tests PASS; examples PASS; rustfmt check PASS; strict all-target/all-feature clippy PASS; unchanged line coverage gate PASS at 92.45%. Final source Release UI build and adoption are still pending.

Final Release UI build gates PASS; staged current binaries with original iteration 32 backup at artifacts/ui-stage-backup-d7d60381a39f482291854b575805d711. Portable ZIP and extracted UI/CLI PASS. No-device test cannot qualify: independent unchanged native WPD inventory lists USB storage D:, and the UI correctly opens device selection. Inventory retained at artifacts/all-gold-loop/iteration-34/portable-device-inventory.json. No disconnected-device pass is claimed. Original 35 medal workloads do not require portable-device disconnection.

## Completed adoption and benchmark

Batch: refresh-20261009T135820398Z. Final archive: benchmarks/runs/2026-10-09T14-11-11-169Z/report-data.json. All 35 original categories have fresh qualifying ten-observation WinBlaze values; no pending categories. Final report validators pass; all 70 frozen competitor best objects and official values exactly match the frozen baseline. No competitor executable was benchmarked.

Final standings: 30 gold, 5 silver, 0 bronze (previously 29 gold, 4 silver, 2 bronze). All ten source changes are in the staged supported Release build. The all-gold objective remains unmet.

| Metric | Previous | Latest | Result |
|---|---:|---:|---|
| Startup working set / peak at first idle | 16.7891 MiB | 14.6445 MiB | Both gold; about 12.8% lower |
| First idle queue time | 53.095 ms | 50.170 ms | Gold |
| Quick-folder timing variability | 22.2288% | 11.7334% | Bronze to silver |
| Full C: timing variability | 35.1276% | 6.2040% | Bronze to silver |
| Empty-file timing variability | 11.1111% | 8.3919% | Silver |
| Dense-folder timing variability | 6.0640% | 8.5597% | Silver; regression retained |
| Standard-user full-drive private memory | 438.4707 MiB | 469.8906 MiB | Gold to silver; regression retained |
| Empty-file private memory | 3.5000 MiB | 1.1270 MiB | Gold |

All 400 measured WinBlaze feature observations validated (including 30 PNG observations), with no unmeasured non-MFT entries. Existing Release UI budgets pass: 467 ms elapsed, 31 ms peak frame, 5 ms peak flush, exact fixture totals and normal shutdown. Rust workspace/examples/format/lint gates pass and coverage is 92.45%. Final OS XAML build passes imports/icons/restored-window/expansion/focus/lazy-panel/three PNG/CLI console checks. Portable ZIP and extracted binaries pass. Disconnected-device validation remains unavailable because WPD independently enumerates USB storage D:; this is explicitly not recorded as a no-device pass.

The fixed administrator worker remains ready (PID 28536, original reviewed worker script unchanged). Its job published via the corrected repository working directory and completed without additional UAC prompts or recovery of measurements. Original iteration 32 staged binaries are preserved at artifacts/ui-stage-backup-d7d60381a39f482291854b575805d711.

Five remaining silver categories: four unchanged-definition timing variability events and standard-user full-drive private memory. Next investigation should quantify the larger file-record stride and isolate dense-folder scan/export variability; these are hypotheses, not established causes.
