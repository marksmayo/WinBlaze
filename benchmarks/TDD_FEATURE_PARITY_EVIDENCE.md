# Feature parity: red/green evidence

Implemented in vertical public-behavior slices on 4 October 2026. Each failure below was observed before its implementation; follow-up regression checks use real parsers, catalogs, Windows UI, GPU pixels and PNG decoding.

| Behavior | Red evidence | Green verification |
|---|---|---|
| Reopen WinBlaze CSV offline | Missing public import API | Unicode/quoted filename round-trip; no source-volume access |
| Import WinDirStat CSV | Unsupported schema error | Empty folders, allocation totals and UTC timestamps preserved |
| Import WizTree CSV | Missing local-time import API | Banner, optional volume columns, directory attributes and explicit timezone conversion |
| Reject malformed/incomplete CSV | Duplicate directory accepted | Strict quotes/header, numeric fields, duplicate paths and incomplete directory reports rejected |
| Reopen file-only export | Missing parent directory error | Common root and missing folder hierarchy reconstructed |
| Atomic native import | Missing native import API | Successful model publication; failed import retains previous catalog/export |
| Date interoperability | Unix seconds stored as Windows ticks | Catalog FILETIME ticks convert to/from true CSV/JSON Unix seconds |
| Legacy saved CSV dates | Older FILETIME export could not reopen | Legacy ticks are recognized and re-exported as true Unix seconds |
| Atomic benchmark results | Missing atomic writer | Read-only-file failure preserves the previous JSON; successful replacement publishes a complete result |
| Drive-root/calendar handling | Drive-root rollup mismatch | Root paths reopen; impossible calendar dates rejected |
| Pattern/size/date filtered CSV | Missing filtered export interface | Case-insensitive `*`/`?`, multiple patterns and inclusive logical-size/UTC bounds |
| Validate filters before output | Missing filtered native API | Invalid bounds return failure without overwriting an existing destination |
| Complete extension totals | 40 extensions / 280 bytes instead of 43 / 301 | Completed native snapshot returns every extension; live preview stays bounded |
| Stable complete largest-file pages | Missing paging API | 10,051 tied files returned once in stable order across the 10,000-file cache boundary |
| Dense weighted treemap geometry | Missing shared geometry module | Positive subpixel rectangles retain area; omitted children retain aggregate weight |
| Dedicated PNG encoding | Missing PNG module | WIC-encoded image decodes with exact dimensions and pixel colors; bad stride rejected |
| CSV import UI | File menu command absent | Real File picker opens offline CSV; all 9,001 imported files survive JSON re-export |
| Treemap PNG UI | File menu command absent | Real Save As produces a nonblank 1920x1080 GPU treemap; >90% interior coverage for dense fixture |
| Largest-file navigation | Public backend tracer above | Actual Cleanup next/previous controls show 51–100 and 1–50 of 9,001 files |
| Advanced filter UI | Public filtered-export tracer above | Multiple patterns plus min/max bytes select the expected file; invalid input shows guidance |
| Feature medals/report | Filter support remained partial | Updated parity rows use full/gold only after implemented behaviors pass |

## Validation

`cargo test --offline --workspace`, strict workspace/all-target/all-feature clippy, rustfmt, and `scripts/check-coverage.ps1` passed. Rust line coverage: **93.29%**, required **92%**, with the repository's existing FFI/raw-volume exclusions.

Native geometry/PNG regression: `tests/native/treemap.vcxproj` and `artifacts/treemap/treemap.exe`. UI checks: `tests/ui/import-scan.ps1` and `tests/ui/export-features.ps1`; they own their app processes and use isolated caches/fixtures.

The benchmark runner now measures 12 WinBlaze feature jobs, n=10 each, including own/WinDirStat/WizTree imports, advanced filters, all largest-file pages, full extension totals and dedicated GPU PNG export. Timing, sampled CPU, working/private memory and validated output remain separate from feature-support medals. Administrator reruns are still pending after the earlier canceled prompt.

## Semantics and limits

CSV reopening is session-only and does not replace the persisted disk-scan cache. A fresh scan after importing starts a full scan rather than applying changes to an unrelated cached catalog. Inputs must be UTF-8 exports with file records; older WinBlaze FILETIME values are recognized and normalized on re-export; incomplete directory-only reports and unrelated volume roots are rejected. WizTree's timezone-free dates use this PC's Windows timezone rules; import on the exporting machine for correct dates. WinDirStat ISO UTC and WinBlaze Unix dates are timezone-independent.

Filtered export writes files only. Filename patterns are semicolon-separated wildcards; logical-byte and UTC-date bounds are inclusive. Missing timestamps do not pass a date filter. PNG is the dedicated 1920x1080 treemap, not a capture of the window; dense/budget-limited children use weighted aggregate tiles. Zero allocated-size files remain listed in the tree and largest-file view but have no positive treemap area.

Generated full-scan cache copies are pruned between GUI repeats, retaining the final cache for cache-load timing and preserving observation/log metadata. Atomic JSON publication prevents disk/write failures from truncating the previous result.


## Remaining parity: 4 October 2026

| Public behavior | Red evidence | Green evidence |
| --- | --- | --- |
| Complete content duplicates | New public detector/module missing | Exact byte copies grouped; same-size different-content files and later-block differences rejected |
| Hard-linked storage and duplicates | Hard-link aliases formed a false duplicate group; walker reported 10,000 allocated bytes | Two logical names/10,000 logical bytes retain only the OS-measured 8,192 allocated bytes; aliases are not reclaimable duplicates |
| Owner/group/DACL export | Missing permission descriptor API and analysis export | Real Windows descriptors include O/G/D, missing entries report errors, complete CSV schema validated |
| Atomic analysis exports | Existing catalog source was overwritten | Invalid kinds and source destinations are rejected before replacement; report staging protects prior output |
| Production scan CLI | Missing production binary target and staged executable | Scan, duplicate and permission commands pass on real fixtures; packaged CLI runs and exports successfully |
| Raw MFT command | Missing C ABI dump API | Invalid root and NUL path rejected without replacing prior output; positive live validation requires administrator access |
| Move/recycle | Missing file-action component | Existing destination rejected, move preserves the file, only regular files recycle; real UI cancellation preserves source and confirmed actions succeed |
| Recursive monitoring | Missing watcher component | Add/modify/remove notifications delivered; cache-like internal writes excluded; real UI automatically publishes an added file without a manual scan |

Rust workspace coverage passed at **92.77%** against the existing **92%** gate. Strict workspace/all-target/all-feature clippy, formatting, Release native/CLI/UI builds, native Windows tests and real UI parity tests passed. Public regressions are in `Scanner/tests/{content_duplicates,allocation,permissions}.rs`, `Native/tests/{analysis_exports,cli,mft_export}.rs`, `tests/native/{file-actions,directory-watcher}.vcxproj`, and `tests/ui/parity.ps1`.

The portable ZIP includes `winblaze-cli.exe`, and the MSI definition includes the same component. The MSI has not been installed by this task. ACL reports cover discretionary access rather than privileged SACL audit entries. File actions require explicit UI confirmation, reject directories/reparse points and never overwrite a move destination; Windows may require additional confirmation when recycling is unavailable.

Benchmark adapters use n=10: duplicates and ACL include process startup, full fixture scan/index/persistence and report export. Move/recycle and watcher adapters measure actual native operations on their own generated fixtures, excluding GUI interaction and rescan; the watcher observation includes a 100ms quiet interval. CPU and memory values remain sampled lower bounds. Raw MFT positive benchmarks remain visibly pending until an administrator run succeeds. Feature-support medals do not imply identical output formats or timing scopes.


Final benchmark identity correction: the standalone Scanner/Index examples were rebuilt separately, and their n=10 engine, index-memory and public-operation results were refreshed. The engine runner previously accepted a malformed comma-separated profile string as an absent directory; its profile allowlist and manifest oracle now reject invalid/missing datasets before publishing observations. UI/DLL/CLI identities and package bytes were already correct. The administrator prompt was canceled, so positive raw MFT runs remain pending and older native/MFT identities are excluded from current medians.

## Final expanded parity validation — 4 October 2026

The GUI acceptance sweep passes all 20 sequential scripts, including detailed metadata, gray PNG output, shell popup/cancel, owned hard-link consolidation, owned empty VHDX compaction and portable picker/cancel. Rust workspace/examples and strict Clippy pass; unchanged line-coverage gate reports 92.39%.

A real installed-exporter interoperability run exposed localized nine-column JSON without Owner: the new public read_json regression failed with Invalid WinDirStat CSV fields. Matching the normalization header width to the validated input schema made that regression and all 13 JSON tests pass. Rebuilt native adapters then passed all 90 interoperability probes (nine cases, n=10).

Timed n=10 reruns are separate from functional probes. Expanded adapters cover detailed JSON, TSV, own/foreign JSON and UTC CSV reopen, regex/allocation queries, cost estimates, four hash algorithms, two-root scan, hard-links, NTFS compression/sparsification and detached VHD zero-block compaction. No missing comparator or unequal operation scope is awarded a performance medal.
