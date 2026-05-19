# Release Checklist

## Preflight

- Confirm `TODO.md` has no release-blocking Priority 0 or Priority 1 items.
- Confirm release notes describe the current stable MVP shell scope.
- Confirm version/build ID is recorded in `docs\RELEASE_NOTES.md` and matches
  `src\WinBlaze.UI\Package.appxmanifest`.

## Build And Test

For a local Debug preflight, run:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\check-local.ps1 -Configuration Debug
```

For Release candidates, run the individual release commands below so Release
artifacts and benchmark medians are captured explicitly.

```powershell
cargo test -q
& "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe" src\WinBlaze.UI\WinBlaze.UI.vcxproj /p:Configuration=Release /p:Platform=x64 /m /nologo /v:minimal
powershell.exe -ExecutionPolicy Bypass -File tests\ui\smoke.ps1 -AppPath src\WinBlaze.UI\bin\x64\Release\WinBlaze.UI.exe
powershell.exe -ExecutionPolicy Bypass -File scripts\package-portable.ps1 -Configuration Release
```

## Benchmark

```powershell
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size tiny -GenerateDataset
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size small -GenerateDataset
powershell.exe -ExecutionPolicy Bypass -File benchmarks\record-environment.ps1
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-baseline-set.ps1 -Profiles tiny,fanout,fanout-large,scale -GenerateDatasets -EnforceBudgets
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-baseline-set.ps1 -AppPath src\WinBlaze.UI\bin\x64\Release\WinBlaze.UI.exe -Profiles tiny,fanout,fanout-large,scale -GenerateDatasets -EnforceBudgets -BudgetPath benchmarks\performance-budgets.release.json
powershell.exe -ExecutionPolicy Bypass -File benchmarks\estimate-index-scale.ps1 -SampleFiles 100000
powershell.exe -ExecutionPolicy Bypass -File benchmarks\run-ui-incremental-benchmark-suite.ps1 -AppPath src\WinBlaze.UI\bin\x64\Release\WinBlaze.UI.exe -Size tiny -Mutations add,remove,modify -GenerateDataset -OutputPath benchmarks\winblaze-incremental-baseline.json
powershell.exe -ExecutionPolicy Bypass -File scripts\package-installer.ps1 -Configuration Release -Platform x64 -ValidateOnly
```

Record median results from at least three runs for each selected dataset.

## Hardware Validation

- Run a removable-drive scan and physically disconnect the drive during traversal.
  Confirm the scan reports transient issues and the app remains responsive.
- Run at least one larger hardware-calibrated dataset beyond the checked-in
  generated profiles before claiming release-scale parity.
  `docs\P2_COMPLETION_EVIDENCE.md` describes the deterministic P2 gates that
  should already pass before this manual validation.

## Diagnostics

- Verify `%TEMP%\WinBlaze-startup.log` rotates.
- Verify `%LOCALAPPDATA%\WinBlaze\logs\events.jsonl` rotates.
- Verify `%LOCALAPPDATA%\WinBlaze\logs\failures.jsonl` rotates.
- Verify failure report export:

  ```powershell
  powershell.exe -ExecutionPolicy Bypass -File scripts\export-failure-report.ps1
  ```

## Packaging

- For portable preview releases, include app binaries, native DLL, resources,
  README, troubleshooting docs, and license metadata.
- Confirm the CI `WinUI build and portable package` job uploaded the portable zip
  artifact for the candidate commit.
- For installed releases, verify installer, uninstall, shortcuts, signing, and
  app data retention.
- Generate and inspect release/update metadata:

  ```powershell
  powershell.exe -ExecutionPolicy Bypass -File scripts\write-update-manifest.ps1
  ```

- Check signing prerequisites before release-channel builds:

  ```powershell
  powershell.exe -ExecutionPolicy Bypass -File scripts\check-signing-prereqs.ps1
  ```

## Clean First Run

- Test on a Windows user profile without `%LOCALAPPDATA%\WinBlaze`.
- Confirm app launches visibly.
- Confirm first scan creates logs and index directories.
- Confirm smoke test passes.

## Rollback

- Keep the previous release package available.
- Confirm older builds reject unsupported cache versions cleanly or require a
  rescan.
- Do not delete user app data during rollback unless explicitly requested.
