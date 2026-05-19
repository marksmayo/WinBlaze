# Stack Decision

## Chosen Direction

- Scanner core: Rust
- UI: WinUI 3 with C++/WinRT
- Persistence: SQLite first
- Rendering: WinUI 3 shell chrome with Direct2D-backed SwapChainPanel surfaces for the treemap and other dense views

## Why This Stack

- Rust gives strong performance with memory safety for the hottest path.
- WinUI 3 gives us a modern Windows-native shell with the right long-term UX surface.
- C++/WinRT keeps the UI layer close to the platform while still letting Rust own the hot path.
- SQLite provides a durable starting point for persistent indexing without locking us into a premature custom format.
- GPU-backed rendering is the right fit for large treemaps and high-density visualizations.

## Guardrails

- Keep the scanner/UI boundary narrow.
- Avoid copying large file records between layers.
- Use a SwapChainPanel host with Direct2D rendering for dense visual regions.
- Profile before introducing a custom cache format.
- Preserve incremental rendering and partial results from the start.
