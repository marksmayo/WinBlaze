# Startup working set investigation

The full three-product rerun completed on 8 October 2026 (Sydney). All 35
categories have accepted n=10 evidence: WinBlaze 27 gold, 7 silver, 1 bronze.
The only bronze is startup working set at first idle: shipping WinBlaze
34.3203125 MiB versus retained WinDirStat 16.244140625 MiB. Definitions and
observation boundaries remain unchanged. No product binary was changed.

## Current isolated candidate

Fresh alternating paired launches, ten per host, empty isolated appdata:
`artifacts/all-gold-loop/iteration-9/full-ui-paired-080334ebab2e47ae8987db30c10677e5/observations.json`.

| Host | First-idle working set | First idle | CPU at first idle | Initialized working set |
|---|---:|---:|---:|---:|
| OS XAML candidate F8E0818E | 16.46484375 MiB | 38.66195 ms | 31.25 ms | 112.1171875 MiB |
| Shipping WinUI 675F52F7 | 32.62890625 MiB | 54.4064 ms | 62.5 ms | 140.259765625 MiB |

These are medians. All twenty observations verify five enabled controls, the
exact owning window/runtime, unchanged identical native DLLs, and normal exit 0.
They do not establish keyboard, caption accessibility, real portable-device,
pending-dialog lifetime, or complete adoption parity.

The candidate was also tested with the existing canonical startup runner,
default appdata, ten observations, and separate output/history paths:
`artifacts/all-gold-loop/iteration-10/candidate-startup-default-storage.json`.
Its working-set median is 16.4765625 MiB. The resource regression gate
`artifacts/all-gold-loop/iteration-10/verify-startup-working-set.mjs` is RED
for shipping and candidate against the current retained WinDirStat baseline.
The candidate misses by 0.232421875 MiB; no gold or production improvement is
claimed. These candidate observations never enter the official report.

## Resident-page attribution

Two additional diagnostic pairs:
`artifacts/all-gold-loop/iteration-9/full-ui-paired-6f56d1755c2845fc807fb1dc6ea62cbd/observations.json`.
The QueryWorkingSet decoder matches the local Windows SDK PSAPI_WORKING_SET_BLOCK
layout. The page snapshot starts 4-9 ms after the first-idle counter and the app
continues initializing, so it must not be presented as exact first-idle residency.
Module ranges are enumerated after the page snapshot.

Shipping snapshots include about 3.0-3.2 MiB of resident Intel igc64.dll pages
and 2.8-2.9 MiB of Microsoft.UI.Xaml.dll. Warm candidate snapshot includes about
0.88 MiB of Windows.UI.Xaml.dll and 1.57 MiB of unattributed pages. The first
candidate snapshot has much greater shared runtime residency than its warm
snapshot. This provides leads for runtime initialization analysis; it does not
prove a particular allocation or GPU setting caused the official bronze.

Next work should reduce real initialized memory and close candidate functional
parity gaps. Do not trim the working set, postpone initialization solely past
first idle, remove measurement overhead, or change medal scope to pass the gate.

## Competitor reuse policy

Subsequent loop iterations refresh WinBlaze. Complete competitor bests are reused
until version/hash identity changes or archived baseline evidence is unavailable.
The planner selects only WinBlaze against the completed current baseline.
Fixtures cover version/hash changes, invalid counts, missing sources, explicit
full refresh, and UTF-8 definition names. A retained-worker entry-point check
confirms both unchanged competitors are skipped and elevated results unchanged.
WinBlaze import tests may still generate competitor export input fixtures outside
the timed competitor workloads.

A test initially passed an unsupported script parameter, which Windows PowerShell
accepted as an unused argument and launched an unintended competitor run. The
test process and its own children were stopped; elevated result JSON was restored
from the validated iteration-10 archive. The script now has strict parameter
binding and an explicit batch-policy input; the corrected skip test passes.

The completed rerun's report check originally hard-coded WinDirStat 2.8.0. It now
checks the recorded current version (2.9.2) for each competitor. Freshness,
original definitions, report rendering/focus, live publication, sample counts,
and all 490 feature observations pass. The all-gold gate still fails as expected.
No broad Rust/UI build, coverage, or adoption gate is claimed for these harness
and diagnostic changes. The overall optimization goal remains active.

## Candidate ownership progress, 8 October

The candidate now asserts that a weak window reference has expired after its
shutdown guard finishes and its owning pointer is released. This exposed real
window retention during pending permissions and multiple-root dialogs (exit 97),
which the earlier ordinary exit-0 checks could not detect.

The isolated adapter now hides its owned dialog on close and tracks its async
completion. Complete method leases cover the permissions and multiple-root
coroutines, including their user dialogs. Their observed RED-GREEN evidence:

| Case | RED artifact | GREEN artifact |
|---|---|---|
| Permissions options | ui-parity-20261007T130832495Z | ui-parity-20261007T131247381Z |
| Permissions results | Not a separate RED; additional coverage | ui-parity-20261007T131436428Z |
| Multiple roots | ui-parity-20261007T131518941Z | ui-parity-20261007T131756436Z |

Artifacts are under `artifacts/`, with dialog-close-result.json recording the
actual executable hash, observed pending dialog and exit status. Normal
permission reading/filtering passes. Normal multi-root scanning passes on
shipping and the changed candidate; an earlier intermittent menu-locator failure
remains recorded and is not mislabeled as a lifetime failure.

Current candidate SHA A14C44464D58155241947A0AEB9A968110C4E372EBF383716905F5FF842A3CF4.
Fresh matched n=10 cohort `full-ui-paired-c8722ee5e38a40c38e493e0db8b48cd8`:
candidate 16.44140625 MiB first idle, shipping 32.423828125 MiB; all twenty normal
exits pass, including the candidate's stricter release assertion. Canonical
default-appdata startup n=10 gives 16.435546875 MiB, still RED against retained
WinDirStat 16.244140625 MiB. Its evidence is
`artifacts/all-gold-loop/iteration-10/candidate-A14-startup-default-storage.json`.
Small differences from the preceding candidate cohort are not attributed to
these lifecycle fixes as a performance gain.

The maintenance dialog reproduces exit 97 on A14
(`ui-parity-20261007T132208559Z`), providing the next actual RED. Other dialog,
input/accessibility/theme/device and adoption checks remain incomplete. The
candidate remains isolated; the shipping binary and official 35 values are
unchanged. The fresh ranked 100 hypotheses are in
`ALL_GOLD_LOOP_ITERATION_11_PLAN.md`, with startup investigation prioritized.
