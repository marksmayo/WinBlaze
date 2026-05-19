# Implementation Plan

## Phase 1: Foundation

1. Choose the application stack.
2. Define core models and interfaces.
3. Implement basic volume scanning.
4. Add error handling and progress reporting.

Stack decision:

- Rust scanner core
- WinUI 3 with C++/WinRT UI
- Binary snapshot persistence, with SQLite retained only as a future backend
  option
- GPU-backed visualization

## Phase 2: Persistence

1. Add a disk-backed index format.
2. Load and save scan sessions.
3. Implement incremental rescans.

## Phase 3: UI

1. Build the application shell.
2. Show live scan results.
3. Add tree, treemap, and detail views.

## Phase 4: Performance

1. Add benchmark datasets.
2. Measure scan throughput and memory usage.
3. Optimize hot paths.

## Phase 5: Release

1. Package the app.
2. Add crash recovery.
3. Document setup and usage.
