# WinBlaze Production Roadmap

Updated: 5 October 2026. This is the roadmap for shipping and maintaining a trustworthy Windows disk analyzer. Existing functionality still needs release-candidate validation; an unchecked item can mean missing evidence rather than missing implementation. Earlier completion notes are preserved in [ROADMAP_HISTORY.md](docs/ROADMAP_HISTORY.md).

## How to use this roadmap

- **P0 — release blockers:** finish before distributing a production candidate.
- **P1 — production qualification:** finish before declaring the supported release production-ready.
- **P2 — maintenance and growth:** schedule after the initial release; these are not prerequisites unless included in its advertised scope.
- Assign an owner, issue and acceptance evidence to each item. Close a validation task only against a named commit, exact packaged binaries and recorded environment.
- Use red/green TDD for implementation changes. Keep the 92% Rust line-coverage gate, strict Clippy and formatting checks. Every benchmark rerun uses **n=10**; preserve complete outputs and measurement boundaries.
- Public release requires every P0/P1 item to be complete, or explicitly excluded from supported scope with user-facing documentation and recorded approval. Medal dominance is a separate optimization objective, not proof of production readiness.

## Starting position and evidence gaps

Rust scanning/indexing, the C ABI, WinUI frontend, CLI exports, file actions, portable/MSI packaging scripts and CI already exist. Current optimization checks passed workspace/examples/Clippy/format and 92.11% line coverage. The next benchmark cycle is in progress; do not describe it as complete. Current binaries still need fresh administrator measurements and a refreshed portable package.

Repository inspection found these concrete gaps:

- `docs/RELEASE_STRATEGY.md` describes no in-app update consumption, but `MainWindow.cpp` contains download/install and launch-update flows.
- CI can skip signing and MSI generation without failing; this is acceptable for development builds, not a production release gate.
- `tests/ui/import-scan.ps1` expects an obsolete menu label, and `scripts/check-local.ps1` still invokes it.
- README, platform/security/release documents contain conflicting versions, Windows minimums, feature descriptions and historical performance claims.
- The updater copies files into the installation directory; interrupted/partial replacement and recovery need dedicated qualification.

## P0 — Freeze the supported product and release inputs

- [ ] **Define the release contract.** Agree the initial supported Windows versions, x64 architecture, filesystems, removable media, MTP devices, UNC/network paths, virtual disks and cloud-placeholder behavior. Label experimental or unsupported combinations explicitly.
- [ ] **Create one authoritative version.** Generate crate, AppX/manifest, executable-resource, About dialog, CLI, updater and MSI versions from one input; reject mismatches during packaging.
- [ ] **Reconcile documentation with shipped behavior.** Update README, `SUPPORTED_PLATFORMS.md`, `RELEASE_STRATEGY.md`, `RELEASE_NOTES.md` and `PRODUCTION_SECURITY_REVIEW.md`; remove obsolete claims and replace outdated menu names.
- [ ] **Freeze a release-candidate commit.** Include all intended source, tests, assets and scripts in a reviewable commit; build from a fresh checkout without relying on untracked files, local caches or developer DLLs.
- [ ] **Document operational limits.** Explain partial scans, inaccessible files, unknown sizes, logical versus physical totals, hardlinks, reparse policy, live-volume changes and raw-MFT privilege requirements.
- [ ] **Define release acceptance budgets.** Set supported scale, peak/private memory, startup-to-usable-window, cancellation latency, UI responsiveness and export correctness thresholds before the final measurements.

## P0 — Secure distribution and updates

- [ ] **Provision production signing.** Configure protected signing credentials, publisher identity, timestamping and key-rotation/revocation procedures. Keep secrets out of source, logs and distributable artifacts.
- [ ] **Make release signing mandatory.** A production workflow must fail when signing credentials, expected signatures, timestamps or required MSI artifacts are missing. Development builds may remain explicitly unsigned.
- [ ] **Verify every distributed executable.** Sign and verify the UI, native DLL, CLI, updater and MSI; verify the exact final package contents on a separate clean machine. Do not modify signed files afterward.
- [ ] **Anchor updater trust independently of release hosting.** Verify downloaded executables/helpers against trusted publisher signatures and an explicit rotation policy before executing or replacing them; a ZIP and matching SHA-256 from the same release are insufficient publisher authentication.
- [ ] **Harden extraction and staging.** Test path traversal, absolute paths, reparse points, hostile archive entries, oversized archives and concurrent same-user replacement. Verify the bytes actually consumed, not only an earlier download.
- [ ] **Make update application recoverable.** Stage a complete verified version, handle locked files/process-exit timeouts and preserve the previous version. Inject copy failure, disk-full, termination and reboot between replacement steps; restart a coherent old or new build.
- [ ] **Support installed and portable update semantics.** Test unwritable Program Files, required elevation, MSI-managed installations, custom locations and portable directories. Do not leave the MSI database inconsistent with an in-app replacement.
- [ ] **Test the update failure matrix.** Cover offline operation, HTTP errors, truncated downloads, wrong hashes/publishers, invalid manifests, downgrade attempts, cancellation, concurrent updates and failed relaunch. Fail safely with actionable feedback.
- [ ] **Review privileged boundaries.** Keep ordinary scans unelevated, request elevation only where needed, and ensure elevated helpers cannot consume attacker-controlled paths or replacement binaries without validation.
- [ ] **Complete dependency and artifact review.** Run current Rust advisory checks; inventory native/runtime/NuGet dependencies, licenses and redistribution terms. Produce dependency notices and an SBOM for the shipped package.

## P0 — Protect user data and validate destructive actions

- [ ] **Revalidate file identity before mutation.** For rename/move/recycle, hardlink replacement, compression and sparsification, handle files changing between selection and execution; test reparse substitution and permission changes.
- [ ] **Make action boundaries explicit.** Confirm intended targets and consequences, prevent silent overwrite, report partial failures, and preserve unrelated files. Keep imported/offline/stale-catalog actions disabled or require validated live identities.
- [ ] **Qualify hardlink deduplication.** Require byte verification and compatible same-volume targets; cover concurrent content changes, existing aliases, metadata/ACL differences, failures and accurate reclaimable-byte estimates.
- [ ] **Qualify storage mutations.** Verify file contents, sizes and allocation before/after compression or sparsification; exercise interruption, permissions, open handles and unsupported filesystem behavior.
- [ ] **Use isolated fixtures for safety tests.** Exercise destructive APIs only in owned temporary directories or disposable virtual disks. Include collision, cancellation and rollback cases.
- [ ] **Bound every untrusted import.** Validate CSV/TSV/JSON/MFT and binary-cache sizes, counts, offsets, nesting and relationships before allocation; extend hostile/truncated-input corpus tests to current formats.
- [ ] **Handle hostile imported paths.** Reject or safely represent traversal, malformed roots, duplicate identities, cycles, embedded control characters, oversized fields and inconsistent totals; importing must never scan or mutate a referenced path implicitly.
- [ ] **Define safe spreadsheet export behavior.** Decide how CSV fields beginning with spreadsheet formula characters are handled without breaking lossless scan reopen; document the chosen export mode and test it.
- [ ] **Review the C ABI contract.** Version layouts, document string/handle lifetimes and ownership, contain panics/exceptions, validate supported inputs and test callback shutdown, cancellation, repeated destroy and allocation failures.

## P1 — Scanner and catalog correctness across supported environments

- [ ] **Establish independent correctness fixtures.** Compare file/directory identities, logical/allocation bytes and errors against known fixtures; matching another analyzer alone is not sufficient.
- [ ] **Qualify both scan backends.** Test standard-user directory walk and elevated MFT on supported NTFS configurations, including sector/cluster sizes, fragmented MFT extents and extension records. Verify transparent fallback with a visible reason.
- [ ] **Cover filesystem edge cases.** Include hardlinks, named streams, sparse/compressed/encrypted files, hidden/system entries, case-sensitive directories, long/Unicode paths, junctions, symlinks and mount points. Publish counting/exclusion semantics.
- [ ] **Test live-volume changes.** Create/delete/rename files during scans; ensure totals are internally coherent and identify the scan as a best-effort live view rather than a point-in-time snapshot.
- [ ] **Validate permissions and failures.** Access-denied, sharing violations, missing roots, unreadable metadata and device failures must produce partial-result/error indicators, not false successful zero-byte totals.
- [ ] **Run real non-NTFS/removable tests.** Exercise every filesystem promised by the release, removable-media disconnect/reconnect and empty/full devices on physical hardware.
- [ ] **Qualify MTP and virtual disks if supported.** Test locked/asleep/disconnected devices, empty roots, unknown sizes and timeouts; validate VHD/VHDX mount/unmount and cleanup on disposable media.
- [ ] **Qualify cloud/network behavior or exclude it.** Test placeholders without unintended hydration, UNC authentication/disconnection and remote latency before advertising support.
- [ ] **Verify cancellation and shutdown.** Bound cancellation, close-during-scan/import/export and hung-I/O behavior; no orphan process, worker, handle or stale callback may survive session teardown.
- [ ] **Test simultaneous work and isolation.** Define allowed concurrent scans, imports, exports and multiple app instances; prevent sessions or roots from contaminating each other's cached models.
- [ ] **Qualify incremental scans and monitoring.** Handle rename/delete, root switches, burst events, watcher overflow, missed changes and restart; fall back to a full refresh when correctness cannot be guaranteed.
- [ ] **Verify every report/layout.** Extension totals, largest files, duplicate/permission reports, treemap/flame/sunburst and PNG exports must reflect the complete catalog; distinguish truncated display previews from exported data.
- [ ] **Verify third-party scan interchange.** Test current supported WinDirStat/WizTree CSV variants, encoding, locale, quoting/newlines, partial metadata and import/re-export fidelity using versioned fixtures.

## P1 — Persistence, recovery and long-run reliability

- [ ] **Test atomic snapshot durability.** Inject interrupted writes, disk-full, antivirus locks and process termination; keep the previous valid snapshot and recover cleanly from primary/backup corruption.
- [ ] **Exercise migrations and rollback.** Maintain fixtures for every supported snapshot version; reject unknown future versions safely, support the documented rescan path and preserve user settings.
- [ ] **Define cache ownership and retention.** Identify caches by root/volume, prevent stale reuse after remount/reformat and implement clear-cache controls that cannot delete scanned user data.
- [ ] **Verify writable-data locations.** Installed, portable, read-only and roaming-profile environments must use documented cache/settings/log paths; separate elevation contexts must not expose other users' data.
- [ ] **Run multi-hour engine and UI soaks.** Repeat full/incremental scans, imports, exports, cancellation and reopen; track working/private memory, handles, threads, GPU resources, log/cache growth and leftover processes.
- [ ] **Test resource pressure.** Exercise low memory/disk, very large catalogs, pathological deep/wide trees and multiple windows; show clear errors and preserve the previous usable catalog.
- [ ] **Handle rendering recovery.** Test graphics-device loss, remote desktop, integrated/discrete GPUs and software fallback without crashes or permanent blank treemaps.

## P1 — UX, accessibility and support

- [ ] **Complete a first-run task review.** Make root selection, privilege/fallback status, scan progress, cancel, partial results and next actions clear. Validate maximized launch, folder controls, back navigation and empty/error states.
- [ ] **Validate usable startup.** Measure an actual visible, responsive window separately from the hidden first-idle proxy; avoid blocking startup on network checks or large cache restoration.
- [ ] **Run full keyboard and Narrator checks.** Cover tree paging, menus, dialogs, search, layouts, cancellation and file actions; maintain focus order, accessible names and meaningful announcements.
- [ ] **Test visual accessibility.** Check light/dark/high-contrast themes, 100–200% scaling, multiple monitors, narrow windows, large text and long paths. Ensure selection/errors are not conveyed only through color.
- [ ] **Qualify localization behavior.** Test non-English Windows, decimal/date formats, Unicode filenames and exports. State the shipped UI languages; do not imply localization that has not been tested.
- [ ] **Document privacy and data controls.** Explain local path/ACL metadata, raw-MFT export sensitivity, log/cache retention, update-network access and the absence of telemetry. Offer deliberate diagnostic sharing/redaction.
- [ ] **Provide useful failure diagnostics.** Include version/build/backend/error context, bounded rotating logs and an explicit support-bundle export; retain private symbols for actionable crash investigation.
- [ ] **Publish support guidance.** Document installation, privileges, filesystem limitations, importing/exporting, cleanup safety, recovery, issue reporting and known problems.

## P1 — Reproducible CI, packaging and performance qualification

- [ ] **Repair obsolete automation.** Update `import-scan.ps1` and callers to current menus/dialogs; make Release paths explicit and ensure the local gate fails on each external tool's nonzero exit code.
- [ ] **Run the whole supported regression suite.** Include current native and UI parity tests in CI or a documented interactive Windows qualification job. Record unavailable/manual checks as pending, never as passed.
- [ ] **Validate a clean toolchain restore.** Pin Rust/Windows SDK/WinUI/NuGet/WiX and relevant build-tool inputs; prove a clean checkout produces the intended Release binaries with no stale Debug/native mixture.
- [ ] **Protect release automation.** Use least-privilege jobs, reviewed dependency updates, protected release tags/environments and a defined approval/publishing process. Keep artifact provenance and signing logs.
- [ ] **Install on clean supported machines.** Validate first launch without developer tools, required VC++/Windows App Runtime dependencies, offline installation expectations, standard-user operation and missing-runtime guidance.
- [ ] **Exercise the MSI lifecycle.** Fresh install, upgrade, repair, uninstall/reinstall, downgrade refusal and failed-install rollback must preserve documented user data and leave correct shortcuts/permissions.
- [ ] **Exercise portable deployment.** Test extraction/moving, paths with spaces/Unicode, unwritable directories and antivirus scanning; verify all needed DLLs/resources/helpers/CLI are bundled.
- [ ] **Verify the final artifacts.** Compare packaged UI/DLL/CLI versions and hashes to the tested build, verify signatures, and generate manifests only after signing. Test the exact ZIP/MSI intended for publication.
- [ ] **Finish the active performance cycle.** Complete current standard-user and administrator n=10 scans/features/startup tests, regenerate the HTML report and preserve competitor versions/hashes. Do not combine replaced-binary observations into current medals.
- [ ] **Validate measurement quality.** Treat missing/exit-race telemetry as missing, separate first idle from visible readiness, verify export outputs and record OS/hardware/elevation/cache conditions and exceptions.
- [ ] **Broaden calibration.** Run supported low-memory, HDD/SATA/NVMe and Windows configurations. Measure p50/p90, CPU, peak/private memory, cancellation and responsiveness against release budgets.
- [ ] **Enforce meaningful regression budgets.** Keep independent correctness checks, stable owned datasets and n=10 performance qualification. Preserve existing event definitions; pursue the all-35-gold objective without weakening correctness or manufacturing medals.
- [ ] **Qualify the offline report.** Verify static results without JavaScript, responsive measurement tables, fixed podium, missing-data explanations, version provenance and no external-network dependency.

## P1 — Release and operational readiness

- [ ] **Run a complete release-candidate rehearsal.** Build, test, sign, package, install, scan, export, update, recover and uninstall using the published instructions on a clean machine.
- [ ] **Record a release evidence bundle.** Include commit/version, artifact hashes/signatures, CI results, supported-machine matrix, benchmark datasets, manual checks and known limitations.
- [ ] **Publish precise release notes.** Describe supported features and migration/recovery steps; separate historical performance from current comparable results and remove stale project statistics.
- [ ] **Prepare incident and rollback procedures.** Retain the previous trusted installer/portable build, define security reporting and release withdrawal, and test user-data-preserving rollback.
- [ ] **Define maintenance ownership.** Assign responsibility for dependency/security updates, regressions, crash triage, support requests and periodic clean-machine qualification.
- [ ] **Complete the go/no-go checklist.** Review remaining blockers with named owners; publish only the tested, signed artifacts from the accepted candidate. Update `docs/RELEASE_CHECKLIST.md` to match these gates and n=10 policy.

## P2 — After the supported release is qualified

- [ ] Deepen oversized UI/FFI modules around scan sessions, update application, file actions and catalog queries; preserve public behavior and benchmark before/after.
- [ ] Continue measured CPU/memory/startup improvements, including bounded capacity planning and reduced unused CLI/UI work, with explicit trade-offs and red/green evidence.
- [ ] Add a sustained parser-fuzzing pipeline, sanitizer-capable native test configuration and broader concurrency/fault-injection coverage.
- [ ] Expand supported hardware/OS/filesystem/device coverage only after qualification; evaluate ARM64 and localization separately from the x64 release.
- [ ] Consider snapshot-consistent scanning, advanced incremental indexing and additional enterprise deployment options as scoped product work.
- [ ] Review long-term settings persistence, accessibility feedback and benchmark thresholds after real-world beta feedback.

## Suggested execution order

1. Resolve the P0 product/version/documentation contract and updater trust/recovery design.
2. Finish P0 signing and data-safety tests while repairing automation and clean builds.
3. Qualify scanner/catalog/recovery behavior, packaging and supported-machine coverage.
4. Complete UX/accessibility, soaks and n=10 performance budgets on the candidate.
5. Sign/package once, run the end-to-end release rehearsal, record evidence and make the go/no-go decision.

Useful entry points: [local gate](scripts/check-local.ps1), [CI](.github/workflows/ci.yml), [release checklist](docs/RELEASE_CHECKLIST.md), [security review](docs/PRODUCTION_SECURITY_REVIEW.md), [packaging](docs/PACKAGING.md), [live benchmark report](benchmarks/CURRENT_COMPARISON.html).
