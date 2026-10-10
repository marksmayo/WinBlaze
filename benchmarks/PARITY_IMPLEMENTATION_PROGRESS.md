# Feature Parity Implementation Progress

The requested missing-feature work was developed in public-interface red/green slices. This record distinguishes functional acceptance from timed benchmark evidence.

## Implemented and verified

- Complete-index paged search, regex, logical/allocated size bounds, UTC dates and extension filters; the GUI finds records beyond its former 8,192-row preview limit.
- CSV reopen with reordered English/localized positional WinDirStat columns, optional Owner retention and explicit WizTree Local/UTC input. JSON and quoted TSV import/export; detailed JSON preserves attributes, dates, Owner and lossless physical identities.
- Scan-time subtree exclusions and filename inclusion rules, validation before publication, and disjoint multiple-root catalogs preserving original paths.
- Interactive byte-verified duplicate investigation; SHA-256, MD5, SHA-1 and SHA-512; confirmed two-path hard-link consolidation; conservative no-recall content-open safeguards.
- File/folder results-row rename, batch move/recycle, native Explorer context menus, readable account/permission rows, timestamped bounded change history and configurable storage-tier cost estimates.
- Logical/allocated treemaps, balanced rectangles/horizontal strips, live-volume free/unaccounted tiles, configurable PNG dimensions/size basis/free-space and grayscale.
- Confirmed NTFS compression, verified-zero sparsification and detached dynamic VHD/VHDX zero-block compaction on owned fixtures.
- Read-only WPD portable-device inventory, selection and cancellable metadata scan adapter; picker/cancel verified. Positive Pixel 9 Pro metadata scanning now passes at n=10; seven device errors per scan and no independent completeness ground truth remain explicit.
- Offline raw MFT parser/import with fixup, geometry and atomic-publication regressions; live administrator dump/reopen now passes at n=10.

## Completed final gates

Rust workspace and example tests pass; strict Clippy and rustfmt pass. Coverage is 92.03% against the unchanged 92% gate. Release Rust/native/UI binaries were rebuilt. All 20 relevant GUI acceptance scripts pass. Fresh interoperability validation passes 90 probes: nine cases, n=10 each, with installed executable hashes recorded.

The installed WinDirStat localized JSON without its optional Owner column exposed one additional regression: failing public import test, minimal header-width fix, then all 13 JSON tests pass.

## Timed validation completed

All standard-user comparator workflows and expanded feature adapters completed at n=10. The current dataset contains 690 WinBlaze observations with matching tested executable/DLL identities and 490 validated cross-product feature observations. New workloads have no fabricated historical baseline. Adjacent control/optimized startup experiments also use n=10 and preserve published output/history files.

## Remaining explicit differences

Pixel 9 Pro metadata scans and administrator-positive MFT phases/native scanning, raw exports and real-dump reopen now pass at n=10. Phone scans retain seven device errors and no independent inventory ground truth. Flame/sunburst views and PNG export, direct in-place file/folder rename, detailed CSV export/reopen and account/SID permission exclusion/highlighting are now implemented and pass targeted native/Rust and actual GUI checks. Group-wide duplicate cleanup and filesystem-aware VHD compaction remain outside the implemented criteria. VHD compaction never attaches disks; basic CSV retains its compact seven-column schema.

Support grades apply to the criterion printed in each row. Equal grades and WinBlaze advantages above both competitors are omitted and earn no feature medals; remaining differences stay visible in cell details. Do not rerun the non-idempotent artifacts/parity-audit/update-matrix.py.
