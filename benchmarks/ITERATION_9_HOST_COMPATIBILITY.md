# Iteration 9 host feasibility evidence

The official report remains iteration 8: all 35 original categories, 24 gold,
3 silver and 8 bronze. These diagnostics do not update any medal value.

The completed alternating matched comparison contains ten observations per
framework in `artifacts/all-gold-loop/iteration-9/paired-b65c9ef1a29a4ff1ac215faefda03294/`.
Both hosts initialized the same five controls, verified their content, loaded
the recorded framework DLL, and exited successfully. Binary and runtime hashes
are recorded. First idle and initialized-control boundaries remain separate.

| Initialized median | OS XAML | WinUI 1.8 |
| --- | ---: | ---: |
| Elapsed milliseconds | 182.76 | 262.13 |
| CPU milliseconds | 140.625 | 398.438 |
| Working set MiB | 73.97 | 95.48 |

This supports investigating a full UI candidate. It does not establish a
production improvement: the product contains substantially more controls and
native rendering, and the framework-specific window hosts differ.

## Source compatibility findings

All 22 explicitly qualified control types found in the current UI have
declarations in the installed OS XAML SDK projections. The inventory is saved
as `artifacts/all-gold-loop/iteration-9/control-compatibility-inventory.json`.
This is a declaration check, not a compile or behavioral compatibility test;
unqualified types, property differences and resources still require validation.

The full candidate needs these explicit seams:

- Replace the WinUI `WindowT` owner with a native HWND and desktop XAML island;
  preserve maximize, resize, DPI, icons, activation, close and ownership.
- Preserve strong/weak lifetime behavior across asynchronous operations and
  expose the correct owner HWND to every file picker and native dialog.
- Capture the UI dispatcher on its owning thread and use the OS dispatcher
  for queued work and timers. Background callers must retain that dispatcher.
- Adapt swap-chain panel interop to the OS interface and verify rendering,
  resizing, pointer interaction and PNG export.
- Preserve XamlRoot ownership for content dialogs, keyboard focus and message
  pretranslation, rendering subscriptions and orderly shutdown.
- Verify complete theme, accessibility and feature coverage before timing the
  product. A namespace substitution or a smaller replacement UI is insufficient.

An isolated adapter now passes a runtime contract for HWND ownership, attached
content, title, background-to-UI dispatch, XamlRoot availability, close callback,
HWND destruction and weak-reference expiration. The initial red build and final
green proof are retained in `artifacts/all-gold-loop/iteration-9/host-contract-proof.json`
and adjacent logs. This contract does not prove full keyboard or rendering parity.

All 25 UI source files have an isolated candidate copy with recorded source and
candidate hashes. Complete MainWindow.cpp compiled successfully against the OS
projections and adapter, producing MainWindow.obj. This includes the existing
feature bodies rather than replacements or stubs. The candidate entry point and
native bridge linked successfully. The full executable SHA256 is
`53821C429259A67FCCAF2FB9822E5B04611A961EA463419FFCC36B45A45682F5`.
Successful compilation alone does not establish usable startup or feature parity.

The initial export-features failure was isolated to readiness assumptions in the
automation. A native-window diagnostic recorded the selected Save As window as
hidden. Waiting for the owned visible dialog allowed JSON export to pass its
two-file content check. Shipping WinUI also reproduced later immediate menu and
input lookup failures, proving those failures were not unique to the candidate.
The collector now waits, within eight-second bounds, for the requested menu
item, the first input on each filter dialog, and invalid-filter guidance. It
rejects an invalid filter that opens a visible Save As dialog. Every original
JSON/CSV content assertion remains in place.

Both shipping and candidate executables passed the complete updated export gate
with exit zero. Logs are `production-export-complete-gate.log` and
`full-os-export-complete-gate.log`. The latter verifies two catalog files in JSON,
exactly one selected file in filtered CSV, and invalid-filter rejection.
This proves this feature slice, not every product feature or a startup gain.

The earlier Common-Controls 6 experiment was reverted. A broad UIA enumeration
did not return promptly and was stopped using only its owned processes. The
standalone native-save probe timed out in its non-XAML reference and was stopped;
it provides no accepted comparative evidence. Its cancellation method therefore
must not be treated as proof of a framework defect. The candidate executable
remains at the hash above and production binaries have not changed.

The existing accessibility audit passed: 21 interactive chrome controls named,
20 enabled client controls keyboard-focusable. It detected zero caption buttons;
this is limited evidence and cannot prove caption accessibility or keyboard
operation inside dialogs. No candidate benchmark results have been published.

Microsoft's [OS XAML island limitations](https://learn.microsoft.com/en-us/windows/uwp/xaml-islands/xaml-islands#not-supported)
explicitly state that text-input controls inside ContentDialog do not properly
respond to key presses and recommend a Popup-based alternative. WinBlaze's
ContentDialog declarations therefore require an explicit, behavior-preserving
adapter, even though their types compiled. Theme-change handling also needs
verification. These are functional migration requirements, not cosmetic details.

A real keyboard contract was prepared separately from the value-pattern export
test. It verifies both the owned foreground process and focused input before
typing. Its shipping reference could not satisfy the foreground guard: Windows
reported foreground process zero, activation false, and input focus true. No
keys were sent. This is a current execution-context limitation, not evidence
about OS-host keyboard compatibility. Real keyboard verification remains pending;
do not relax the guard or claim a keyboard pass from programmatic value updates.

No production host migration has been accepted. Candidate window icons, keyboard
focus traversal, message-loop exit paths, error handling, themes, accessibility,
graphics and the full existing feature gates remain required before adoption.
Only a validated candidate merits a fresh unchanged medal rerun.

## Full-source rendering and resource evidence

The visualization gate now enters the destination using the same owned native
dialog controls already verified by the export gate. It waits for a visible
owned dialog and its filename/save controls, propagates message failures, and
retains the original image checks. Real keyboard verification remains separate.
All three layouts passed for both executables: horizontal strips, flame graph
and sunburst. Each produced a nonempty 640x360 PNG whose sampled pixels were
grayscale. Evidence is `visualization-checks.json`, adjacent per-layout logs,
and `os-visualization-strips-controls.log`. This does not prove onscreen pointer
interaction or every graphics lifecycle path.

The full candidate also passed explicit native-host shutdown: after its visible
owner was identified by process and class, WM_CLOSE produced normal process exit
zero. Evidence: `full-ui-close-result.json`. The smaller earlier host contract
alone was insufficient for this product-level cleanup claim.

A complete alternating full-source diagnostic is retained in
`full-ui-paired-1d76e6b28720494f9a8f4d604576617a/observations.json` and `summary.json`.
Ten runs per host passed five representative initialized-control checks, actual
native-window ownership, runtime identity and normal shutdown. Both variants
used the same native backend and fresh isolated empty caches. All twenty
observations remain, including the candidate's higher first-run memory.

| Median boundary/metric | OS candidate | Current WinUI |
| --- | ---: | ---: |
| First idle milliseconds | 78.66 | 154.88 |
| First idle CPU milliseconds | 54.69 | 171.88 |
| First idle working set MiB | 17.95 | 33.20 |
| Observed initialized controls milliseconds | 662.18 | 1182.32 |
| Observed initialized controls CPU milliseconds | 859.38 | 1531.25 |
| Observed initialized controls working set MiB | 127.51 | 160.23 |

The initialized-control boundary includes UIA observation overhead and does not
prove frame presentation or keyboard usability. The isolated-cache context is
also explicit. These results support continued migration investigation; they
are not official startup medals or substituted report values. Complete feature,
keyboard, theme, window accessibility and rendering coverage still precede
adoption and a fresh unchanged all-three-product medal cohort.

## Exit and queue-close regressions

The two framework exit sites are administrator restart and updater restart.
The isolated source transformer now routes both to an explicit owned-host exit
request. No elevation or software update was launched by these tests. The new
request contract first failed compilation because the API was missing, then
passed at runtime: a background thread requested exit repeatedly, two owned
hosts closed with exactly two cleanup callbacks, and an unrelated window stayed
open. It also passed after adding the dispatcher guard.

A separate queue-close contract exposed real residual callback behavior: after
native close, the original adapter accepted another callback and executed two
callbacks. The guarded dispatcher now rejects new work and suppresses already
queued work when its host is closed. The preserved red result is
`host-queue-close-red-result.json`; the green result records zero callbacks and
rejected enqueue. The original host/lifetime contract still passes.

The rebuilt full candidate is now SHA256
`C00365CB1992CFE398364442DB772383843F885F9B96A6A57108DB21D0D9E0FC`.
Its explicit native-close check and complete JSON/filtered-CSV/invalid-filter
gate pass. The earlier resource cohort belongs to executable 53821C..., retained
as `full-os-before-exit-adapter.exe`; it is not new-binary performance evidence.

Queue suppression does not prove background producer lifetime. The source has
detached cache-loading and native-session cleanup work, including raw window
captures. Before adoption, complete the owned-worker/drain contracts described
in [the shutdown lifetime plan](ITERATION_9_SHUTDOWN_LIFETIME_PLAN.md). The primary
UI and native backend binaries remain unchanged.

### Owned producers and active scan shutdown

Candidate SHA256 4AA3400700DE8CEB5CB4718E05D397CC659B5DD5B946E01B44A0A63D02FE62AE
compiled and linked after owning all four original detached jobs. Session start
publication and manual cancellation share the native event state mutex; busy
state is atomic. Close invalidates cache generation, captures active native
session under the mutex and schedules cancellation/destruction. Entrypoint drains
owned jobs while the complete derived window remains alive, then closes/pumps
framework teardown before release.

`host-worker-result.json` proves delayed raw-member job completion before release,
zero queued UI callbacks after close and rejected external work after drain.
`full-ui-active-scan-close-result.json` proves owned visible native C: scan start
(scanner.session_started log and enabled Cancel control), WM_CLOSE and normal
exit zero. `full-ui-cancel-scan-close-result.json` separately proves manual Cancel
invocation after the same active-scan evidence, restored Start availability and
normal WM_CLOSE exit zero. Full build/link logs and both runtime logs are in
iteration-9 artifacts. No forced termination is accepted as success.

Remaining adoption requirements include delayed real cache loading, exception
cleanup lifetime, keyboard/theme/caption/onscreen interaction and remaining
feature parity. These shutdown cases are not a fresh medal cohort and do not
validate all possible shutdown or callback interleavings. Shipping UI hash
675F52F72AE9364655139DC36DC472A5D14DBB4AB8957BB1888546F619259A8F unchanged.

### Caption UIA reference comparison

Direct UIA FromHandle inspection of verified native candidate host D56... finds
zero TitleBar nodes/zero caption Button descendants. To distinguish an island
issue from a shared observation condition, plain-caption-reference.cpp creates
an ordinary Win32 WS_OVERLAPPEDWINDOW with DefWindowProc only; no XAML framework.
The same observer also finds zero TitleBar nodes/zero caption buttons on that
reference (SHA1F38E5...). Native minimize/restore/maximize/close commands PASS on
both. Artifacts native-caption-uia-inventory.json/plain-caption-uia-inventory.json
and source/build/probe scripts preserve evidence. Absence is not shown to be
candidate-specific; caption accessibility remains unverified, not waived.

Microsoft documents passing WM_GETOBJECT to DefWindowProc when not implementing
an accessibility provider; both reference and experimental procedure follow
that default. This documentation alone does not prove runtime caption exposure.
https://learn.microsoft.com/en-us/windows/win32/winauto/handling-the-wm-getobject-message

### Directory viewport regression fixed in isolated candidate

Full candidate D56... failed original expansion assertion after bounded8s while
shipping675... passed. UIA reported four entries and CollapseDocuments but only
19px tree viewport, Documents offscreen/empty and nested row unrealized. Original
inventory retained as expanded-row-before-tree-floor.json. Candidate transformer
now raises directory card row minimum180->320 to accommodate OS control chrome.
Full rebuild candidate SHA DF38EDACACDE757CA2DFE3825EB139CFD5FED45E7F02BAE971A5426695CAC883
passes unchanged expand/collapse and restored-window scan-control clipping gates
(tree-viewport-results.json), plus strips PNG dimensions/size-mode gate. Corrected
independent bounds collector shows194px viewport and nested.txt,File,16B visible
at50px row height. Earlier collector accidentally emitted script blocks rather
than executing capture; its stale19px output is not new-candidate evidence.
No production adoption or medal update; other feature/input/async parity remains.
# Current candidate performance refresh (2026-10-07)

Completed matched alternating launches, n=10 per host, with isolated empty
appdata and unchanged native DLL. Candidate DF38EDAC... (including lifetime and
tree viewport fixes) versus shipping 675F52F7.... All 20 observations retained,
five enabled controls and exact native owner/runtime verified, normal exit 0.
Median first idle: candidate 54.7954 ms, CPU 54.6875 ms, working/peak 18.3379 MiB;
shipping 96.39335 ms, CPU 117.1875 ms, working/peak 35.9082 MiB. UIA-observed
initialized controls: candidate 460.18675 ms / CPU 679.6875 ms / 127.4531 MiB;
shipping 747.6535 ms / CPU 1156.25 ms / 159.8906 MiB.
Evidence: artifacts/all-gold-loop/iteration-9/full-ui-paired-
2a15e3cb51ea46bfa3cee515f8de73bd/{observations,summary}.json.
Diagnostic only, not frame/keyboard evidence, official medals, or adoption proof.
