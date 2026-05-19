# Cache Migration And Versioning

## Current Format

- Runtime cache file: `%LOCALAPPDATA%\WinBlaze\index\winblaze.index.bin`
- Magic: `WBIX`
- Binary snapshot format version: `1`
- Temporary write path: `winblaze.index.tmp`
- Recovery path: `winblaze.index.bak`

The cache stores volume, scan-session, directory, file, lineage, and file change
set records. The active runtime backend is `IndexBackend::BinaryCache`.

## Compatibility Policy

- Readers must reject unknown binary snapshot versions.
- Writers should only emit the current `INDEX_FORMAT_VERSION`.
- If the format changes incompatibly, increment `INDEX_FORMAT_VERSION`.
- If the format changes compatibly, add regression tests that prove old snapshots
  still load before keeping the same version.
- If migration is not implemented for an old format, invalidating and rebuilding
  the cache is acceptable for pre-release builds.

## Migration Steps

1. Add tests that serialize representative old-format data.
2. Increment `INDEX_FORMAT_VERSION` for incompatible changes.
3. Add an explicit migration path or a deliberate invalidation path.
4. Verify corrupt-primary recovery still falls back to `.bak`.
5. Verify `invalidate_cache` removes primary, backup, and temp files.
6. Update `docs/INDEX_STRATEGY.md` with the new field order and version.
7. Run:

   ```powershell
   cargo test -q
   powershell.exe -ExecutionPolicy Bypass -File tests\ui\smoke.ps1
   ```

## Current Gaps

- There is no multi-version reader yet.
- There are no archived binary fixtures for historical formats yet.
- Release packaging has not implemented a user-facing rollback workflow yet.
