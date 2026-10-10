# Compact-record process evidence and startup target

The completed iteration-14 report has **27 gold, 7 silver and 1 bronze**.
All 35 original categories have fresh WinBlaze n=10 evidence. All 490
feature observations validate; definition, freshness, sample count, report,
focus and publication checks pass. All 70 competitor best entries are
identical to iteration 13, including their archived source evidence.
The retained administrator worker reuses comparator baselines with no UAC.

Standard-user full-drive private memory is now **gold at 432.060546875 MiB**,
below WinDirStat's retained 444.83984375 MiB. The previous WinBlaze median
was 473.5 MiB. All ten CLI peak working sets are 419.3–422.4 MiB versus
the preceding cohort's roughly 460–463 MiB. File counts are live and slightly
higher (about 1.799 million), so this is process evidence consistent with
the 24-byte record reduction, not a perfectly controlled filesystem pair.
Snapshot, path, name, timestamp and C ABI fidelity checks pass.

The slower first elapsed observation (34,981 ms) is retained. Full-drive
variability worsens to 14.793058279343416%, still silver. Startup first idle
is now silver at 70.745 ms versus WinDirStat 66.605 ms; startup CPU is
101.5625 ms, also silver. Startup working set remains the bronze at
32.857421875 MiB versus 16.244140625 MiB. Peak startup working set and all
three folder-variability rows remain silver. These latest values replace
WinBlaze's earlier observations. The all-gold gate remains RED.

Final report evidence is runs/2026-10-07T23-50-07-948Z/report-data.json and
artifacts/all-gold-loop/iteration-14/final-validation.json. Shipping CLI is
D178578F, DLL 9FF4B2B, UI AA3F83A5. The full local gate remains RED because
the unchanged tiny UI budget fails at 2,425 ms against 2,000 ms; native and
UI functional checks pass. All build/medal processes are terminal.

The isolated full OS XAML UI candidate remains separate from shipping.
Its 2F7B9406 UI executable is preserved with old ECFE3C40 native backend in
iteration-14/static-host-backend-reference; the candidate now uses validated
9FF4B2B. A backend-only diagnostic mode explicitly requires identical UI
hashes, distinct native hashes and both OS XAML hosts. Ordinary host
comparison still requires identical native backends. End-of-cohort hashes,
five initialized controls, exact runtime and owned normal shutdown remain
checked. Failed preflights are separate from measurement cohorts.

Matched ten-pair cohort e9aba367315b4177884c6e3c62879b09 passes all 20 normal
shutdown and initialized-control checks. New/old medians are 16.6171875 /
16.8515625 MiB at first idle; 31.87305 / 31.08505 ms; 15.625 / 31.25 ms CPU;
111.9453125 / 111.890625 MiB at initialized controls. These are diagnostic
observations, not proof of backend causality or official medal results.

Resident-page snapshots immediately after first idle show **no resident
native DLL pages in any of the 20 snapshots**. COM/OS XAML activation dominates
the attributed early footprint; app initialization continues while the
snapshot is captured. The native-record change cannot be credited with a
causal early startup reduction from this evidence. Candidate DPI setup is
currently necessary: its embedded manifest contains no DPI-awareness
declaration, so deleting the existing setup call would change behavior.

One canonical default-storage n=10 startup measurement for the changed
candidate is RED: **16.251953125 MiB versus 16.244140625 MiB**, an 8-KiB gap.
All ten raw observations and the combined UI/backend identity are retained
in static-host-compact-backend-startup-default-storage.json and
static-host-compact-backend-resource-gate.json. No favorable unchanged-binary
retry or historical-best substitution is allowed. The candidate is not
adopted; complete runtime/feature/ownership/input/theme/device/updater parity
and a measured resource margin remain required before shipping migration.

For the separate tiny UI provider delay, Microsoft's documented Invoke
contract requires prompt asynchronous return. This is a diagnostic lead,
not evidence that WinBlaze's provider is the cause or that a proposed change
passes the budget. See [IInvokeProvider::Invoke](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/nf-uiautomationcore-iinvokeprovider-invoke)
and [ButtonAutomationPeer.Invoke](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.automation.peers.buttonautomationpeer.invoke?view=windows-app-sdk-1.8).
