# Restore native and XAML focus handoff

A repository-host integration test reproduced the missing focus bridge. With a
visible, enabled owned window on the same UI thread, native focus stayed on the
owner and both XAML buttons remained unfocused. The baseline host was the exact
iteration24 build copy. This is a behavioral RED, not a source-text assertion.
An earlier hidden-launch probe lacked the intended setup and is not the basis
for this defect claim. A missing collections header was a harness compile error.

The adapter now transfers native focus into its island, asks XAML to restore the
previous control, and enters the first control if there is no previous focus.
Restore alone did not transfer native focus; that intermediate failure remains
in the evidence. Entry and restoration both pass after the native handoff.

Forward Tab at the last control also went RED against the entry/restore-only
host. The single-island window has no native tab stops, so its TakeFocusRequested
handler cycles First/Last requests within the island. A recursion guard bounds
unsatisfied requests, and the event revoker runs before closing the source.
Forward and reverse message routing pass. The final gate also checks an empty
island and release of the closed window.

The test calls PreTranslateMessage directly with a message for its own focused
island. It does not inject desktop input. Reverse routing saves/restores the
calling thread's keyboard table; it does not attach input queues or use SendInput.
Microsoft documents the required [island focus bridge](https://learn.microsoft.com/en-us/windows/uwp/xaml-islands/advanced-scenarios-xaml-islands-cpp)
and the [thread-local scope of SetKeyboardState](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setkeyboardstate).
The unchanged external physical-keyboard guards still apply; this contract does
not establish full-app physical keyboard parity.

Evidence under artifacts/all-gold-loop/iteration-25 includes focus-visible-before,
focus-first-after, focus-restore-probe, focus-restore-green, focus-tab-before,
focus-tab-after and focus-bidirectional logs/results. The FocusManager-only wrap
probe did not exercise the host keyboard-message route and is not claimed as
proof of that route. The corrected Tab probe supplies the actual RED/GREEN pair.

The final full build and its focus gate pass, including empty-island handoff and
weak-window release. UI SHA256 is
B9C5A4BAED139C1AA54AA026E29A7229E884EFC892BA2CC96041093568B8EA78;
native DLL remains9FF4B2B. Icons, restored scan controls, visible expansion and
the extracted portable ZIP's UI/CLI/four binary identities pass. Three empty
and three changed-catalog no-device discoveries preserve catalogs and exit
normally. The accessibility slice observes21 named controls and20/20 enabled
client controls focusable; zero caption buttons are exposed, so caption
accessibility remains unverified. Twenty paired launches against24 pass
initialized controls, owner, OS runtime, PMv2 and normal release:
iteration-9/full-ui-paired-a41e9535df9445ba90edb6ada66fbfc3.

One fresh canonical ten-run startup cohort measures WS/peak14.849609375MiB,
below retained16.244140625MiB. Raw CPU median15.625ms. CPUrun8 is an observed
zero; the original resource verifier remains RED at its positive-observation
requirement, leaving nine qualifying CPU observations. All samples, including
the first22.890625MiB working set, remain. No unchanged-binary retry or previous
candidate best is substituted. Raw startup and verifier/gate files are in25.

Neither prototype is adopted; the official35 remain27 gold,7 silver,1 bronze
with competitor records retained. These focused adapter checks do not establish
all remaining full-app adoption parity or a completed all-gold goal.
