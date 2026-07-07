#!/usr/bin/env pwsh
# Rust line-coverage gate.
#
# Enforces a minimum line coverage over the unit-testable Rust logic. Files that
# cannot be exercised in headless CI are excluded so the gate reflects code that
# `cargo test` can actually reach:
#   - WinBlaze.Native/src/bridge.rs, api.rs : the C ABI FFI layer, exercised by
#     the C++ WinUI automation tests (tests\ui\*.ps1), not `cargo test`.
#   - WinBlaze.Scanner/src/ntfs.rs, controller.rs, scheduler.rs : the raw-volume
#     MFT reader and threaded scan orchestration, which require a live NTFS
#     volume + Administrator (covered by live scans / the mft_scan_repro example;
#     the pure byte-parsers in ntfs.rs are additionally fuzz-tested).
#
# Requires: cargo-llvm-cov and the llvm-tools-preview component.
param(
    [int]$MinLines = 80
)
$ErrorActionPreference = "Stop"

$ignore = '(WinBlaze\.Native.src.(bridge|api)|WinBlaze\.Scanner.src.(ntfs|controller|scheduler))\.rs$'

Write-Host "Coverage gate: requiring >= $MinLines% line coverage (excluding CI-untestable FFI / raw-volume modules)."
& cargo llvm-cov --workspace --summary-only --ignore-filename-regex $ignore --fail-under-lines $MinLines
exit $LASTEXITCODE
