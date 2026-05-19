# Packaging

## Build Types

WinBlaze should support two distribution shapes.

## Portable Build

Portable builds are zip-style distributions for development, diagnostics, and
power users.

- Contains `WinBlaze.UI.exe`, `winblaze_native.dll`, WinUI/App SDK runtime
  dependencies that are legally redistributable, PRI/resources, and docs.
- Runs without installer registration.
- Stores runtime data in the same user-local locations as installed builds:
  `%LOCALAPPDATA%\WinBlaze\index` and `%LOCALAPPDATA%\WinBlaze\logs`.
- Does not auto-update.
- Is the preferred early preview format until installer/signing work is ready.

Create a portable package from an existing build with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\package-portable.ps1 -Configuration Debug -Platform x64 -Clean
```

The script stages build outputs under `artifacts\portable\<package>` and writes
`artifacts\portable\<package>.zip`.

## Installed Build

Installed builds are the release-channel distribution.

- Uses an installer and uninstall entry.
- Installs Start menu shortcuts and optional desktop shortcut.
- Uses code signing.
- May include an update mechanism after the v1 packaging path is stable.
- Keeps user data in `%LOCALAPPDATA%\WinBlaze` so uninstall/reinstall does not
  destroy indexes unless the user explicitly opts in.

Check installed-build tooling with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\check-installer-prereqs.ps1
```

The preflight detects WiX v4+ (`wix.exe`) or WiX v3 (`candle.exe`/`light.exe`)
and verifies that the installer source and packaging script are present.

Build the MSI from the current portable staging layout with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\package-installer.ps1 -Configuration Release -Platform x64 -Version 0.1.0
```

Validate the installed-build staging layout without requiring WiX with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\package-installer.ps1 -Configuration Release -Platform x64 -ValidateOnly
```

The installer script creates or reuses
`artifacts\portable\WinBlaze-<Configuration>-<Platform>-portable`, validates the
expected app/runtime/docs files, and invokes WiX v4 against
`installer\WinBlaze.wxs`. The MSI is written under `artifacts\installer`.
Omit `-Version` to read the semantic version from
`src\WinBlaze.UI\Package.appxmanifest`.

The CI WinUI Release job runs the installer preflight and conditionally runs the
MSI package step when WiX v4+ is available on the runner. If signing secrets and
an MSI are both available, CI signs the installer before upload. Missing WiX
skips only the MSI artifact; portable packaging remains required.

Create an update/release manifest for published artifacts with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\write-update-manifest.ps1
```

The manifest records schema version, release channel, artifact file names,
sizes, and SHA-256 hashes. It is a release metadata handoff for the future
in-app or installed update checker; WinBlaze does not auto-apply updates yet.
By default the manifest version comes from the app manifest.

## Current Status

- Debug builds are produced by the WinUI project under
  `src\WinBlaze.UI\bin\x64\Debug`.
- A portable zip packaging script exists at `scripts\package-portable.ps1`.
- Installer prerequisite checks exist at `scripts\check-installer-prereqs.ps1`.
- MSI scaffold exists at `installer\WinBlaze.wxs`, with build entry point
  `scripts\package-installer.ps1`. The script supports `-ValidateOnly` for
  WiX-free staging checks, and CI runs both installer preflight and staging
  validation before conditionally uploading an MSI artifact when WiX is
  available. Local MSI compilation is blocked until WiX is installed.
- Update manifest generation exists at `scripts\write-update-manifest.ps1`; the
  app does not yet consume or apply update manifests.
- Signing and verification are scripted at `scripts\sign-artifacts.ps1`, and CI
  has a conditional Release signing step that runs only when signing secrets are
  configured.
- First release packaging should start with a portable zip, then verify the MSI
  on a clean Windows profile and add installed-build gates.

## Release Packaging Checklist

1. Run `scripts\check-local.ps1` for a Debug preflight.
2. Build Release x64 UI.
3. Run `cargo test -q`.
4. Run `tests\ui\smoke.ps1` against the release executable.
5. Run `benchmarks\run-ui-benchmark.ps1` against at least the `tiny` and `small`
   generated datasets.
6. Verify logs rotate and failure report export works.
7. Run `scripts\package-portable.ps1` and inspect the staged files and zip.
8. Run `scripts\package-installer.ps1 -ValidateOnly`; when WiX is available,
   run `scripts\package-installer.ps1`, install the MSI, verify
   shortcuts/uninstall metadata, and run smoke against the installed app.
9. Run `scripts\write-update-manifest.ps1` for the candidate artifacts and
   verify hashes match the files being published.
10. Validate first run on a clean Windows user profile.

## Upgrade And Rollback Behavior

- User data lives under `%LOCALAPPDATA%\WinBlaze` and is shared by portable and
  installed builds.
- App binaries can be rolled back independently of user logs and cache files.
- If an older binary cannot read a newer cache format, it should reject the
  snapshot and require a rescan rather than trying to partially load it.
- Pre-release installers should not delete `%LOCALAPPDATA%\WinBlaze` on uninstall
  unless the user explicitly asks to remove app data.
- A release should include the previous portable package until the installed
  update path is proven.
- Update manifests are additive metadata. They must not require deleting
  `%LOCALAPPDATA%\WinBlaze` and should point users to a full replacement package
  until an in-app updater exists.
- Failure report export should work before and after upgrade so rollback issues
  can be diagnosed.
