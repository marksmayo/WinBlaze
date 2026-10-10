# Byte-label allocation red/green

Startup/runtime summaries and catalog/treemap labels repeatedly format byte
counts. The original formatter constructs a wide stream for each label. Its
unchanged logic was first exposed through the small FormatByteSize helper;
the resource test then failed with exit 3 on a compiled native executable.

The public test checks explicit unit boundaries and compares 23,560 outputs
with the original formatter, including every byte value through 1024,
threshold neighborhoods, uint64 maximum and deterministic full-range values.
It measures allocations only while producing the labels, after reference
formatting has warmed its runtime. The budget allows one returned-text
allocation per label and 64 bytes per label, without a temporary stream buffer.

| Evidence | Original stream | Character conversion |
|---|---:|---:|
| Labels | 23,560 | 23,560 |
| Output characters | 298,411 | 298,411 |
| Allocated bytes | 2,206,512 | 698,672 |
| Allocation calls | 41,993 | 18,433 |
| Resource gate | RED, exit 3 | GREEN, exit 0 |

The helper uses fixed-precision character conversion, preserving the original
double conversion, thresholds and classic decimal text. A stack buffer holds
the digits; one correctly sized string holds the result. Native compilation
passes warning level 4 with warnings treated as errors. MainWindow delegates
to the helper, and the native contract is included in check-local.

The first direct MSBuild invocation failed because its inherited environment
contained both PATH and Path. Windows PowerShell normalizes that environment;
the subsequent native build succeeds. An initial wrapper quoting error is
separate from the actual resource RED, which was rerun directly and returned 3.

The full-source static CRT candidate builds as 2F7B9406 with unchanged ECFE
native backend. Reference 4C32C1FA and its DLL are archived under
static-crt-4C32-before-byte-format. Twenty alternating launches retain all
observations in iteration-9/full-ui-paired-30d95b591a6c42f0ab467412ad72daf1.
Every launch verifies initialized controls, runtime identity and normal
window release. Median first-idle memory is 17.85547 MiB versus reference
18.36328 MiB, while CPU is 31.25 versus 23.4375 ms. Initialized CPU is equal
at 234.375 ms; observed initialized time is 224.68 versus 223.23 ms. This
does not establish a startup speed improvement or the required memory margin.

The new candidate's canonical ten-run startup memory is **17.400390625 MiB**
against retained WinDirStat **16.244140625 MiB**, so the resource gate remains
RED. Evidence is static-crt-byte-format-startup-default-storage.json with
its own identity history. No earlier better candidate value is substituted.
The helper's allocation reduction does not prove a process-memory reduction.

Shipping local validation (session 98399) exits 1, with output in
byte-format-local-check.log. Rust format/clippy/tests/examples, the release
WinUI build, native contracts, all ten path/window cases, smoke, parity,
offline import/export and negative UI checks pass. The unchanged tiny UI
timing budget fails at **2445 ms versus 2000 ms**. The corrected local runner
propagates that failure. This is not a passed local gate.

Shipping UI identity is AA3F83A5; native DLL F1C65B06 and CLI A3B44E52 are
unchanged. The original-35 WinBlaze rerun is active in session 37170, batch
20261007T223406019Z. Competitors remain frozen for their complete matching
versions. All 35 rows stay visible, with old accepted WinBlaze rows retained
until each replacement cohort finishes. Partial latest results currently
show 27 gold/7 silver/1 bronze: empty-file and full-volume variability move
back to silver. Earlier better WinBlaze values are not restored.

The first native full-volume workflow retains its 24.775-s/660.3-MiB
observation; later observations are around 12-14 s/460-462 MiB. No native CLI
code changed in this slice, so that difference is not attributed to the UI
formatter. Inspection identifies a next hypothesis: capacity hints already
include 5% growth room but persist only one root/rules scope. Alternating
roots overwrites that advisory estimate. A bounded cache of recent scopes
may avoid first-run vector growth when returning to a previously scanned
volume; it needs public red/green and process evidence before adoption.
