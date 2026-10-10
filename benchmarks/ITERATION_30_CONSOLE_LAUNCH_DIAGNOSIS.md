# Hidden-launch subsystem diagnosis and terminal compatibility

The next investigation follows unchanged official iteration28 results:
32 gold,3 variability silvers,0 bronze; startup CPU remains pending.
No product binary or official report observation changes in this experiment.

Two isolated copies of current CLI D576E8F1 differ only in PE headers: subsystem
3 (console) versus2 (Windows). All raw section payloads, including code and
data, are byte-for-byte identical. The Windows copy is an experiment, not a
supported or staged product launcher.

Twenty alternating observations reuse the actual hidden Start-Process launch,
whole-process timer and25ms resource sampling code in an artifact namespace.
The diagnostic adds a retained handle and mandatory zero exit status. Every
observation validates the exact72-file root and all output; all initial outliers
remain included. Competitors are not launched and official observations remain
unchanged.

| Diagnostic | Console subsystem | Windows subsystem |
|---|---:|---:|
| Whole-process median ms | 54.18 | 22.865 |
| IQR / median percent | 12.758398 | 14.552810 |
| Valid zero-exit exports | 10 | 10 |

The Windows copy wins all10 paired wall-time observations, but its relative
variability is worse. The launch-speed benefit does not imply a medal win.

## Observed terminal regression

Owned hidden interactive CMD processes receive commands through their own
standard input; no desktop keystrokes are used. The console copy emits
completion JSON before CMD's following marker and propagates usage error2.
The Windows copy emits the following CMD marker before completion and CMD
reports usage error0. This is a real compatibility failure: changing the existing
CLI's subsystem cannot be adopted unchanged.

A compatible product option to investigate is a separately built and packaged
unattended launcher sharing the complete CLI implementation, while preserving
the existing console CLI and its synchronous terminal/error contract. It must
be a supported product capability with complete output/error/redirected-I/O
validation, not an artifact-only benchmark tool. Any measured product version
must retain the existing workload, timing boundary and sampling definitions.
The relative-variability result must remain latest even if a faster launcher
worsens its percentage. No such product addition is implemented yet.

Evidence under artifacts/all-gold-loop/iteration-30: pe-identity-proof.json,
console-results.json, window-results.json, pe-subsystem-comparison.log,
terminal-parity.json and terminal-parity.log. Build/staging/package sites are
scripts/build-os-xaml-ui.ps1, stage-os-xaml-ui.ps1, package-portable.ps1,
package-installer.ps1 and installer/WinBlaze.wxs. Current workflow/feature runners
select target/release/winblaze-cli.exe; any supported alternative must have
explicit identities and retain complete exported data and measurement scope.
