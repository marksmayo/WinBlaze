# Static CRT startup resource experiment

The initial residency evidence showed 224 KiB in MSVCP140.dll, 60 KiB in
VCRUNTIME140.dll and 32 KiB in VCRUNTIME140_1.dll in the warm OS XAML snapshot.
The executable itself contributed only 124 KiB at that snapshot. These are
post-idle diagnostics, not exact medal-boundary attribution. They made a uniform
static CRT build a stronger hypothesis than optimizing application code alone.

`artifacts/all-gold-loop/iteration-11/prepare-static-crt.ps1` copies the complete
current UI source, host helpers and identical native DLL to an isolated directory.
`build-static-crt.cmd` builds every UI translation unit with `/O2 /MT`, avoiding
mixed static/dynamic CRT object files. The primary candidate and shipping project
are unchanged. The Rust boundary exposes C views/callbacks rather than C++ STL
objects. Functional/native boundary checks remain required; this observation is
not complete ABI or adoption proof.

Build session 14754 completed. Static executable SHA:
`9C6050E3F81346E7B262C024A241BA33EE88B8773F6E8E9F6C11E879C0BAE1F0`.
Dynamic CRT reference SHA:
`9F01E8F25205E900CE18EAB0A27A63BA9D6BCB0F78ADB5A838C71F0AD6AF653A`.
Both contain the change-history lifetime fix described below.

## Matched observations

One-pair controls/runtime/normal-close preflight passed. Then ten alternating
launches per build passed five enabled controls, exact owned window/runtime,
identical native DLLs and normal exit with strict weak-window release assertion.
All observations, including the first launch, are retained in
`artifacts/all-gold-loop/iteration-9/full-ui-paired-830ff4a9123248ed80b708aa7ffc4bd4/{observations,summary}.json`.

| Median | Static CRT | Dynamic CRT |
|---|---:|---:|
| First-idle working set | 16.216796875 MiB | 16.4140625 MiB |
| First idle | 40.01675 ms | 37.9634 ms |
| CPU at first idle | 23.4375 ms | 31.25 ms |
| Initialized working set | 112.111328125 MiB | 112.31640625 MiB |

The matched reduction is 0.197265625 MiB. First-idle time was slightly higher in
this cohort, so this is a memory improvement hypothesis with an observed timing
tradeoff, not a universal performance improvement claim.

## Canonical resource gate

The unchanged canonical startup runner collected ten static CRT observations
with default appdata into
`artifacts/all-gold-loop/iteration-11/static-crt-startup-default-storage.json`.
Median working set is **16.244140625 MiB**, exactly equal to the current retained
WinDirStat best. `verify-startup-working-set.mjs` passes against that target with
all ten observations and verified current executable/native identities. This is
a resource GREEN, following the preceding candidate's actual RED above that
target. The original medal ranking gives equal exact values the same place.
There is little margin; subsequent WinBlaze results must remain latest, including
any worse result. No reruns were selected or discarded to manufacture this GREEN.

These observations remain isolated diagnostics. The official shipping report
still has 27 gold, 7 silver, 1 bronze and all 35 validated results. Adoption and
a fresh original-35 WinBlaze rerun are still required before updating medals.
Unchanged competitor bests remain frozen.

## Functional and ownership evidence

Normal static CRT checks pass: timestamped live change history; duplicate/ACL
exports; automatic watcher publication; move cancellation and success; confirmed
fixture recycling; catalog import and offline reopen; full-index search beyond
the preview cap; regex, allocation bounds and invalid-pattern feedback. These
tests exercise real UI/native paths but do not establish all candidate parity.

The change-history pending-dialog test reproduced exit 97/window retention on
dynamic candidate 5B683F2F (`artifacts/ui-parity-20261007T133948266Z`). Complete
method ownership and the owned-dialog close path fix it: dynamic reference
`artifacts/ui-parity-20261007T134322993Z` and static CRT
`artifacts/ui-parity-20261007T134643645Z` both exit 0 with release asserted.
The earlier menu-locator failure is retained separately and is not a lifecycle RED.

Four methods now have complete dialog ownership: permissions, multiple roots,
maintenance, change history. Remaining dialog/native-picker/updater lifetimes,
input, caption accessibility, theme/device behavior and adoption checks remain.
No shipping build change, complete repository build/coverage gate, or all-gold
completion is claimed.

## Latest import ownership build: resource RED

The earlier exact-tie GREEN belongs to binary `9C6050E3` only. The current
static CRT binary `89724031` adds complete ImportCsv coroutine ownership and
the owned timestamp-options dialog. Closing that pending dialog reproduced
exit 97/window retention before the fix (`ui-parity-20261007T135901891Z`),
then passed with release asserted (`ui-parity-20261007T140514428Z`). Native
picker close, normal import/offline search and active-import close also pass.
The pending-dialog harness now retries only visible, enabled menu items in
the exact owned process; locator failures remain separate from lifecycle REDs.

The latest canonical ten-run result is **16.5859375 MiB**, above WinDirStat's
retained **16.244140625 MiB**. Evidence is
`artifacts/all-gold-loop/iteration-11/static-crt-import-startup-default-storage.json`.
The resource gate correctly returns RED. The older GREEN must not be carried
forward as this build's score. Sequential measurements do not prove that the
ImportCsv fix caused this increase.

A two-pair residency diagnostic (`full-ui-paired-c96e7ac4c19a43768894fcca04c58a56`)
still confirms removed dynamic CRT pages and increased executable residency.
It contains substantial launch variation; its lower warm observation does not
replace the failed canonical cohort. Snapshots occur after first idle and do
not establish exact first-idle allocation attribution.

Five methods now have complete dialog ownership, including ImportCsv.
The official report remains all 35 results, 27 gold/7 silver/1 bronze.

## Whole-program optimization experiment rejected

A separate copy uses `/O2 /GL /MT` for all three translation units and
`/LTCG /OPT:REF /OPT:ICF` at link. The reference executable is preserved.
Build and one-pair controls/runtime/release preflight pass. Ten alternating
launches per build retain all observations in
`full-ui-paired-c165f76208964e93afca2edcb9f39da6/{observations,summary}.json`.

| Median | Whole-program optimized DF01B2FE | Reference 89724031 |
|---|---:|---:|
| First-idle working set (MiB) | 16.244140625 | 16.2265625 |
| First idle (ms) | 41.79495 | 42.58395 |
| CPU at first idle (ms) | 31.25 | 23.4375 |
| Initialized working set (MiB) | 111.85546875 | 112.099609375 |

All twenty normal exits pass strict window-release assertion. This experiment
does not improve first-idle memory in the matched cohort and is rejected for
the current target. The matched reference value does not supersede its latest
canonical RED. No official report values change.

## Size optimization experiment rejected

A separate `/O1 /MT` build passes preflight and twenty alternating normal
launches with strict release assertion. All observations are retained in
`full-ui-paired-194d6706228948468c1c13aa8ae540ca/{observations,summary}.json`.
Size build `228DFA82` uses median **16.23046875 MiB** versus **16.216796875 MiB**
for reference `89724031`. Idle medians are 40.24405/43.81105 ms; CPU is 31.25 ms
for both. There is no matched first-idle memory reduction, so this build is
also rejected for the memory target. A lower diagnostic reference median
does not replace its canonical result.

## Storage-cost ownership red/green

Closing the configuration dialog reproduces exit 97/window retention on
89724031 (`ui-parity-20261007T205731846Z`). The complete method lease and owned
configuration/result dialogs fix it in A5867D27. Configuration close passes
with window release asserted (`ui-parity-20261007T205944932Z`), as does result
close (`ui-parity-20261007T210007156Z`). Normal cost estimation still classifies
the fixture into the configured cold tier. Six methods now have complete
dialog ownership. Remaining methods and other adoption prerequisites persist.

The new build's unchanged canonical ten-run startup result is **16.8125 MiB**
versus WinDirStat **16.244140625 MiB**, an actual resource RED, saved in
`static-crt-storage-startup-default-storage.json`. All ten observations and
the executable/native identity are retained. No earlier better WinBlaze result
is substituted; neither this candidate nor the report earns a new gold medal.
The reference 897 executable and DLL were archived before this rebuild under
`iteration-11/static-crt-897-before-storage-ownership`.

## PNG export ownership red/green

The completed shipping iteration-11 report is now 29 gold/5 silver/1 bronze,
with all 35 categories validated. The OS XAML candidate remains isolated.

The pending PNG-options test initially could not reach the offscreen menu
item (`ui-parity-20261007T221306440Z`); that is a locator failure, not a
lifecycle result. Its owned ScrollItemPattern brings the item into view while
retaining the exact-process, visible, enabled invocation checks. The actual
pending-dialog test then reproduces exit 97/window retention on A5867D27
(`ui-parity-20261007T221409397Z`). No global keyboard input is used.

The complete ExportTreemapPng method now holds an admitted coroutine lease
and uses the existing owned-dialog close path. Static build session 82409
exits 0. New executable 4C32C1FA passes pending-options close with normal
exit and release asserted (`ui-parity-20261007T221638929Z`). The native DLL
remains ECFE3C40, matching the reference. The A586 executable and DLL were
preserved in `static-crt-A586-before-png-ownership`.

The complete import-scan test (session 27535, exit 0) also passes on this
candidate: 9,001 offline entries, allocation/timestamp fidelity in JSON,
1920x1080 PNG output with dense coverage, and cleanup pagination. Seven
methods now have complete dialog ownership. Other dialog/native-picker,
updater, keyboard/accessibility, theme/device and adoption checks remain.

The new build's canonical ten-run first-idle memory is **16.873046875 MiB**
against retained WinDirStat **16.244140625 MiB**. The original resource gate
returns RED. All observations and identities are retained in
`static-crt-png-startup-default-storage.json`; the first 24.50-MiB observation
is included. Earlier favorable measurements are not substituted. Sequential
measurements do not prove the ownership change caused the difference. No
candidate adoption or official startup gold is claimed.
