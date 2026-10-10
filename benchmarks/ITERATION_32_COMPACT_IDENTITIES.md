# Exact clustered identity storage

Target memory and processing work rather than manipulating timing variability.
The new public million-file concurrent-accounting resource test initially failed:
18,883,584 bytes retained against an unchanged 12 MiB candidate budget. It now
passes at 8,400,896 bytes, a 55.5% reduction in this clustered synthetic case.
Both figures include the tracker's inline state and retained heap storage.

Narrow identities start in the existing hash set. Sufficiently clustered sets
promote to exact bits; geometric growth is capped at 1 MiB per dense window.
Sparse references remain in a hash set and full-width IDs retain all 128 bits.
No NTFS sequence bits, file identifiers or volume identity are discarded.
The public claim interface, zero-byte behavior and sharding remain unchanged.

A descending range at `u64::MAX` exposed an overflow in the initial candidate
(actual RED). Bounded arithmetic and range positioning fix it (GREEN). Tests
also cover reversed duplicate claims, volume promotion, distant references,
full-width aliases, interleaved workers and zero-byte claims.

Scattered million-file retained storage is 18,887,680 bytes, only 4,096 bytes
above the previous clustered/hash-set baseline. Small 64-file retained storage
rises from 11,520 to 15,616 bytes and still passes the existing 16 KiB ceiling.
This is a documented constant-size tradeoff, not a universal memory reduction.

The separate optimized AB/BA diagnostic retains every one of 240 observations
(20 pairs, three identity shapes, one/four workers, one million identities).
Every run verifies exact byte totals and duplicate accounting. Median elapsed
cost changes were -49.76%/-48.88% clustered, -43.49%/-36.52% mixed-volume with
wide IDs, and -2.25%/-0.27% scattered, for one/four workers respectively.
These are synthetic elapsed costs, not measured process CPU or medal claims.
Source snapshots, raw observations and summaries are under
`artifacts/all-gold-loop/iteration-32`.

Full Rust, coverage, Release UI/package validation and the next original-35
WinBlaze refresh are pending. The previous official report remains 35 rows,
32 gold / 3 silver, with all 70 competitor best records frozen and startup CPU
visibly pending. No new gold medals or product-level resource gains are claimed.

Validation update: workspace tests, example tests, formatting and strict clippy
pass; line coverage is 92.46%. The Release UI gate exposed pointer focus state
after native focus restoration. The previously accepted iteration28 harness
also fails now with the same host-header hash, establishing an existing issue.
The product host now explicitly applies keyboard focus to the restored control
within its own XAML root. The unchanged harness passes entry, restoration,
forward/reverse wrapping, empty-root handling and release. No global input or
weakened assertion is used. Full Release validation is rerunning in a separate
`release-build-v2` directory; the failed run is preserved.

The retained administrator worker PID37316 is absent and its last heartbeat is
2026-10-08T12:18:47Z. Do not treat its old ready status as live or dispatch work
to it. Standard-user refresh remains possible; administrator results must stay
pending until a new approved elevated worker is available.

Adoption update: the complete Release `release-build-v2` gate passed and staged
UI66EC5CEA / native62E561D3 / CLI3A9B6172 (updater unchanged). Portable-package
hashes, extracted CLI/terminal behavior, icons and three disconnected-device
discoveries pass. The unchanged tiny UI budget passes at 412 ms / 123 MiB,
26 ms peak frame / 3 ms peak flush. A single authorized UAC renewal started
WinBlaze-only worker44208; two elevated child-token checks pass and the worker
remains available for future iterations. Neither competitor is launched.
The ten-run refresh `refresh-20261008T232915207Z` is in progress; all prior
35 rows remain visible pending current qualifying values.

Final iteration32 result: batch refresh-20261008T232915207Z completed all 35 qualifying WinBlaze categories. Final standings are 29 gold, four silver and two bronze; all 70 competitor best records and official values are unchanged. Standard collection completed at 2026-10-08T23:40:36.539Z and the native observations completed at 23:41:35.576Z. Administrator publication initially failed because .NET resolved relative paths from System32. The same ten valid native observations were recovered with absolute paths; no measurement was rerun or selected. Final validation and all four report validators pass. The next-job script now sets the .NET working directory explicitly. The retained worker is PID44208; no additional UAC approval was requested.

The newest user request is analysis of the four silvers. SILVER_TOP_10_ANALYSIS.md records the current gaps, code evidence, ten ranked hypotheses and diagnostic phase measurements. Those diagnostics do not change the official report. Iteration33 allocation source remains unstaged and does not explain these small-fixture or startup silvers.
