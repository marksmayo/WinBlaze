# Startup initialization experiment and maintenance ownership

The official completed report remains 27 gold, 7 silver, 1 bronze across all 35
categories. No shipping binary or official value changed in this iteration.
Complete unchanged competitor baselines remain frozen under the user's latest
instruction; experiments below involve only WinBlaze builds.

## Maintenance ownership RED-GREEN

The current-source candidate A14 reproduced a retained window after closing the
maintenance options dialog: `artifacts/ui-parity-20261007T132208559Z/dialog-close-result.json`,
exit 97 under the weak-reference release assertion.

The isolated transformer now retains a complete method lease from maintenance
entry through both options/confirmation dialogs and the existing background/UI
continuations. Both dialogs use the adapter's owned-dialog cancellation path.
Regeneration, compile session 62078, and link session 25275 completed successfully.
Primary candidate SHA:
`5B683F2F42B3D1853E33BA5F3F2DEBB3A8B5114BC8B9575000955BCF37E2D3D7`.

Options close passes at `artifacts/ui-parity-20261007T132735595Z`; confirmation
close passes at `artifacts/ui-parity-20261007T132813213Z`. The confirmation case
does not apply maintenance. Normal explicit GUI compression passes on shipping
and candidate, preserving the fixture's complete content hash and setting the
NTFS compressed attribute. The first candidate normal-feature attempt failed
before locating its menu item; the unchanged retry passed and the earlier failure
is retained, not relabeled as a lifecycle or maintenance failure.

Permissions, multiple roots and maintenance now have complete method/dialog
ownership. Other coroutines, static updater operations, keyboard, caption
accessibility, themes, portable-device and adoption checks remain incomplete.

## Implicit XAML initialization hypothesis: rejected

Microsoft documents that constructing DesktopWindowXamlSource before any hosted
UIElement initializes the XAML framework without an explicit
WindowsXamlManager.InitializeForCurrentThread call:
[WindowsXamlManager initialization documentation](https://learn.microsoft.com/en-us/uwp/api/windows.ui.xaml.hosting.windowsxamlmanager.initializeforcurrentthread).
Shutdown is asynchronous and requires completed async work and a drained message
pump: [WindowsXamlManager.Close documentation](https://learn.microsoft.com/en-us/uwp/api/windows.ui.xaml.hosting.windowsxamlmanager.close).
These documents support a feasible experiment, not a prediction of lower memory.

The separate `artifacts/all-gold-loop/iteration-11/implicit-candidate` copies all
current candidate source and the identical native DLL. It creates the source
before looking up the dispatcher and before constructing controls. Its close
path hides/cancels the window but holds the XAML source until admitted work is
drained, then closes the framework and pumps pending messages. Shipping and the
primary candidate remain unchanged. Constructor-failure and all feature parity
paths were not established for this experimental lifecycle.

Build session 63027 completed. A one-pair preflight verified five enabled
controls, exact owner/runtime, identical native DLLs and normal release for both
hosts. Then a complete alternating n=10 comparison retained all observations,
including the first launch:
`artifacts/all-gold-loop/iteration-9/full-ui-paired-be6d79fdda934c52aee3ebfc276db4ee/{observations,summary}.json`.

| Metric | Implicit initialization | Existing primary candidate |
|---|---:|---:|
| First-idle working set median | 16.6015625 MiB | 16.35546875 MiB |
| First-idle median | 40.4224 ms | 39.28975 ms |
| CPU median at first idle | 31.25 ms | 31.25 ms |
| Initialized working set median | 112.275390625 MiB | 112.255859375 MiB |
| Normal exit with object-release assertion | 10/10 | 10/10 |

Implicit executable SHA:
`5CB41EC6AE7EDEE51B406FD0403E7A9682C73F8E0BDD6CFCA8A70E0831788FDF`.
Both cohorts still exceed the retained WinDirStat 16.244140625 MiB target. The
implicit path is rejected as a performance improvement and has not been adopted.
No favorable run was substituted for either median. This is diagnostic evidence,
not a new official medal score or complete candidate adoption proof.
