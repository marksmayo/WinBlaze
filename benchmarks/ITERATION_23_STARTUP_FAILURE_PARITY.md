# Restore startup failure reporting

The isolated entry point lacked shipping's user failure log and error dialog.
A separate negative executable preinitializes COM as MTA and calls the actual
candidate entry point, which requests STA. This forces a real apartment conflict
before creating the UI. There is no fault-injection flag in the normal app.

The original negative executable fails the behavioral contract: exit99,
no user error dialog and no user failure log. After restoring startup reporting,
the same contract passes with routine tracing disabled: one UI/wWinMain failure
record with an explanation, an owned startup-error dialog and original HRESULT
80010106. The test dismisses only the dialog belonging to its launched process.
Evidence: artifacts/all-gold-loop/iteration-23/startup-failure-red.log,
startup-failure-green.log, failure-before-* and
failure-after-280e848089694f339cd7c48fb01754d0/result.json.

The normal entry point uses the existing StartupTrace helper, installs the
existing unhandled-exception filter, preserves the weak-window release assertion
and reports caught HRESULT/standard/unknown failures. It no longer relies on an
artifact-directory text file. Both normal and negative builds passed. A line
ending mismatch initially prevented the filter insertion; inspection caught it,
and the final source was corrected and rebuilt before validation.

Normal candidate UI SHA256:
6C5732A8FB842F5622BBBEF7FF24C9D6E8C2CBDB3784228AF09CBE544828D3F9.
Native DLL remains9FF4B2B; MainWindow and NativeBridge objects, bitmap icons,
SegmentHeap/PMv2 manifest and four first-use imports are preserved from21.
Icon pixels pass (ui-icons-20261008T074029895Z). Twenty paired launches pass
the five initialized controls, owned-window, exact OS runtime, PMv2 and normal
release checks: iteration-9/full-ui-paired-12f5a6c8e9d84989a09a9380915f623e.

One fresh canonical ten-run cohort measures working set/peak **15.6171875MiB**,
still below retained WinDirStat16.244140625MiB. First-idle median32.95ms;
raw CPU median23.4375ms. All observations, including first23.63671875MiB and
later16.84-16.89MiB samples, remain. CPU run7 is an observed zero. The original
resource verifier rejects it at its positive-observation requirement, and the
unchanged medal builder excludes nonpositive startup resource samples, leaving
nine qualifying CPU observations. Thus the full resource gate is **RED**, even
though the independently computed memory metric clears the target. No zero is
replaced, no test definition is changed, and no unchanged-binary retry or prior
candidate best is substituted. Raw and gate files are retained in iteration23.

The nine fresh functional feature checks completed against21; they are not
misrepresented as a new nine-check cohort against23. The changed entry point's
normal and failure behavior has separate evidence above. Product integration,
reproducible packaging and remaining input/window validation continue before
adoption. This candidate is not adopted. All35 official rows remain27gold,
7silver,1bronze, and all70 competitor best records stay unchanged.
