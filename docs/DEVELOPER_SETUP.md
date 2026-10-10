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

- Startup trace: `%TEMP%\WinBlaze-startup.log` (enable routine tracing with `WINBLAZE_STARTUP_TRACE=1`; failures remain logged)
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


## Parity feature checks

### Release OS XAML desktop host

Release/x64 builds now stage the OS XAML desktop host from the current UI
sources. Run MSBuild from a Visual Studio developer shell; its post-build target
uses PowerShell 7, records source/binary hashes, and backs up previous binaries
under `artifacts/` before staging the four Release binaries. Debug builds retain
WinUI. Use `/p:WinBlazeUiHost=WinUI` for an explicit Release fallback.

To build and validate an isolated OS XAML output after restoring UI NuGet packages:

```powershell
pwsh -NoProfile -File scripts/build-os-xaml-ui.ps1
pwsh -NoProfile -File tests/ui/os-xaml-build.ps1
pwsh -NoProfile -File tests/ui/os-xaml-package.ps1 -BuildDirectory artifacts/os-xaml-ui/full-os-candidate -OutputRoot artifacts/os-xaml-package
```

The builder discovers Visual Studio, Windows SDK and restored package headers,
builds the native DLL, CLI and updater, and records source and binary hashes.
Generated sources and outputs stay under repository `artifacts/`. Reviewed
source transformations adapt the current full UI to the OS XAML runtime; they
include owned asynchronous shutdown and its desktop window adapter. The host
retains bitmap window icons, PMv2, SegmentHeap and first-use imports for all six
DLLs declared in the project. Import checks use Python; UI checks use Windows
PowerShell 5 in a desktop session. The Release directory's `winblaze-host.json`
records its staged host, source hashes, binary hashes and backup directory.

For an explicitly selected portable package, use
`scripts/package-portable.ps1 -UiHost OsXaml -BuildDirectory artifacts/os-xaml-ui/full-os-candidate`.
The packager verifies all four binaries against the build inventory and uses a
separate `-os-xaml-portable` name. The default packaging option retains its
legacy WinUI directory layout.
The default packaging path includes the Release directory's existing WinUI
sidecars even when its executable is the OS XAML host. The explicit OS XAML
package verifies the smaller four-binary inventory. Benchmark medals require
fresh observations against the staged binaries; isolated build results do not
replace those observations. See `benchmarks/ITERATION_26_RELEASE_OS_XAML.md` for
runtime evidence and remaining validation limits.

`cargo build --release -p winblaze-native` builds both the C ABI DLL and the shipped `target/release/winblaze-cli.exe`. The portable packaging script stages the CLI; the MSI definition includes it too.

Run the real Windows feature regressions in a desktop session:

```powershell
powershell -NoProfile -File tests/ui/parity.ps1
powershell -NoProfile -File tests/ui/import-scan.ps1
powershell -NoProfile -File tests/ui/export-features.ps1
```

Build `tests/native/file-actions.vcxproj` and `tests/native/directory-watcher.vcxproj` with MSBuild Release/x64. Each executable requires its own generated fixture directory argument. These tests only operate on files they create. The full refresh builds both adapters before timing when `-SkipBuild` is omitted:

```powershell
pwsh -NoProfile -File benchmarks/run-winblaze-refresh.ps1 -Runs 10
```

Administrator raw-volume runs remain separate; `run-feature-comparison.ps1 -OnlyMft -Runs 10` measures both WinBlaze and WizTree raw MFT exports with an administrator token. MFT/ACL outputs are validated then removed; aggregate timing, CPU/memory samples and binary identities are retained. Never lower repetition counts when rerunning benchmarks.
