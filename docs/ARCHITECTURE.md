# Architecture

## Goals

- Scan large NTFS volumes quickly
- Keep UI responsive during active scans
- Persist indexed results for fast startup and incremental rescans
- Support benchmarking against competitor tools

## Proposed Subsystems

### Core

Shared data contracts, aggregation logic, and scan state.

### Scanner

Low-allocation filesystem traversal and metadata extraction.

Implementation choice: Rust for the scanner core, with a narrow FFI boundary
to keep the hot path fast and memory-safe.

### Index

Persistent storage, change tracking, and incremental update logic.

Initial storage choice: SQLite for rapid delivery and reliable recovery, with a
custom binary cache reserved for later optimization if profiling warrants it.

### UI

Window shell, treemap, folder tree, search, and detail panes.

Implementation choice: WinUI 3 with C++/WinRT to get a modern native shell
while preserving responsiveness during scans.

Rendering choice: Direct2D/SwapChainPanel-backed drawing for treemap and other
high-density views.

### Benchmarks

Repeatable datasets and timing/memory measurement.

## Immediate Design Questions

- Which WinUI 3 hosting strategy should we standardize on?
- What cache schema and migration strategy should the index use?
- How should scan events be streamed to the UI?
