# WinBlaze.UI

WinUI 3 / C++/WinRT native shell for scan visualization and interaction.

Active contents:

- application bootstrap and stable code-built shell window host
- scan root, start, cancel, and keyboard shortcuts
- catalog-backed folder tree/list and detail panes
- Direct2D/SwapChainPanel treemap with DirectWrite labels and tile hit testing
- indexed search and filtering controls
- runtime, frame-time, and correctness diagnostics
- native bridge to the Rust scanner/index DLL

The legacy full-shell variants and inactive `MainWindow.xaml` host artifacts have
been removed from active source. The active host uses code-built section chips
because a direct `NavigationView` wrapper previously crashed after activation.
Use `tests\ui\smoke.ps1` from the repository root to verify launch,
scan, search, diagnostics, treemap rendering, cancel, and crash-log checks
against the Debug or Release executable.
