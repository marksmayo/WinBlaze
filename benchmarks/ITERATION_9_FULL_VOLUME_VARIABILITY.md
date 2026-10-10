# Full C: timing variability investigation

The user has prioritized this category over the pending UI startup candidate.
The official iteration 8 result is 48.34% (IQR / median, R-7 quantiles, ten
startup + scan/indexing + CSV export observations). The retained WinDirStat
historical 2.8.0 target is 5.525888421531474%. WinDirStat has since updated to
2.9.2, requiring a fresh version-specific baseline. Original definitions and all
35 report rows remain.

Iteration 8 WinBlaze full-volume observations ranged from 31.16 to 91.67 seconds.
WinDirStat and WizTree also varied substantially in that same batch. Nearly
constant file counts do not explain the spread, but shared variation alone does
not establish a system-load or cache cause.

The shipping CLI identity is
EE3FDF8D99AF69D3F1752762F2AF93FFD8CA6AE43289B0EB2BA2D39DFC2023DE.
Its existing optional WINBLAZE_CLI_PHASES instrumentation distinguishes the
terminal event, session destruction, and export. No production code was changed.

An interrupted diagnostic in
artifacts/all-gold-loop/iteration-9/full-volume-phases-847b53f5-891f-4c66-8f04-520f9ae1e855
retained its evidence. Its first run was 54.106 seconds: 53.094 seconds to the
terminal event, about 1 millisecond destruction, 0.877 seconds export. The writer
failed on its second atomic replacement, which Windows denied. Immutable per-run
observations and normal diagnostic aggregate writes now avoid that failure.

The clean ten-run diagnostic is
artifacts/all-gold-loop/iteration-9/full-volume-phases-985bce91-55c4-4ca3-a849-44327c5a5b2c.
Median wall time was 13.566 seconds, with 4.302% variability. Scan/index
publication median was 12.788 seconds; destruction 1.329 milliseconds; export
0.756 seconds. File counts ranged from 1,772,710 to 1,772,726. Wall time correlated
with scan phase at 0.9983 and kernel CPU at 0.9898; export correlation was -0.0618.
Median process CPU was 48.047 seconds, of which 43.352 seconds was kernel CPU.

This is diagnostic evidence, with direct Process.Start, redirected output and
optional phase files. It cannot replace the official medal result. There was no
new implementation between the interrupted and clean cohorts. Their timing
change demonstrates why a quiet or warmed repeat alone is insufficient proof of
a product improvement.

Ranked explanations and predictions:

1. Enumeration/kernel cost changes with cache or system state. The scan phase
   and kernel CPU should explain most wall variation. Supported by the clean
   cohort; the precise cache/system cause is not established.
2. Resource sampling perturbs timing. An alternating ABBA comparison with ten
   sampled and ten unsampled runs should show a reproducible sampler penalty.
3. Finalization/export dominates spread. Phase measurements should correlate
   with wall time. Not supported by the clean cohort.

The next diagnostic alternates the existing benchmark resource counter queries
with unsampled runs, retaining identical process launch, scan, export, token and
phase instrumentation. Each group must be summarized separately. Official
comparison remains unchanged until validated benchmark evidence is available.

The completed alternating twenty-run cohort is
artifacts/all-gold-loop/iteration-9/full-volume-phases-00af96fc-23c6-47e0-b3f7-f32109516bea.
Each sampler group contains ten observations. Sampler-on median was 14.509
seconds with 28.498% variability; sampler-off median was 12.858 seconds with
26.400% variability. Both fail the retained competitor variability target.
The observed median sampler difference is 12.85%, but this single interleaved
cohort does not establish a stable causal penalty. Late runs increased to
29.318 seconds with sampling and 21.773 seconds without sampling. Their scan
phases were 28.580 and 20.577 seconds, respectively. Both groups have wall/scan
correlation above 0.9997. Therefore removing the sampler does not resolve the
variability defect, and finalization/export is not the principal target.

Read-only process CPU snapshots were retained as full-volume-background-start.json
and full-volume-background-end.json. They cover only part of this cohort, do not
include CPU counters unavailable to the standard token, and cannot rule out
storage, antivirus, cache, thermal or scheduling effects. No external processes
were stopped or reconfigured.

Next: distinguish kernel enumeration stalls from scan/index consumer or model
publication stalls with finer stage evidence. Keep a failing full-volume
variability cohort, rather than selecting the earlier quiet cohort as a fix.
The original medal verifier still passes all 35 unchanged rows and retained
competitor bests. No product change or new official medal claim was made.

Further stage evidence: ten standard-token scans with writable isolated storage
completed in full-volume-phases-3cf56762-a5c8-45fc-83b0-1d6d16837e80. All
persistence flushes succeeded. Wall median was 13.686 seconds with 15.869%
variability. Scanner-completed median from launch was 12.337 seconds; approximate
model publication 249.765 milliseconds; session finalization 186.622 milliseconds;
export 681.135 milliseconds. Scanner-completed time explained wall variation
(correlation 0.9952). Publication estimates mix UTC log time and monotonic CLI
time and include launch offset; they are coarse diagnostics, not precision
measurements.

The atomic-writer regression failed inside the filesystem sandbox but passed
outside it. A proposed explicit backup-file change did not fix the restricted
case and was reverted completely. No writer change remains. Normal application
cache/log files had stale timestamps in the restricted diagnostics. This is an
execution-context defect, not established as a product defect or causal proof
of all timing variation.

Ten further standard-token diagnostics outside the filesystem sandbox using
normal application storage completed in
full-volume-phases-bbe35b36-4639-4eeb-abcc-1dac60862dc2. All persistence flushes
succeeded. Median wall time was 11.4095 seconds with 1.2320% variability; scanner
completed median 10.338 seconds, approximate publication 223.463 milliseconds,
finalization 136.502 milliseconds, export 572.598 milliseconds. Median CPU was
40.414 seconds, including 36.586 seconds kernel CPU. These observations are
diagnostic and do not replace canonical benchmark evidence. The environment
comparisons were consecutive cohorts, not an interleaved causal experiment.

WinDirStat 2.9.2 executable hash is
52742814A3983B0A934F9B857A8B7CF3A10328134BA1129E91D8DE25293B68B2.
The initial restricted-context n=10 Full C cohort is retained separately in
windirstat-2.9.2-full-35c12003-9f41-4a8c-9ca5-b5b30e8a3c7a, median21.1659
seconds and variability21.5641%. It is not mixed into the 2.8.0 retained bests.
WizTree's executable hash remains unchanged.

The diagnostic variability verifier caught the absent current-version baseline
before comparing performance. That failure is a baseline-identity guard, not a
performance RED/GREEN claim. A fresh canonical iteration10 now runs all original
35 categories across all three products outside the filesystem sandbox. Standard
tests retain standard tokens; the new retained worker handles admin tests.
