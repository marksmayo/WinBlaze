# Runtime adoption audit

Iteration21's candidate remains the eligible experimental binary:
4A0F08C463A08AF99E4BD5F4792CC4FF74841AA2F4D2CED81E8389EA70A7C6A6,
with native DLL9FF4B2B and canonical startup working set14.826171875MiB.
It is not adopted. Official35 results remain27gold/7silver/1bronze.

An audit compares candidate copies against current shipping source, accounting
for the explicit XAML projection change. Shared native bridge, file operations,
watcher, portable discovery, virtual disks, theme palette, byte formatting,
treemap geometry/PNG and startup logging helpers match. MainWindow differs in
the host integration and previously tested ownership/dispatcher changes.
App.cpp/main.cpp copies in the candidate folder are not the executable's entry
point; the build uses full-os-main.cpp. Source matching is not runtime parity.

The updater/render helper prefix of MainWindow.cpp is identical after namespace
projection and the explicit Application.Exit-to-owned-host-close substitution.
The check retains a normalized hash. This verifies source preservation, not an
end-to-end downloaded update. Shipping's declared minimum OS build is19041.
See artifacts/all-gold-loop/iteration-22/source-parity.json and integration-audit.json.

The audit found a directory-card floor difference,180 in shipping versus320 in
the candidate. A separate180 variant was built (UI26185DBE). At a restored
1000x1000 window, the empty directory list measures20px in shipping,124px in
the320 candidate and19px in the180 candidate. A test comparing these empty-list
heights is inadequate as an adoption criterion: it rewards matching a cramped
layout without proving folder operation. It is retained only as diagnostic
evidence, not a green functional gate.

The behavioral check requires an owned folder expand affordance to be visible
and inside the tree viewport before invoking it, then verifies expansion and
collapse. It FAILS against the180 variant and PASSES against iteration21's320
variant. Shipping also fails the new visibility assertion in this environment.
The larger floor is an intentional accommodation already documented by the
generator. The180 variant is rejected; neither shipping layout nor the eligible
candidate was changed. No performance cohort was collected for the rejected
variant. Evidence: tree-180.log, tree-320.log, tree-shipping.log and
visible-tree-comparison.json. This is actual behavioral RED/GREEN evidence;
matching empty-list height is not.

The first layout probe serialized infinite coordinates for an offscreen GPU
surface. Those initial files are not valid JSON evidence for a verifier.
The probe now records null coordinates for unavailable bounds; the subsequent
shipping-valid and candidate-before-valid cohorts are valid. No performance
observations were altered.

A fresh feature parity batch for the eligible iteration21 binary completed:
all nine checks passed (import/search, scan rules, invalid rules, metadata,
hardlinks, duplicates, history, volume space and storage costs). Per-test
hashes and logs are in candidate-feature-results.json. Remaining adoption work includes reproducible
product build/packaging, startup failure reporting, and input/window integration.
Interactive keyboard ownership prerequisites remain unavailable; other work
continues. No competitor tests or official medal refresh ran this iteration.
