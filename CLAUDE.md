# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

WinBlaze is a high-performance disk usage analyzer for Windows that provides fast NTFS scanning with real-time visualization. It uses Rust for the scanning backend and C++/WinRT with WinUI 3 for the frontend, featuring GPU-accelerated Direct2D treemap rendering.

## Key Commands

### Building
```powershell
# Run Rust tests
cargo test -q

# Build Debug UI (requires VS2022 Build Tools)
& "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe" src\WinBlaze.UI\WinBlaze.UI.vcxproj /p:Configuration=Debug /p:Platform=x64 /m /nologo /v:minimal

# Full local gate check (tests + build + smoke tests)
powershell.exe -ExecutionPolicy Bypass -File scripts\check-local.ps1 -Configuration Debug
```

### Testing
```powershell
# Run UI smoke tests
powershell.exe -ExecutionPolicy Bypass -File tests\ui\smoke.ps1

# Generate benchmark datasets
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size tiny -Clean

# Run performance benchmarks
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size tiny
```

## Architecture

The codebase follows a multi-layer architecture with FFI boundary between Rust and C++:

- **src/WinBlaze.Core/** - Shared domain models and data contracts
- **src/WinBlaze.Scanner/** - Rust filesystem scanner with NTFS MFT optimization
- **src/WinBlaze.Index/** - Persistent storage and incremental update logic  
- **src/WinBlaze.Native/** - Rust cdylib providing FFI bridge to C++
- **src/WinBlaze.UI/** - WinUI 3 application with Direct2D treemap rendering

### Key Design Patterns

1. **Performance-First**: Scanner hot path in zero-allocation Rust, batched UI updates, GPU-accelerated rendering
2. **Streaming Architecture**: Real-time progress updates from scanner to UI via callbacks
3. **Error Resilience**: Comprehensive filesystem error handling with fallback strategies
4. **Testing Strategy**: Unit tests (Rust), UI automation tests (PowerShell), performance benchmarks with budgets

### Technology Stack

- **Backend**: Rust with NTFS MFT enumeration, binary snapshot persistence
- **Frontend**: C++/WinRT, WinUI 3, Direct2D/DirectWrite for GPU rendering
- **Testing**: Rust tests, PowerShell UI automation, performance regression tests
- **Logging**: Structured JSONL logs to %LOCALAPPDATA%\WinBlaze\logs\

## Development Notes

- Always run `scripts\check-local.ps1` before committing to verify tests, build, and smoke tests pass
- Performance budgets are enforced - check `benchmarks/performance-budgets.json` for limits
- UI virtualization limits: 256-row pages, 8,192-entry catalog cap for responsiveness
- Rust formatting: 100-char width, reorder imports (see rustfmt.toml)
- The project uses semantic versioning with manifest-driven releases