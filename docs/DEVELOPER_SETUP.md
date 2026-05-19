# Developer Setup

## Prerequisites

- Windows 11 or current Windows 10.
- Visual Studio 2022 Build Tools with C++ workload.
- Windows App SDK/WinUI 3 dependencies available to the C++ project.
- Rust stable toolchain.
- PowerShell 5 or later.

## Build Rust Workspace

```powershell
cargo test -q
```

This builds and tests the core, scanner, index, native bridge, and workspace test
crates.

## Build Debug UI

```powershell
& "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe" src\WinBlaze.UI\WinBlaze.UI.vcxproj /p:Configuration=Debug /p:Platform=x64 /m /nologo /v:minimal
```

The Debug executable is:

```text
src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe
```

If linking fails with `LNK1168`, close any running `WinBlaze.UI.exe` process and
build again.

## Local Gate

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\check-local.ps1 -Configuration Debug
```

The local gate runs Rust tests, compiles Rust examples, builds the WinUI
executable, runs positive and negative UI smoke, runs the tiny UI benchmark, and
creates a portable package. Use
`-RunBenchmarkBudgets` to also run the budgeted
tiny/fanout/fanout-large/scale Debug UI benchmark set,
`-RecordCompetitors` to also record local competitor tool inventory,
`-CheckSigning` to run the signing prerequisite check, or `-CheckInstaller` to
run the installed-build tooling check. Use
`-SkipUiSmoke`, `-SkipNegativeUiSmoke`, `-SkipBenchmarks`, or `-SkipPackage`
when isolating failures. Use `-AutoSkipUiSmokeIfUnavailable` on non-interactive
hosts where UI Automation prerequisites are expected to fail.

## UI Smoke Test

Check UI Automation prerequisites:

```powershell
powershell.exe -ExecutionPolicy Bypass -File tests\ui\check-prereqs.ps1
```

Run the smoke test:

```powershell
powershell.exe -ExecutionPolicy Bypass -File tests\ui\smoke.ps1
```

The smoke test launches the Debug app, scans a small fixture, checks search,
diagnostics, treemap rendering, cancel, recent crash logs, and closes the app.

For missing-root diagnostics:

```powershell
powershell.exe -ExecutionPolicy Bypass -File tests\ui\negative.ps1
```

## Benchmark Fixtures

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size tiny -Clean
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size tiny
```

Use `small`, `medium`, `fanout`, `fanout-large`, or `scale` when broader
performance evidence is needed. `fanout-large` stresses one dense sibling list;
`scale` exercises a larger file count with zero-byte files to keep disk usage
low. Use `benchmarks\record-environment.ps1` with baseline runs so results carry
machine and storage context.

## Runtime Data

- Startup trace: `%TEMP%\WinBlaze-startup.log`
- Native scanner/index events: `%LOCALAPPDATA%\WinBlaze\logs\events.jsonl`
- Failure reports: `%LOCALAPPDATA%\WinBlaze\logs\failures.jsonl`
- Binary index snapshot: `%LOCALAPPDATA%\WinBlaze\index\winblaze.index.bin`

## Current Development Notes

- The stable startup path is the code-built MVP shell in `MainWindow.cpp`.
- Legacy full-shell variants have been removed from active source. Add new shell
  sections as small, verified changes on top of the stable MVP shell.
- Do not reintroduce WinUI `ProgressBar` into the active scan path until startup
  and smoke coverage prove it safe.
- A direct active-shell `NavigationView` host wrapper previously built but
  crashed inside `Microsoft.UI.Xaml.dll` after activation; use small verified
  navigation changes until that is isolated.
