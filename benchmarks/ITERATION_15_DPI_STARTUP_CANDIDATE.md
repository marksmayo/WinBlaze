# DPI manifest startup candidate

This isolated full-source OS XAML candidate declares PerMonitorV2 awareness
in its embedded manifest instead of calling SetProcessDpiAwarenessContext
at startup. Production UI and the official 35 medal results are unchanged.
Microsoft recommends declaring process awareness in the manifest:
[default DPI awareness](https://learn.microsoft.com/en-us/windows/win32/hidpi/setting-the-default-dpi-awareness-for-a-process).

Candidate source and build command are retained under
artifacts/all-gold-loop/iteration-15/dpi-host. UI SHA-256 is
5CAADC980317FF674224470801891B22E3840B45719A2E3ED807EC4361811B1C;
native DLL SHA-256 is
9FF4B2BAC5DAFD0160FF06874D21528044C7EDC2B9E1F2AC25996808834A9F62.
The reference UI is 2F7B9406 with the same native DLL and build flags.

The preflight and ten matched pairs verify actual owned-window
PerMonitorV2 awareness, positive window DPI, five initialized controls,
the exact OS XAML runtime and normal exit. The ten-pair cohort is
iteration-9/full-ui-paired-7a35daf17d51478f8434539a2e57b770.
Candidate/reference first-idle medians are 16.248046875/17.115234375 MiB,
30.1144/28.46095 ms and 31.25/15.625 ms CPU. Initialized working sets
are 111.974609375/111.884765625 MiB. These paired diagnostics do not
prove a causal memory benefit or constitute official medal results.

One canonical default-storage ten-sample cohort for the changed binary
passes the working-set target: **16.22265625 MiB versus WinDirStat's
retained 16.244140625 MiB**, a narrow 22-KiB margin. First-idle median
is 41.87 ms and CPU median 31.25 ms. All ten samples, including the
22.49609375-MiB first sample, are retained in
iteration-15/dpi-startup-default-storage.json. The computed gate and
identity are retained in dpi-resource-gate.json. This is the latest
candidate cohort; no historical-best substitution or unchanged-binary
retry was used. The previous 2F7B9406/9FF candidate's 16.251953125-MiB
failure remains recorded separately.

The first attempt to start this canonical cohort was rejected because
automatic approval review hit its usage limit, not because the action
was considered unsafe. A later review approved the same requested
benchmark, which completed successfully. No approval check was bypassed.

The candidate is not adopted. Remaining full UI, ownership, picker,
input, theme, device and updater parity must be assessed before shipping
migration. The unchanged 2000-ms tiny UI budget also remains unresolved.
Official standings remain 27 gold, 7 silver and 1 bronze across all 35
results. Competitor bests remain frozen for their existing identities.

Further candidate validation completed on the same 5CAADC98 binary:
- tests/ui/parity.ps1 passed duplicate/ACL exports, automatic watcher
  publication, move cancellation/success and confirmed recycling. Evidence:
  artifacts/ui-parity-20261008T030616058Z.
- Four additional pending-dialog close cases passed normal exit:
  permissions-results (030714612Z), maintenance-confirmation (030724339Z),
  storage-cost-results (030734245Z), import-native (030746622Z). Each folder
  retains dialog-close-result.json and the observed binary identity.
- a11y-audit.ps1 passed 21/21 named controls and 20/20 enabled focusable
  client controls. No caption buttons were exposed, so caption UIA
  invocation and broader accessibility remain unverified.
- Owned native minimize/restore/maximize/close passed, recorded in
  iteration-15/native-caption-result.json. This does not resolve caption
  UIA accessibility.
- parity-portable-devices.ps1 failed to find Portable device selection;
  evidence fixture is artifacts/ui-parity-20261008T030823372Z. Source
  returns before that dialog when discovery finds no devices or errors.
  That explanation remains a hypothesis: the actual discovery result
  was not captured. Do not treat this as a successful device parity check
  or change the original assertion. Next step is to capture discovery
  status and assess device-selection cancellation with a suitable device.

All processes from these checks completed; no candidate was shipped and
no new official medal cohort was started.

The owned portable-discovery diagnostic captured the actual result:
No portable device found. Connect and unlock a phone in USB file-transfer mode.
Normal close passed; evidence artifacts/ui-parity-20261008T032814946Z/
portable-discovery-result.json. This confirms the missing-device branch,
not device-selection or attached-device cancellation parity. The original
portable-device assertion is unchanged. The diagnostic source is retained
as iteration-15/observe-portable-discovery.ps1.

Three separate Full C: diagnostic probes of shipping CLI D178578F completed
with optional existing phase telemetry; official original-35 values remain
unchanged because this is a predeclared three-sample diagnostic, not a
replacement ten-sample medal cohort. Per-run raw phases, observations and
full-phase-summary.json are retained under iteration-15. The before-terminal
stage accounts for most elapsed variation, while export remains around
three-quarters of a second. This directs the next performance investigation
to traversal/indexing rather than export. Counts differ slightly on the
live disk; there is no controlled causal performance claim.

The goal service reports usageLimited. The objective remains unachieved;
no complete or blocked status was asserted and no automation was created
as a workaround. All diagnostic processes are terminal.

Standalone no-device UI coverage is now tests/ui/parity-portable-no-device.ps1,
with usage documented in tests/ui/README.md. It requires the exact no-device
message and rejects discovery errors or an attached-device dialog. Each
repeat clears prior status, preserving catalog hashes, scan root and fixture
contents while asserting restored scan controls and normal owner-checked
shutdown. No global keyboard input is used. The original attached-device
parity script is unchanged.

Both builds pass three repeated existing-catalog discoveries with three
newly published, distinct fixture catalogs, proving actual rescans after
no-device discovery. Candidate evidence: ui-parity-20261008T033646406Z;
shipping evidence: ui-parity-20261008T033720337Z. Summary:
iteration-15/no-device-rescan-summary.json. Both also pass the final strict
empty-catalog case (candidate033811378Z, shipping033818461Z), with no catalog
created. Earlier two-repeat empty cases also passed. PowerShell syntax and
diff whitespace checks pass. Only UI test/documentation changes were made;
no production behavior, benchmark definition or official medal row changed.
Attached-device selection/scanning/cancellation remains unverified.
