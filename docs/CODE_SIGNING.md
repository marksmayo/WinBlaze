# Code Signing

## Goal

Release builds should be Authenticode-signed before distribution. The signing
script and conditional CI step exist, but release signing remains inactive until
a certificate is configured in the local environment or CI secrets.

## Expected Workflow

1. Build Release x64 artifacts.
2. Sign `WinBlaze.UI.exe`.
3. Sign `winblaze_native.dll`.
4. Package portable or installed release artifacts.
5. Sign the MSI installer artifact when one is produced.
6. Timestamp signatures with a trusted timestamp server.
7. Verify signatures before publishing.

## Local Command Shape

Check local signing prerequisites with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\check-signing-prereqs.ps1
```

The preflight reports `signing_ready`, `signtool.exe` availability, certificate
thumbprint/path configuration, certificate discovery, and timestamp URL state
for the current environment.

Set `WINBLAZE_SIGNING_THUMBPRINT` or pass `-CertificateThumbprint` once a
certificate is available.

Sign and verify the default Release binaries with:

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts\sign-artifacts.ps1 -Configuration Release -Platform x64
```

`scripts\sign-artifacts.ps1` accepts either `WINBLAZE_SIGNING_CERT_PATH` plus
`WINBLAZE_SIGNING_CERT_PASSWORD`, or `WINBLAZE_SIGNING_THUMBPRINT`. Pass
`-IncludeInstaller` after MSI packaging to include any `artifacts\installer\*.msi`
files in the default signing set, or pass explicit `-Files` paths.

## CI Requirements

- Store signing credentials in the CI secret store:
  `WINBLAZE_SIGNING_CERT_BASE64` and `WINBLAZE_SIGNING_CERT_PASSWORD`.
- Avoid exporting private keys to the workspace.
- Ensure signing happens after the Release build and before packaging.
- Publish verification logs with release artifacts.

## Current Status

- No signing certificate is configured.
- A conditional CI signing step exists for Release builds when signing secrets
  are present. CI also signs a produced MSI artifact when both signing secrets
  and the installer artifact are available.
- Local signing prerequisite checks are scripted through
  `scripts\check-signing-prereqs.ps1`, including explicit readiness reporting.
- Local signing and verification are scripted through
  `scripts\sign-artifacts.ps1`; verification now fails the script when
  `signtool` reports a missing or invalid signature.
- Release checklist requires signing only for installed/release-channel builds.
- Portable development previews may remain unsigned until the signing path is
  available, but should be clearly labeled as development builds.
