# Troubleshooting

## App Does Not Appear

1. Check whether `WinBlaze.UI.exe` is still running:

   ```powershell
   Get-Process WinBlaze.UI -ErrorAction SilentlyContinue
   ```

2. Check the startup trace:

   ```powershell
   Get-Content "$env:TEMP\WinBlaze-startup.log" -Tail 80
   ```

3. Check recent Windows application errors:

   ```powershell
   Get-WinEvent -FilterHashtable @{LogName='Application'; StartTime=(Get-Date).AddMinutes(-10)} |
     Where-Object { $_.ProviderName -eq 'Application Error' -or $_.Message -like '*WinBlaze*' } |
     Select-Object TimeCreated,ProviderName,Id,Message
   ```

## Logs

WinBlaze writes local diagnostics only:

- Startup trace: `%TEMP%\WinBlaze-startup.log`
- Native scanner/index events: `%LOCALAPPDATA%\WinBlaze\logs\events.jsonl`
- Failure reports: `%LOCALAPPDATA%\WinBlaze\logs\failures.jsonl`
- Binary index snapshot: `%LOCALAPPDATA%\WinBlaze\index\winblaze.index.bin`

The JSONL logs rotate when they reach their size caps. The previous file is kept
with a `.1` suffix.

## UI Diagnostics

The Diagnostics section reports UI batching, frame timing, working set, scan
summary totals, issue count, issue-code breakdown, last issue path/message when
available, incremental added/removed/modified/renamed/moved counts, and
catalog-sample byte reconciliation.

## Smoke Test

Run the checked-in smoke test from the repository root:

```powershell
powershell.exe -ExecutionPolicy Bypass -File tests\ui\check-prereqs.ps1
powershell.exe -ExecutionPolicy Bypass -File tests\ui\smoke.ps1
```

The smoke test launches the Debug app, scans a small fixture under `C:\tmp`,
checks search, diagnostics, treemap rendering, cancel, and recent application
crashes, then closes the app.

Run the negative smoke test to verify missing-root diagnostics:

```powershell
powershell.exe -ExecutionPolicy Bypass -File tests\ui\negative.ps1
```

It scans a deliberately missing path and expects diagnostics to show an issue
code plus the last issue path/message.

## Common Recovery Steps

- Close any existing `WinBlaze.UI.exe` before rebuilding. A running app keeps the
  executable locked and can cause linker error `LNK1168`.
- If the UI loads old results, run a new scan for the intended root. The app
  reloads the binary snapshot after scan completion.
- If the index snapshot appears stale or corrupt during development, delete
  `%LOCALAPPDATA%\WinBlaze\index\winblaze.index.bin` and rerun a scan.
- If startup fails after a UI change, check `WinBlaze-startup.log` for the last
  completed `BuildShell` trace marker.

## Reporting Failures

Generate a local failure-report zip:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\export-failure-report.ps1
```

Include:

- the exact build configuration and commit/build ID
- the path scanned
- the last 80 lines of `%TEMP%\WinBlaze-startup.log`
- `%LOCALAPPDATA%\WinBlaze\logs\failures.jsonl`
- the recent Windows Application Error entry, if present
- whether `tests\ui\smoke.ps1` passes
