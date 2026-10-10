# Feature parity audit — 4 October 2026

The HTML comparison contains 53 scoped capabilities with per-product evidence. WinBlaze now meets all 53 stated criteria fully, including a positive ten-run Pixel 9 Pro metadata scan. These are criterion-level grades: they do not imply identical interfaces or complete parity on every variant described below. Equal support grades are excluded from medal events.

## Version and evidence boundaries

Installed WinDirStat 2.8.0 was audited against official release/v2.8.0 source commit 579dd76d6870b4efbb9497644698155fdab84284. WizTree 4.33 was checked against its installed interface, official guide and versioned history. The benchmark runners retain executable versions and SHA-256 identities. Source-reviewed competitor behavior is distinguished from executed owned-fixture checks; no closed-source absence claim extends beyond its exposed audited interface.

WinBlaze's new functional validation includes Rust/public native regressions, real NTFS fixtures, 20 actual GUI acceptance scripts, and fresh installed-exporter interoperability tests. Rust line coverage passes at 92.03% with unchanged exclusions and gate.

## Fresh interoperability checks

Run: pwsh -NoProfile -File benchmarks/verify-parity-interop.ps1 -Runs 10. All nine cases pass ten times, totaling 90 probes; hashes and results are in [the validation JSON](current-parity-interop-validation.json).

| Probe | Verified result |
|---|---|
| WizTree export with a .json filename | CSV content, not JSON |
| WizTree default local timestamp import | Known UTC timestamp preserved |
| Explicit WizTree UTC import | Known UTC timestamp preserved |
| WinDirStat optional Owner CSV | Owner, size and UTC retained through detailed JSON |
| Reordered English Owner CSV | Caption mapping preserves Owner, size and UTC |
| Localized positional CSV | Translated fixed-order fixture accepted with Owner retained |
| Installed WinDirStat JSON | Size and UTC preserved |
| Localized positional JSON | Translated installed fixed-order schema accepted without optional Owner |
| WinBlaze quoted TSV roundtrip | Size and UTC preserved |

Translated/reordered fixtures are explicit format probes derived from installed exports; they are not an assertion that the installed application's language setting was changed.

## Remaining differences and verification limits

- A fresh connected Pixel 9 Pro batch passed ten metadata scans (19,917–19,918 files, seven errors per scan, zero unknown sizes). Earlier empty-root attempts are diagnostic evidence only. No independent complete device inventory ground truth exists.
- Administrator-positive MFT phase/native scan tests, both raw exporters and WinBlaze real-dump reopen now pass at n=10. Signature and internal-total checks do not establish record-by-record completeness.
- WinBlaze now supports balanced rectangles, horizontal strips, flame graph and sunburst, including PNG export. Geometry tests and actual GUI exports pass.
- Duplicate actions operate on selected files or a confirmed pair; group-wide duplicate cleanup is absent.
- WinBlaze supports detached VHD zero-block compaction; filesystem-aware compaction that attaches a disk is absent.
- WinBlaze now provides direct in-place file/folder editing alongside its Rename dialog. Owned GUI fixtures verify cancellation, collision refusal and content preservation.
- Cloud duplicate guards refuse offline/recall/reparse content, including conservative cases. A real cloud-provider hydration check remains unavailable.
- Multiple roots are sequential and session-only; history is bounded to the running session. Readable ACL rows do not calculate effective access.
- Detailed CSV adds attributes, created/accessed timestamps, owner and lossless physical identities; export/reopen regressions and actual GUI metadata export pass. Basic CSV remains seven columns. Missing metadata remains absent. Permission exclusions and five ordered regex/color/rights rules are dialog-local; they describe ACEs, not effective access.

## Performance evidence

Fresh n=10 standard-user batches completed: 690 current WinBlaze observations and 490 validated feature observations. Every timed adapter states its operation boundary, validates output, and records binary identity. A cached export, scan-plus-export and owned-fixture maintenance test are distinct workloads. Missing comparator measurements and unequal scopes do not earn performance medals. New adapters have no invented historical baseline.
