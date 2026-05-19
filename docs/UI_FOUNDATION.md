# UI Foundation

## Shell

- Keep the main window simple and task-oriented.
- Present scan controls, current status, and result summaries immediately.
- Reserve the lower-density views for tree, treemap, and detail panels.
- Keep the app on the `AppT`/`IXamlMetadataProvider` hosting path, even while the
  active shell is built in code; UI Automation depends on that metadata-provider
  shape for reliable smoke-test discovery.
- Use the code-built navigation chips as the MVP shell navigation pattern. A
  direct `NavigationView` wrapper previously crashed after activation, so keep
  the chip host unless that control path is isolated separately.
- Keep navigation chips accessible as first-class navigation: stable automation
  names, help text, tab order, access keys, and Ctrl/Alt numeric shortcuts are
  part of the host contract.

## Interaction

- Keep scan controls available while a scan is running.
- Show partial data as soon as it arrives.
- Surface search and filtering controls in the main shell rather than hiding them in
  menus.

## Visualization

- Build toward a treemap and a proportional directory tree.
- Keep details visible without forcing navigation away from the main view.
- Make hover and focus states deliberate rather than decorative.
- Use the virtualized catalog list as the MVP tree surface: it must keep
  paging, path-depth indentation, visible level/parent metadata, top-level group
  context, and stable UI Automation behavior. A fully expandable `TreeView` can
  be evaluated later only if it preserves those constraints.

## Visual System

- Keep active-shell color, radius, and spacing decisions centralized through the
  `ShellTheme` tokens in `src\ShellTheme.h`.
- Mirror stable shell colors into `src\App.xaml` as `WinBlaze*Brush`
  resources so future XAML-hosted shell controls can use the same palette
  without reintroducing one-off colors.
- Route cards, navigation chips, progress indicators, and visualization accents
  through style helpers before introducing new one-off styling.
- Keep `ShellTheme.h` and the WinUI resource dictionary in sync while the active
  shell remains code-built.

## Rendering

- Keep expensive redraw work outside the scanner hot path.
- Batch UI updates where possible.
- Treat virtualization as mandatory for large trees and lists.
