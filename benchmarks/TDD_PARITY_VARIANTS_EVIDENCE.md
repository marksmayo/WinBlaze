# Parity variants: red/green evidence — 4 October 2026

- Rich CSV public roundtrip: missing exporter produced a compiler failure. Added streaming fourteen-column export and metadata-aware reopen; timestamps initially remained absent, then fixed metadata path mapping. Nine CSV-export and sixteen CSV-import tests pass, including lossless maximum physical IDs. Real GUI export verified current owner and physical identities.
- Flame/sunburst: native geometry regression failed to compile because hierarchy layout/hit testing was absent. Added weighted hierarchy bands/sectors, ancestry, zero-size refusal and radial hit tests; native suite passes. Integrated both layouts, shared size basis, hover/click targeting and configured PNG output. Real GUI export tests pass for both.
- In-place rename: real owned-fixture UI regression failed because the inline row action was missing. Added row editor, Save/Enter and Cancel/Escape with collision-safe native rename. First real file rename passes; collision, cancellation and nested-folder regressions also pass.
- Account controls: native regression failed on missing view policy. Added account/SID matching, exclusion and highlighting; tests pass. Audited installed WinDirStat 2.8.0 settings exposed regex exclusions and five color/rights rules. A second failing public policy test drove compiled regex rules, invalid-regex reporting, ACE-mask thresholds and ordered colors. Native policy tests pass; final GUI regex-account controls pass.

Benchmarks use n=10, with validated outputs and explicit geometry/control versus GUI rendering boundaries. Device-positive and administrator-positive validation require the user's hardware/prompt participation; source grades do not substitute for successful live checks.

Final workspace and example tests, strict Clippy and rustfmt pass. Rust line coverage is 92.03% against the unchanged 92% gate. Added rejection/absence tests cover missing metadata, provider errors, malformed identities/timestamps, overflow and empty detailed snapshots. GUI exports were decoded at 640×360 with grayscale channels and nonuniform pixels checked. A process-exit cleanup race was corrected without changing application behavior; both layout checks completed cleanly.

Metadata grading is split by actual comparator fields: WinDirStat Owner; WizTree Created/Last Accessed dates and MFT-record references. The composite richer WinBlaze field set is excluded from parity medals.

## Live administrator validation — 4 October 2026

Ten current-binary raw MFT phase runs and ten full native pipeline runs passed. Ten raw exports each for WinBlaze and WizTree passed output/signature checks. Ten WinBlaze reopens of the retained real dump reconstructed 1,688,653 files and matching exported root totals. These checks do not establish record-by-record completeness.

The initial Pixel 9 Pro batch passed seven scans before an empty-root failure on run eight; the next attempt returned an empty root on its first scan. Those incomplete batches are diagnostic evidence, not completed n=10 results.

## Completed live Pixel batch

Fresh n=10 Pixel 9 Pro metadata scans passed CSV count/root-total and current-executable identity checks. Median 15.68 seconds; 19,917–19,918 files; seven errors per scan; zero unknown sizes. The final two scans had one fewer file. No independent complete phone inventory ground truth exists. Earlier incomplete batches remain diagnostic evidence only.
