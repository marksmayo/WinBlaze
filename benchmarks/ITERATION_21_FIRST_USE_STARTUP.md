# Restore first-use linking in the OS XAML candidate

The experimental command-line build omitted the shipping project's first-use
linker policy for SHELL32, d3d11, d2d1 and DWrite. Restoring those four settings
with delayimp.lib changes no application function, initialization boundary,
control construction or feature. Microsoft documents the mechanism in
[/DELAYLOAD](https://learn.microsoft.com/en-us/cpp/build/reference/delayload-delay-load-import?view=msvc-170).

The actual PE import test fails against iteration20 because d3d11 is eager,
then passes for all four DLLs after relinking. All three compiled object files,
resource input, manifest and native DLL are byte-identical to iteration20.
The full build command and actual relink command are retained. The unchanged
source's error-log paths still refer to iteration20.

One fresh canonical ten-run cohort passes the unchanged startup resource
verifier: **14.826171875 MiB** working set and peak, versus retained WinDirStat
**16.244140625 MiB**. The margin is **1.41796875 MiB**. CPU median is31.25ms
versus62.5ms; first-idle median43.475ms. All samples, including first
23.21484375MiB, remain. There are no substituted samples, unchanged-binary
retries, competitor reruns or earlier candidate best substitutions.

Candidate UI SHA256 is
4A0F08C463A08AF99E4BD5F4792CC4FF74841AA2F4D2CED81E8389EA70A7C6A6;
native DLL remains
9FF4B2BAC5DAFD0160FF06874D21528044C7EDC2B9E1F2AC25996808834A9F62.
Evidence lives under artifacts/all-gold-loop/iteration-21, including
identical-object-inputs.json, eager-import-red.log, first-use-import-green.log,
first-use-startup-default-storage.json and first-use-resource-verifier.log.

Matched20 diagnostic launches pass all five initialized controls, exact OS
runtime, owner, PMv2 and normal release. First-idle medians are14.80078125MiB
for this host and16.220703125MiB for iteration20. All ten new snapshots lack
attributed resident pages for the three graphics DLLs; all ten references
include them. Shell32/dxgi appear in one new snapshot and all references.
Snapshots occur after first-idle counters while initialization continues;
they do not independently establish exact boundary attribution or usable
window time. Cohort: iteration-9/full-ui-paired-5bfe3301d5b142f4853192aa3491362c.

Icon pixels pass against iteration20 (ui-icons-20261008T071940653Z).
The unchanged9001-file offline CSV import, complete JSON export, normalized
timestamp,1920x1080 PNG, dense treemap coverage and cleanup pagination test
passes (ui-export-features-20261008T072003435Z). That existing test's cleanup
does not assert normal exit, so its PASS is not additional shutdown evidence.
The full file-operation test also passes: duplicate/ACL exports, automatic
watcher publication, move cancellation/success and confirmed recycle.

The owned Tab probe stops on the shipping reference because global UIA focus
does not establish ownership, before posting any key. No focus guard is
weakened and no candidate regression is established. Interactive keyboard
parity remains unverified. Caption accessibility, theme, updater and
attached-device parity remain adoption work.

The candidate is **not adopted**. Official35 results remain27gold/7silver/1bronze,
including shipping startup working set32.857421875MiB; all70 competitor best
objects are unchanged. No production source changed. The all-gold objective
remains unachieved.
