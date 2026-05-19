# Priority 0 Visual Review

Current scope: the active WinUI shell built in `src\WinBlaze.UI\src\MainWindow.cpp`.

## Result

Priority 0 visual review is complete for the stable MVP shell.

## Checks

- Active shell colors are centralized in `src\WinBlaze.UI\src\ShellTheme.h`.
- Matching stable brushes are mirrored in `src\WinBlaze.UI\src\App.xaml` as `WinBlaze*Brush` resources.
- Cards, compact cards, banners, accent panels, navigation chips, progress fills, detail panels, and treemap focus use shared theme/style helpers instead of one-off styling.
- Card and panel radii are centralized at 8px; pill-style navigation/breadcrumb chips remain intentionally rounded.
- The active tree/list and treemap no longer show fake startup catalog data.
- Tree/list empty, expanded-row, and status copy uses catalog-row terminology rather than provisional sample wording.
- Treemap empty/status copy points to real scan or cached-catalog data.
- The shell remains a dense task UI, not a landing page or marketing layout.
- Debug and Release UI smoke pass after the latest visual-system changes.

## Deferred Outside P0

- A deeper visual refresh can still happen later, but it should preserve the current stable host, automation names, and smoke-test behavior.
