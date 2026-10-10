# Build and package the alternative host from repository sources

The opt-in OS XAML UI now has repository-owned host sources, a current-source
projection script, build discovery, icon encoding and portable packaging.
It no longer depends on sources or object files from an old experiment folder.
The default WinUI project and official benchmark binaries remain unchanged.

The build contract went RED before the public builder existed, then GREEN with
a fresh compile of the full UI, native DLL, CLI and updater. The inventory
records source inputs, discovered SDK/header versions and all four binary hashes.
UI SHA256 is 7C751BDC682E491E74A42E546EDBE0C0EB615E0F76CA344214C6B7A0F13CFABC;
native DLL remains 9FF4B2B and CLI D178578F. Twenty paired launches verify five
initialized controls, owned windows, exact OS runtime, PMv2 and normal release.

The portable contract went RED because the old packager selected the default
WinUI executable instead of the requested candidate. It now verifies the exact
four-binary inventory, stages a separately named OS XAML ZIP, and passes the
extracted CLI fixture scan and extracted UI icon/normal-shutdown checks.
PowerShell 5 packaging passes for both explicit OS XAML and default WinUI paths;
the latter retains its UI identity and required PRI. Path guards reject escaped
build outputs and escaped package cleanup targets before writes/deletion.

The stronger visible-tree contract went RED for the rejected iteration22
180-pixel layout and GREEN for this build's retained 320-pixel layout. It checks
the owned expand affordance's bounds inside the actual viewport before invoking
it. Empty-list height matching alone remains insufficient functional evidence.

One canonical ten-run startup cohort measures WS/peak 14.849609375 MiB, below
retained WinDirStat 16.244140625 MiB. First-idle median is 31.95 ms; raw CPU median
23.4375 ms. All samples remain, including first WS 22.484375 MiB and CPU run10
zero. The original resource verifier is RED on its positive-observation
requirement; only nine CPU observations qualify for the unchanged medal builder.
No zero replacement, favorable retry or historical candidate best is used.

Evidence is under artifacts/all-gold-loop/iteration-24: build-contract*.log,
package-contract*.log, visible-tree-*.log, path-validation.json, build inventory,
repository-startup-default-storage.json and repository-resource-gate.json.
The paired cohort is iteration-9/full-ui-paired-8442396f9c914793818eb52ebbe461e7.

This candidate is not adopted. Input/window integration and remaining runtime
parity continue. All original35 official rows stay 27 gold, 7 silver, 1 bronze,
with all70 competitor best records unchanged. Build/package progress is not
reported as a new medal result.
