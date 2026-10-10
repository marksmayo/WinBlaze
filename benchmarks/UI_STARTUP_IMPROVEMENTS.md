# UI startup improvement audit

50 distinct opportunities reviewed, ordered by evidence and implementation cost. One production change qualified and was implemented with a failing I/O regression. Existing work is recorded without claiming new savings. Remaining entries require profiling and behavioral regressions before they qualify as zero-to-low risk. No millisecond savings are invented.

| Rank | Opportunity | Status | Estimated saving / qualification |
|---:|---|---|---|
| 1 | Opt-in routine startup tracing | implemented | Eliminates all routine trace file opens, size queries and writes; 50-call regression: 50 writes to zero. |
| 2 | Cache diagnostic trace destination | candidate | Only affects opted-in diagnostic runs; saves repeated path resolution, not normal startup. |
| 3 | Use UTF-8 literals for diagnostic tracing | candidate | Avoid literal conversion when diagnostics enabled; prove encoded log behavior first. |
| 4 | Lazily format dynamic diagnostic messages | candidate | Avoid constructing disabled dynamic messages; inspect call-site allocation cost. |
| 5 | Batch diagnostic log appends | candidate | Reduce diagnostic-only I/O; preserve ordering and crash evidence. |
| 6 | Avoid duplicate root-path reads | candidate | Reduce initial XAML Text property reads; validate root state first. |
| 7 | Reuse startup text styles | candidate | Reduce repeated per-control style construction; measure object creation and visual parity. |
| 8 | Reduce initial layout invalidations | candidate | Combine properties before attaching controls; validate accessibility and layout. |
| 9 | Remove duplicate initial ApplyShellState | candidate | One pre-ready shell visibility/control refresh; must preserve initial state. |
| 10 | Remove duplicate initial summary formatting | candidate | Avoid summary rebuilt by ApplyShellState; profile before changing. |
| 11 | Remove duplicate initial runtime formatting | candidate | Avoid repeated runtime string construction; retain diagnostics correctness. |
| 12 | Remove duplicate initial SetSection | candidate | Avoid redundant visibility assignments; verify navigation and keyboard focus. |
| 13 | Remove duplicate initial search preview | candidate | One redundant FormatSearchQuery/label update. |
| 14 | Remove duplicate initial event label update | candidate | One overwritten event text assignment; preserve ready messaging. |
| 15 | Cache immutable theme brushes per window | candidate | Reduce repeated SolidColorBrush construction; first verify no mutable sharing. |
| 16 | Skip setting unchanged text labels | candidate | Avoid XAML invalidations when text is identical; requires measured hot labels. |
| 17 | Skip setting unchanged visibility | candidate | Avoid redundant XAML dependency-property assignments; small unknown gain. |
| 18 | Skip setting unchanged enabled state | candidate | Avoid scan-control invalidations; preserve validation transitions. |
| 19 | Move native DLL initialization off loaded handler | candidate | Remove LoadLibrary/symbol lookup from UI thread; needs thread-safe DLL initialization first. |
| 20 | Reserve native catalog vector only when nonempty | candidate | Avoid allocating 10,000 entries on empty first run; check allocation profile. |
| 21 | Detect absent catalog before snapshot allocation | candidate | Avoid empty-cache allocations, without racing a new scan or weakening error reporting. |
| 22 | Defer diagnostics pane construction | candidate | Avoid hidden diagnostics controls before first use; requires broad UI behavior tests. |
| 23 | Defer cleanup pane construction | candidate | Avoid hidden cleanup controls; preserve paging and import readiness. |
| 24 | Defer search pane construction | candidate | Avoid hidden search controls; preserve keyboard shortcuts and search state. |
| 25 | Defer filter pane construction | candidate | Avoid hidden filter controls; preserve query defaults. |
| 26 | Defer extension pane construction | candidate | Avoid hidden type controls; preserve complete totals. |
| 27 | Defer toolbar overflow items | candidate | Avoid creating unopened menus; requires accessibility and shortcut tests. |
| 28 | Defer export dialogs | already present | Dialogs are created on invocation; no new startup saving. |
| 29 | Defer GPU treemap creation | already present | Renderer is demand driven; no demonstrated eager GPU startup saving. |
| 30 | Keep full cache decoding off UI thread | already present | Snapshot parsing already runs on a background thread. |
| 31 | Keep scan work off UI thread | already present | Native scan pipeline already asynchronous. |
| 32 | Avoid automatic initial disk scan | already present | Startup waits for selected root; no new saving. |
| 33 | Keep update network request asynchronous | already present | Update check already background work; do not remove update functionality. |
| 34 | Keep native bridge demand-loaded | already present | Constructor defers bridge loading until scan/cache load. |
| 35 | Avoid periodic flush timer before scan | already present | Flush timer created on demand. |
| 36 | Avoid image asset downloads | already present | App icon assets are local resources. |
| 37 | Reuse initial query formatting result | candidate | Avoid multiple formatting calls for identical empty query. |
| 38 | Avoid initial empty tree sorting | candidate | Check current empty-tree path before adding shortcut. |
| 39 | Avoid initial empty search filtering | candidate | Check default root and empty-catalog semantics before shortcut. |
| 40 | Avoid initial empty extensions rendering | candidate | Measure placeholder rebuilds; preserve first-run empty state. |
| 41 | Avoid initial empty largest-files rendering | candidate | Measure native/root requests on startup before shortcut. |
| 42 | Coalesce breadcrumb refreshes | candidate | Avoid duplicate navigation rebuilding; verify path validation and imported roots. |
| 43 | Coalesce selection text refreshes | candidate | Avoid formatting repeatedly before first populated selection. |
| 44 | Reserve startup text formatting buffers | candidate | Reduce reallocations in measured large labels; size evidence needed. |
| 45 | Remove redundant window title assignment | candidate | Avoid duplicate title APIs; check title/accessibility behavior. |
| 46 | Remove redundant maximize calls | candidate | Avoid extra layout; existing WinUI/HWND fallback may be needed on this machine. |
| 47 | Load shared icon handles | candidate | Avoid duplicate icon decoding; icon ownership and DPI semantics need checking. |
| 48 | Use incremental/link optimization evidence | candidate | Measure current compiler/linker settings before changing; no assured runtime gain. |
| 49 | Delay nonessential counters until first frame | candidate | Remove initial formatting from critical path; preserve initial diagnostics snapshot. |
| 50 | Defer cached catalog rendering in bounded batches | candidate | Reduce first-frame UI blocking for restored catalogs; test progressive correctness separately. |
