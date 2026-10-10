# Startup candidate icon parity

The sole official bronze remains startup working set at first idle:
32.857421875 MiB for shipping WinBlaze versus retained WinDirStat
16.244140625 MiB. The isolated iteration-15 candidate measured
16.22265625 MiB in its canonical ten-run cohort but remains unadopted.

Further unchanged UI checks on that candidate pass window sizing, version
display, expansion, guidance, folder-picker cancellation and offline external
CSV reopening/export. These checks do not establish complete runtime parity.

The new tests/ui/window-icons.ps1 exercises the initialized, exact-owned
WinBlaze window. It checks small and taskbar icon pixels against resource 1
embedded in the observed executable, at the actual displayed dimensions,
and requires normal owner-checked shutdown. The asset comparison uses the
executable's own resource because the source asset and scaling can differ
from an older built executable. Early probes which selected a window before
initialization or compared against the current source asset are not valid
candidate-regression evidence.

With the corrected observable boundary, shipping AA3F83A5 passes
(artifacts/ui-icons-20261008T041242138Z/result.json); candidate 5CAADC98
fails **Small window icon missing**
(artifacts/ui-icons-20261008T041248309Z/failure.json). This establishes an
actual parity RED before implementation.

Iteration-16/icon-host is a separate copy of the candidate source with the
same native DLL. It embeds the repository WinBlaze icon and assigns both
native window icons before initializing the XAML island. Owned icon handles
are released on teardown and constructor failure. This adds required visible
behavior; no initialization is deferred beyond the measurement boundary.
The earlier candidate and all its failed/passed resource observations remain
intact. Build, icon GREEN and a fresh canonical resource gate must complete
before this candidate can be assessed. No shipping or medal change is made.

Build37917 completed exit0. Corrected candidate SHA-256:
8D0481F0417673B3CCECA53FA1D7B39B03E5162C997BBF4856E26B6ABEBAD8F1.
Native DLL remains 9FF4B2B. Icon regression GREEN with normal exit:
artifacts/ui-icons-20261008T041512062Z/result.json.

One canonical default-storage n10 cohort for this changed binary remains
RED: **16.455078125 MiB versus 16.244140625 MiB**, a216KiB gap. Median
first idle38.545ms, CPU31.25ms. Raw ten samples, including24.31640625MiB
first sample, identity and computed gate are in iteration-16/
icon-startup-default-storage.json and icon-resource-gate.json. Earlier
manifest-only resource GREEN is historical, not the latest corrected
candidate value. There is no unchanged-binary favorable retry.

Matched alternating n10-per-host diagnostics with residency and DPI checks
completed: iteration-9/full-ui-paired-5352078e6bba4a64ad5df3be20e4df1f.
All five controls, native owner, PMv2 and normal exits pass. Raw observations
and computed summary are retained. These snapshots occur after original
first-idle counters while initialization continues; they do not prove icon
causality or replace canonical medals. Production remains unchanged; full
candidate adoption and allgold remain unachieved.
