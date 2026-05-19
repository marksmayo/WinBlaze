# WinBlaze

WinBlaze is a high-performance disk usage analyzer for Windows.

## Repository Layout

- `src/WinBlaze.Core` - shared domain models and contracts
- `src/WinBlaze.Scanner` - filesystem enumeration and scan pipeline
- `src/WinBlaze.Index` - persistent indexing and incremental rescans
- `src/WinBlaze.Native` - Rust cdylib boundary for the UI shell
- `src/WinBlaze.UI` - WinUI 3 / C++/WinRT application shell
- `tests/WinBlaze.Tests` - unit and integration tests
- `benchmarks` - performance harness and benchmark data
- `docs` - architecture and implementation notes

## Current Status

WinBlaze currently runs as a stable WinUI 3/C++/WinRT MVP shell backed by the
Rust scanner and native bridge. The visible shell includes scan controls, cancel,
breadcrumbs, indexed search and filters, a catalog-backed virtualized tree/list,
details, diagnostics, and a Direct2D/SwapChainPanel treemap with DirectWrite
labels and GPU-surface hit testing.

The scanner and index core are implemented with compact binary snapshot
persistence. SQLite remains a documented option, but it is not the active runtime
backend. Incremental rescans are wired end to end through the UI and covered by
the checked-in smoke workflow.

Runtime diagnostics are available in the UI and through local logs:

- `%LOCALAPPDATA%\WinBlaze\logs\events.jsonl`
- `%LOCALAPPDATA%\WinBlaze\logs\failures.jsonl`
- `%TEMP%\WinBlaze-startup.log`

The old recovery-era branches and inactive full-shell variants have been removed
from the active startup route. The current focus is release validation,
packaging, signing readiness, and hardware/manual checks that cannot be covered
by deterministic repo tests.

## Developer Workflow

Build the Debug UI executable with MSBuild:

```powershell
& "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe" src\WinBlaze.UI\WinBlaze.UI.vcxproj /p:Configuration=Debug /p:Platform=x64 /m /nologo /v:minimal
```

Run the checked-in UI smoke test:

```powershell
powershell.exe -ExecutionPolicy Bypass -File tests\ui\smoke.ps1
```

The smoke test launches `src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe`, drives a
small scan under `C:\tmp`, checks search/diagnostics/treemap status, exercises
cancel, checks for recent WinBlaze application crashes, and closes the app.

## Project Docs

- [Architecture](docs/ARCHITECTURE.md)
- [Implementation Plan](docs/IMPLEMENTATION_PLAN.md)
- [Developer Setup](docs/DEVELOPER_SETUP.md)
- [Stack Decision](docs/STACK_DECISION.md)
- [Release Strategy](docs/RELEASE_STRATEGY.md)
- [Release Notes](docs/RELEASE_NOTES.md)
- [Supported Platforms](docs/SUPPORTED_PLATFORMS.md)
- [Product Definition](docs/PRODUCT_DEFINITION.md)
- [Scanner Strategy](docs/SCANNER_STRATEGY.md)
- [Index Strategy](docs/INDEX_STRATEGY.md)
- [Cache Migration](docs/CACHE_MIGRATION.md)
- [Incremental Indexing Scope](docs/INCREMENTAL_INDEXING_SCOPE.md)
- [Data Model](docs/DATA_MODEL.md)
- [UI Foundation](docs/UI_FOUNDATION.md)
- [Search and Filtering](docs/SEARCH_FILTERING.md)
- [Benchmark Methodology](docs/BENCHMARK_METHODOLOGY.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)
- [Packaging](docs/PACKAGING.md)
- [Code Signing](docs/CODE_SIGNING.md)
- [Release Checklist](docs/RELEASE_CHECKLIST.md)
