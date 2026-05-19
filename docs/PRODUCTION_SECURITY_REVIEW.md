# Production Security And Privacy Review

This review covers the current v0.1.0 release-candidate shape. WinBlaze does not currently implement network telemetry or remote crash upload; production risk is mainly local path metadata, local cache/log retention, installer trust, and robustness against malformed local files.

## Local Data

- Cache snapshots are stored under `%LOCALAPPDATA%\WinBlaze\index` and contain scanned file and directory path metadata.
- JSONL diagnostic logs are stored under `%LOCALAPPDATA%\WinBlaze\logs` and can include scanned paths, issue paths, failure messages, timing counters, and cache diagnostics.
- Troubleshooting exports or copied log snippets may reveal usernames, project names, filenames, and mounted drive paths.

Release copy should say plainly that logs/cache are local by default and may contain file path metadata. Users should review logs before sharing them.

## Implemented Hardening

- Binary index snapshots are versioned and magic-checked before loading.
- Corrupt primary snapshots fall back to the backup snapshot when available.
- Oversized collection counts and oversized/truncated strings are rejected before large allocations.
- Additional regression coverage now exercises oversized string lengths, invalid enum values, truncated records, and huge collection lengths.
- Native callback string ownership no longer intentionally leaks path/name/diagnostic strings during scan callbacks.
- Logs have size limits and rotation to avoid unbounded growth during broad scans.

## Remaining Release Risks

- Release artifacts must be Authenticode-signed and timestamped before public distribution.
- Installer install, uninstall, repair, upgrade, and rollback behavior still need physical validation outside the repo test harness.
- Physical removable-media hot-unplug, non-NTFS/exFAT fallback, OneDrive placeholder files, and low-permission folders still need release-candidate validation on real hardware.
- WinBlaze should keep telemetry out of scope for v0.1.0 unless opt-in UX, privacy copy, upload failure handling, and retention rules are designed and tested.
- Long-run soak tests should monitor working set, handle count, log growth, cache growth, and leftover processes.

## Release Wording Requirements

- State that WinBlaze writes local cache and diagnostic files under `%LOCALAPPDATA%\WinBlaze`.
- State that local logs/cache may include file and folder names.
- State that no telemetry or remote uploads are performed by default for v0.1.0.
- Document how to clear local state: delete `%LOCALAPPDATA%\WinBlaze` after closing the app, or use any future in-app clear-cache control when implemented.
