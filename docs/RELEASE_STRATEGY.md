# Release Strategy

## Versioning Model

- Use semantic versioning for the product line: `MAJOR.MINOR.PATCH`.
- Keep the workspace crates on the same `0.x.y` family until the first public 1.0 release.
- Use prerelease tags for unstable builds, such as `v0.1.0-alpha.1` or `v0.1.0-rc.1`.

## Tagging

- Tag release commits with `vMAJOR.MINOR.PATCH`.
- Treat git tags as the source of truth for release artifacts.
- Keep patch releases limited to bug fixes and packaging updates.
- Use minor releases for additive features.
- Reserve major releases for breaking API, file format, or compatibility changes.

## Artifact Naming

- Rust crates continue to publish with their workspace package versions.
- The Windows app package version should mirror the semantic release version using the
  `MAJOR.MINOR.PATCH.0` AppX/Package.appxmanifest format.
- Keep build metadata out of the published package identity unless a release process
  explicitly needs it.
- Local and CI packaging scripts read the default semantic release version from
  `src\WinBlaze.UI\Package.appxmanifest` through `scripts\get-release-version.ps1`.

## Release Process

1. Merge the release candidate into `main`.
2. Run the CI checks and release packaging workflow.
3. Tag the commit with the release version.
4. Produce signed installers and any portable artifacts.
5. Generate update/release metadata with artifact sizes and SHA-256 hashes.
6. Publish release notes summarizing:
   - user-visible changes
   - scanner/indexing behavior changes
   - crash and scan-failure reports from `%LOCALAPPDATA%\WinBlaze\logs\failures.jsonl`
   - compatibility notes
   - known issues and follow-up work

## Guardrails

- Do not cut a release without passing the CI gate.
- Do not change file format versions without an explicit migration note.
- Do not publish an update manifest whose hashes were generated from different
  artifacts than the files being uploaded.
- Keep release notes short, factual, and scoped to the shipped version.
