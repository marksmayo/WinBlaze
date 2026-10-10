# Bitmap icon startup experiment

The icon-corrected iteration-16 candidate failed the unchanged ten-sample
startup working-set target at 16.455078125 MiB versus 16.244140625 MiB.
The next isolated candidate changes only icon resource encoding: sizes
16,24,32,48,64 use 32-bit bitmap data and transparency masks; the original
128/256 PNG payloads remain intact. Runtime icon setup, DPI awareness,
full UI source and native DLL are unchanged. Required initialization is
not deferred. Source/master icon and production binaries are untouched.

The saved encoder artifacts/all-gold-loop/iteration-17/encode-dib-icons.ps1
reproduces the measured encoded asset byte-for-byte. The public Windows
icon decoder renders identical pixel hashes for **all seven sizes**;
verify-icon-encoding.ps1 and icon-pixel-fidelity.json retain the evidence.
Run that verifier with powershell.exe, matching the existing UI test host.
An initial PowerShell 7 helper compilation failed due to assembly references;
its console success text was invalid and is not accepted evidence. The
corrected verifier stops on any failure and requires all seven observations.

The initialized window-icon regression also passes against the iteration-16
PNG executable's resource (ui-icons-20261008T042101229Z). Both actual small
and taskbar icon pixels match; normal close exits0. Build18739 completed0.
New UI SHA-256 is998370A1E718DB147D0976D13C45607C8A14D3333EBAFEC6CC4C6016E3097242;
native remains9FF4B2B. Sources/build/asset are in iteration-17/dib-host.

One canonical default-storage n10 measurement remains **RED**:
16.39453125 MiB versus16.244140625, a154KiB gap. Median first idle35.46ms,
CPU31.25ms. Every sample is retained, including24.3046875MiB first sample,
in dib-startup-default-storage.json; computed gate/identity are in
dib-resource-gate.json. The lower result than iteration16 is an observation,
not a controlled causal improvement or a new official medal. No unchanged
binary retry or historical-best substitution was used.

Matched alternating n10 per host, with post-idle residency, DPI and full
control checks, completed at iteration-9/full-ui-paired-
c83e1f5d3c5949309a87e061eb685402. Bitmap/PNG first-idle medians are
16.353515625/17.537109375 MiB; initialized medians126.953125/128.31640625.
All20 normal exits, five controls, owner and PMv2 checks pass. Snapshots
occur after first-idle counters while initialization continues. The previous
matched snapshot cohort did not attribute resident pages to a codecs DLL;
there is no proven separate decoder-module saving.

Candidate remains unadopted. Official all35 results remain27gold7silver1bronze;
competitor bests remain unchanged. Startup working set is still the sole
official bronze. Further memory reduction and complete UI parity are required.
