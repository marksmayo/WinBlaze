# UI regression checks

Build the Windows UI before running these scripts. `path-ux.ps1` defaults to the
Release executable; pass `-AppPath` to check another build.

```powershell
foreach ($case in 'empty','invalid','quoted','browse','expand','drive','window','version','guidance','cancel') {
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/ui/path-ux.ps1 -Case $case
    if ($LASTEXITCODE) { throw "Failed: $case" }
}
```

The checks exercise the real app through UI Automation: blank and invalid paths,
quoted paths, native folder-picker cancellation, folder expansion, drive selection,
maximized startup, restored-window layout, release identification, and guidance.
The cancellation check starts a standard-user system-volume scan and immediately
cancels it to verify that directory controls unlock again.
`negative.ps1` also verifies that missing folders and file paths preserve prior
scan results. `smoke.ps1` covers full scans, incremental changes, search, rendering,
and cancellation. `a11y-audit.ps1` checks accessible control names and keyboard focus.

Tests use isolated application caches and close only their own app process.
Path checks create unique fixtures under `artifacts/`. For smoke tests, supply
dedicated fixture directories; that script recreates the supplied directories.

Capture a populated window for visual review:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/ui/path-ux.ps1 `
    -Case expand -ScreenshotPath artifacts/ui-modern-maximized.png
```

Capture falls back to window-only rendering when desktop capture is unavailable.
Inspect the resulting image; a successful capture alone does not establish visual quality.
The local validation gate includes the directory and window checks when UI smoke
tests are enabled.


## CSV interoperability and image export

Run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/ui/import-scan.ps1` after a Release build. It opens a 9,001-file offline CSV from the File menu, re-exports every record to JSON, checks a nonblank 1920x1080 treemap PNG with dense-area coverage, and navigates largest-file pages in Cleanup.

`tests/ui/export-features.ps1` exercises full JSON and advanced filtered CSV through real save dialogs: multiple patterns, min/max bytes and invalid-filter guidance. Native geometry and exact PNG pixel round-trip checks use `tests/native/treemap.vcxproj`.


## Duplicate, permission, file-action and monitoring parity

Run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/ui/parity.ps1` against the Release build. It scans a generated fixture, exports a verified duplicate CSV and complete ACL report through the real File menu, enables monitoring and verifies automatic refresh after adding a file, then disables monitoring. Cleanup checks that canceling Move preserves the source, confirming Move succeeds without overwriting, and confirming Recycle removes the generated file. It closes only its own app and uses an isolated cache.

The native FileActions and DirectoryWatcher tests cover rejected overwrites, file-only recycling, add/modify/remove notifications and ignored internal writes. Raw MFT export requires an administrator token and is verified separately by the n=10 feature runner; a standard-user UI check must not request privilege or claim a successful raw dump.

## Portable discovery without a connected device

Run both cases when Windows reports no discoverable portable devices:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/ui/parity-portable-no-device.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/ui/parity-portable-no-device.ps1 -WithCatalog
```

Each case repeats discovery three times. It requires the exact no-device guidance,
preserved scan root and fixture contents, unchanged persisted catalog, enabled
scan controls and normal shutdown. `-WithCatalog` publishes a changed fixture
catalog before every attempt, also checking that rescans work after discovery.
The empty case checks that discovery creates no catalog. Each repeat clears the
previous status, so stale guidance cannot satisfy the next attempt. Results and
failures are saved in an isolated `artifacts/ui-parity-*` directory with the
executable hash. `-AppPath` selects a candidate build; `-Repetitions` controls
the number of attempts.

These cases do not replace `parity-portable-devices.ps1`, which requires a
discoverable device and verifies selection cancellation. A discovery error or
an attached-device selection dialog fails the no-device test. No device files
are read or changed by this test.

`window-icons.ps1` checks the initialized owned window's small and taskbar
icons against its embedded icon resource at the displayed dimensions and
requires normal shutdown. Run with the default Release build or supply
`-AppPath`. It records pixel hashes and executable identity in
`artifacts/ui-icons-*`. This verifies native icon parity; it does not establish
caption accessibility or high-contrast rendering.

The opt-in OS XAML build is covered by `os-xaml-build.ps1` (PowerShell 7).
It builds from current repository sources, checks recorded input/binary hashes,
window icons, restored scan controls and visible directory expand affordances.
`path-ux.ps1 -Case expand -RequireVisibleTree` requires an owned, visible expand
button inside the actual tree viewport before invocation.

`os-xaml-package.ps1 -BuildDirectory <build>/full-os-candidate -OutputRoot <output>`
packages that exact inventory, extracts the ZIP, checks all four binary hashes,
scans an isolated fixture with the extracted CLI and validates the extracted UI's
icons and normal shutdown. These checks do not establish interactive keyboard,
theme, attached-device or updater end-to-end parity.

`os-xaml-focus.ps1 -BuildDirectory <build-root>` compiles the repository host
with a two-button test window. It checks native owner focus entry, preservation
of the last focused control, forward/reverse Tab routing, an empty island and
release of the closed window. Native calls and messages target only the test's
own window on its own thread. Reverse routing temporarily changes only that
thread's keyboard-state table and restores it immediately. It never uses
`SendInput` or attaches input queues. This is an adapter contract, not proof of
physical keyboard interaction with the full app. The OS XAML build gate includes
this check.
