# Segment heap startup experiment

Microsoft documents the SegmentHeap application-manifest setting for Windows
10 build19041 and later and notes that it generally reduces memory usage:
[heapType](https://learn.microsoft.com/en-us/windows/win32/sbscs/application-manifests#heaptype).
This is a hypothesis for WinBlaze, not a promised improvement. Both reference
executables lacked a heapType declaration.

Two separate copies were made: the iteration17 OS XAML candidate and the
shipping WinUI app. Only their embedded manifests were changed. Windows SDK
mt.exe extracted each original manifest and embedded the modified version;
original UAC, compatibility and DPI declarations remain. All non-resource PE
sections are byte-identical to their respective references, verified in
artifacts/all-gold-loop/iteration-18/code-section-parity.json. Both retain
native DLL9FF4B2B. No shipping file or source was changed.

The OS candidate C4EB3D77 passes initialized small/taskbar icon pixels against
the prior executable and normal close (ui-icons-20261008T052706726Z). A matched
n10-per-host diagnostic passes all20 initialized-control, owner, PMv2 and
normal-exit checks: iteration9/full-ui-paired-
4b62bb64964d4322b0b7567298896b90. Segment/default first-idle medians are
16.2578125/16.66015625MiB; initialized126.68359375/126.1796875MiB. These
diagnostics do not establish a causal improvement.

One canonical default-storage n10 cohort for the changed OS candidate is
**RED:16.28125MiB versus16.244140625MiB**, a38KiB gap. Median idle34.435ms,
CPU31.25ms. All samples including first24.265625MiB are retained in
segment-startup-default-storage.json and segment-resource-gate.json.

The shipping WinUI copy130A5616 also passes icon pixels and normal close
(ui-icons-20261008T053001085Z). Its separate canonical n10 result is
**33.783203125MiB**, above both gold16.244140625 and silver31.65625. Median
idle50.365ms, CPU62.5ms. All samples including37.61328125MiB first remain in
shipping-segment-startup.json and shipping-segment-resource-gate.json.
The setting is not adopted. No unchanged-binary retry or historical-best
substitution was used. This does not claim that the setting caused a
regression against an earlier shipping cohort on a changing live system.

The residency probe now separately lists mapped files for pages outside its
module ranges, while preserving the original UnattributedBytes and first-idle
counters. Three diagnostic pairs e8751ba2959d4edd9aa2f960461ad77a identify
resident C_1252.NLS12KiB and C_437.NLS8KiB, plus locale-related data. This
does not justify changing application encoding semantics to recover the
38KiB gap. Snapshots run after first-idle counters as initialization continues;
the added probing overhead is not usable-window or official medal evidence.

Official results remain27gold7silver1bronze across35 rows; retained competitor
bests are untouched. The OS XAML candidate still requires resource margin
and complete UI parity before adoption. Automatic goal status is not changed.

Two repeated no-device discoveries with actual changed-fixture rescans also
pass on the OS segment candidate, preserving catalog/root/files and normal
shutdown: artifacts/ui-parity-20261008T053045305Z. All processes are terminal.
