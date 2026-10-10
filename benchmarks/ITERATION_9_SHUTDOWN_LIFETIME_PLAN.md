# Candidate shutdown lifetime requirements

The OS-host candidate remains experimental. Its measured resource improvement
does not waive shutdown or feature parity.

Current evidence:

- Host-exit requests from a background thread close both owned test hosts,
  invoke cleanup once each, tolerate repeated requests, and preserve an unrelated
  window. Both candidate restart paths are explicitly routed to this request.
- The original queue-close regression returned exit 20: after native close,
  the dispatcher accepted new work and executed two queued callbacks.
- The guarded dispatcher returned exit zero: new work was rejected and queued
  callbacks were suppressed. The original host behavior contract still passes.
- `LoadPersistedCatalogSnapshot` captures `this` in a detached thread. Its source
  comment assumes the window outlives the dispatcher. The candidate entry point
  currently releases its window before closing the manager and draining messages.
- Scan event handlers also capture `this`; session destruction and cancellation
  use detached cleanup threads. Queue suppression alone does not prove these
  producers have stopped accessing the window.

Required next implementation and red/green evidence:

1. Make cache-loading and session-cleanup work explicitly owned and drainable.
   Register work before it can race with shutdown; account for work created by
   another owned worker while draining.
2. On close, stop accepting UI work, invalidate stale cache results and cancel
   the active native session. Serialize session capture with its event handler;
   do not hold the pending-state mutex while joining native callbacks.
3. Keep the complete window alive until its owned producers and cleanup have
   finished. Drain guarded UI work and framework teardown in a defined order
   before releasing the window. Do not label forced process termination as clean
   shutdown or replace this with a deliberately leaked window.
4. Test delayed cache completion and active-scan shutdown, not only an idle empty
   window. Assert that no member access outlives the window, no UI callback runs
   after close, and shutdown finishes without deadlock or abnormal exit.
5. Rebuild every translation unit affected by the changed base layout. The
   resource cohort measured executable 53821C...; its values do not automatically
   describe rebuilt candidates. Complete the remaining feature, keyboard, theme,
   caption and rendering checks before adoption and a fresh unchanged medal run.

Primary WinUI and native backend binaries remain unchanged. The official report
continues to retain all 35 results with version-specific competitor bests.

Integration checkpoint: experimental host now exposes RunBackground,
DrainBackground and IsClosing; entrypoint drains with the derived object alive,
then closes/pumps the manager before releasing the window. Copy transformer owns
all four original detached jobs, invalidates cache generation on close, captures
active session under pending mutex and registers cancel/destroy, and ignores
post-close events in the locked state-update block. Host worker RED missing APIs,
GREEN exit zero (host-worker-result.json), delayed raw-member access completes
before release and UI callbacks remain zero after close.

Still required before adoption: serialize BeginScan session publication and both
manual cancel routes with native completion (shipping source currently modifies
m_session outside the event mutex); register cancellation before GUI updates can
reenter; test active native scan and delayed catalog load; handle constructor or
entrypoint exception cleanup without releasing derived state before drain. Do not
claim current transformer alone proves these remaining races safe.

Exception cleanup checkpoint: DrainBeforeRelease guard declared after the owned
window and framework manager now attempts Close, DrainBackground and framework
teardown in order on both Finish and stack unwinding. Explicit Finish reports the
first cleanup error after all steps; destructor preserves the original exception.
Contract GREEN covers delayed worker access before owner destruction, throwing
Close without skipped drain/teardown, and original exception identity. Runtime
negative control replacing guard with no-op fails exit 10; guarded contract exits
zero. Full candidate entrypoint integrates the same helper; rebuild/regression
not yet finished at this checkpoint. Real delayed persisted-catalog loading and
full UI exception injection remain unverified; no broad lifetime completion claim.

Additional producer inventory: the shipping MainWindow has fire-and-forget
operations (ImportCsv at 2409/2449-2450, exports and other dialogs) using
get_strong, resume_background and apartment_context directly, outside the four
std::thread sites. Strong captures protect object allocation but are not proof
of framework teardown ordering or suppressed UI continuation after Close.
Before claiming comprehensive shutdown, inspect/test close during those actual
background operations as well as cache/scan. Do not infer full producer coverage
from the owned thread group's four-site assertion.

Deterministic real native cache overlap contract completed:
`host-cache-close-contract.cpp` links the candidate's NativeBridge.obj and loads
unchanged ECFE... native DLL against real persisted20000-file snapshot. It blocks
inside the first real callback, closes host while callback is pending, then
releases callback100ms later. Host owns work, complete derived object stays alive
until drain. GREEN exit0: files20000/read1720455 bytes, zero member accesses after
release and zero UI callbacks after close. Negative control omits explicit drain
before derived release; detects8192 attempted accesses after release, exits20,
without dereferencing released memory. Both result JSONs retained. Full candidate
D56... binary unchanged. This verifies host/native callback lifetime protocol;
it does not execute full MainWindow with a callback barrier or cover its other
fire-and-forget operations/keyboard/theme/caption/render parity.

Full candidate close-during-import check: verify-active-import-close.ps1 reuses
owned native Open-dialog helper with a200000-entry offline catalog. It requires
exact Opening saved snapshot status and disabled scan-root input before sending
WM_CLOSE to PID/class-verified native host. CandidateDF38... exits normally0;
active-import-close-result.json preserves observedbusy state and exacthash. This
is real fullMainWindow coroutine-path close behavior, not a synthetic host. It
proves observedcase exits normally, not that background import finished or all
coroutine producers were drained; keep scope explicit in adoption evidence.

Owned coroutine tracer bullet (2026-10-07): OwnedBackgroundAwait.h composes the
existing owned thread group with a C++20 awaiter. owned-coroutine-contract.cpp
suspends real coroutine work behind a barrier, closes admission, verifies drain
has not returned, releases work, and requires continuation completion before
drain returns. Initial missing helper build RED; implementation GREEN exit0.
Next behavior: rejected work must not silently succeed. Runtime RED exit22;
explicit rejection exception GREEN exit0, retained green build/log. Resume
occurs on the owned worker; awaiter must never be accessed after resume because
the coroutine can destroy it. Full MainWindow integration and real import
overlap regression remain pending; this helper is not adoption evidence alone.

Import integration checkpoint: full compile session72387 terminal0; link86451
terminal0 rebuilds entrypoint and NativeBridge against the updated host header.
Candidate SHA0515883EE932892B7C21A7DCCDEDBDBA26646117B9A6BF9FBD8C09114CA36614.
Transformer uses AwaitBackground for the actual native import, with IsClosing
checks before/after UI marshal. Active200000-file import close session25761
terminal0; normal offline9000-file import/search session8268 terminal0. Owned
coroutine/work/unwind executable contracts all PASS. Production unchanged.
Remaining important interleaving: background completion can enqueue a UI
continuation just before close. The owned job can finish while that continuation
is suspended; group drain alone does not prove continuation runs before manager
teardown. Test/own that complete coroutine lifetime before claiming adoption.
Other background coroutines remain outside this import-specific transformation.

Complete import-continuation lease checkpoint: OwnedCoroutineLifetime.h tracks
an operation across worker completion and queued UI resume. Drain pumps pending
messages while the manager/window remain owned, until leases release; background
worker failures are rethrown after this drain. The runtime negative control
models framework teardown before the callback and fails exit20 without touching
freed UI; positive control exits0. coroutine-lifetime-result.json retains both.
Full build46505/link90721 terminal0; current candidate SHA
DA0F439598E9B752CC22C0E09406A61B3BDE110AE174F12D41C7118200520F52.
Real active200000-file close99574 and normal9000-file import/search24566 terminal0.
Actual close test scope remains observed case; deterministic ordering is covered
by the helper contract separately. ExportCatalog still uses direct background
resume and must be covered next; this is not full coroutine/adoption evidence.

ExportCatalog ownership checkpoint: exact source transform registers the native
export with AwaitBackground and retains an operation lease through UI resume,
with closing checks before/after marshal. Compile71473/link29570 terminal0,
candidate SHA70DDF88951DB613B5CE1E365A72E5BE0004ED3ED78B3A4A564C419DE310938F5.
New real UI export-close test imports50000 offline files, observes detailed JSON
export status and locked inputs, closes exact owned host, requires normal exit
and all50000 JSON file entries afterward. Before-ownership run98730 ALSO passed;
this is not a runtime RED or proof that prior code waited for the export.
After-ownership run55708 PASS; evidence ui-parity-20261007T110328690Z.
Normal JSON/filter/invalid-filter feature gate4347 PASS. Existing red/green
owned-work/queued-continuation contracts establish the reused protocol; this
observed export case does not cover every race or remaining coroutine path.

Multiple-root ownership checkpoint: source transformer admits an operation lease
and runs ScanRoots inside AwaitBackground, with closing checks surrounding UI
resume. Compile61643/link3527 terminal0; candidate SHA
67826CF3B856AF6D3B92482CD84406D6B4942891504018FFAF05E027BC50E4DA.
Original UI test initially missed the Multiple scan roots dialog after300ms.
Same control lookup now waits at most8s with unchanged named/path assertions;
paired shipping675 and pre-change candidate70DD both PASS (session26184).
Updated candidate normal multiple-root parity67773 PASS: both original file
paths publish in one combined catalog. Owned-worker/continuation contracts PASS.
No claim for real close during ScanRoots or cancellation latency; native ScanRoots
is synchronous and has no cancellation handle in this transformed operation.
Remaining coroutine/adoption coverage is still required.

Read-only analysis checkpoint: StorageCost and DuplicateResults now execute in
AwaitBackground with operation leases and closing checks around UI resume.
The lease scope ends before displaying the results dialog; it covers native
work/return, not user dialog lifetime. Compile39034/link98408 terminal0, current
candidate A04D0F95BB7E23A6E8D7FB9C1400F7A7F908A85AE2313CCD4F16C205E5535AE9.
Serial parity-storage-cost and parity-duplicates batch86796 terminal0. Same-hash
result JSON records cold-tier classification/monthly cost and selected hash,
verified duplicate group/filter PASS. Reused owned-work/continuation contracts
PASS. No new per-analysis runtime RED or close-during-analysis claim; ownership
protocol has the earlier deterministic red/green controls. Remaining other
producers, dialog shutdown, keyboard/theme/accessibility/adoption still pending.

Permission inspection checkpoint: native PermissionRows callback work executes
through AwaitBackground with a complete-return lease, close checks around UI
marshal, and lease released before the results dialog. Compile41304/link68131
terminal0; candidate F2F85D07BD3F6FE97BD9E3CA1A27CC550990049D891CC3FF956E0184C7E15C92.
First pre-change attempt launched relocated backup without normal dependency
layout and could not open; not feature evidence. Corrected private layout includes
unchanged native DLL and opens. Original300ms lookup then missed Permission path;
same lookup now waits at most8s. Serial shipping675/pre-changeA04/currentF2F
batch59981 allPASS owner/allow/filter/SID exclusion original assertions. Protocol
contracts PASS. No real close-during-PermissionRows or results-dialog shutdown
claim. Other producers and adoption requirements remain pending.

Confirmed file-operation ownership checkpoint: ConsolidateDuplicates and
CommitInlineRename use owned native jobs plus complete-return leases and closing
checks around UI resume. Build79467/link84964 terminal0; candidate
5E79FFF7BF259C66AC7FA85A20F600B3FDE7DC4C2404DEA498D26E61361D32A5.
Hard-link fixture check PASS preserves paths/bytes. Initial file/folder inline
checks failed fixed-delay Inline item name lookup; raw results retained. Same
lookup now waits8s maximum. Pairedshipping675/current5E79 allfour checks74719
terminal0: file collision/cancel/content and folder nested content assertions
PASS. Only repository-local fixtures modified. Protocol contracts PASS; not
close-during-mutation or full dialog lifecycle evidence.

Dialog rename ownership checkpoint: RenameItem runs the underlying Rename call
inside AwaitBackground with complete-return lease/closing checks. Compile75133,
link82319 terminal0; candidate646C6A3CB28AC35F7CF8F1C8BC42A6873F7AFF2C707BFCA6CE0FB4160806D102.
Initial56985 file/folder checks missed New item name after fixed300ms, raw failures
retained. Same named lookup now waits at most8s. Shipping675/current646 paired
batch51034 allfourPASS file content and nested folder contents original assertions.
Operations used repository-local fixtures. Protocol contracts PASS; no blanket
close/error/dialog lifecycle claim. Remaining producers/adoption still pending.

Remaining local file actions checkpoint: maintenance, batch move/recycle and
single move/recycle native work now owned with complete-return leases and close
checks. Initial transformer refused a missing blank-line anchor; corrected exact
anchor, regeneration PASS. Compile45630/link37795 terminal0; candidate
F3D38CA35B76686F8E5279D8A4CCC4BF2AFAD2674251DC87D403A502420813A1.
Initial99311 checks failed early control lookups/global-focus Save. Bounded8s
samecontrol lookups added for maintenance/batch; broad parity uses the previously
verified PID/visible native-dialog filename/button helper (functional checks,
not keyboard proof). Paired74734: compression and batch recycling shipping/current
PASS; move picker one-shot failed both. Same owned-picker lookup now waits10s;
paired37290 shipping/current broad parity PASS duplicate/ACL reports, automatic
watcher publication, move cancel/success, confirmed recycling. Fixtures local.
Protocol contracts PASS. Direct resume_background sites now only three in
portable-device flow. Dialog/static updater/input/theme/adoption coverage remains;
these functional checks do not establish all close/error interleavings.

Portable producer tracking checkpoint: retain original coroutine threadpool/COM
path, register complete operation leases instead of moving COM calls onto new
std::threads. Discovery lease covers background/UIreturn, ends before selection.
Metadata scan/import lease covers existing phases and temporary snapshot cleanup;
closing checks suppress publication/UIupdates. Compile67484/link81623 terminal0;
candidate F8E0818E6DE49CC707E1DA703E26CD72BD8D8CC8415577842C07052828AAAB10.
Original and bounded selection checks fail on bothshipping/candidate. Corrected
status observer15610 verifies exact No portable device found on both. Initial
observer output was swallowed by nullassignment; rerun uses WriteHost. Hardware
selection/live scan/cancellation remain unverified, not waived. Deterministic
lifetime protocol PASS, but not a real-device shutdown-interleaving proof.
