# Full-volume concurrency evidence

The official report remains 27 gold, 7 silver, 1 bronze across all 35 categories.
No scanner defaults or medal definitions changed in this experiment.

The existing directory-walk example accepts explicit worker budgets. The new
runner rotates/reverses two, three and four workers across ten rounds, under a
standard user token and the shared measurement mutex. All thirty scans complete
with matching file/directory event and summary counts; each reports 129 issues.
The live C: file count varies slightly during the experiment. All samples remain.
This example uses an unbounded event channel and consumes records without the
native index, persistence or export. Its measurements cannot replace medals or
prove end-to-end gains.

| Workers | Median seconds | P90 seconds | Timing variability | CPU lower-bound seconds | Peak private MiB |
|---|---:|---:|---:|---:|---:|
| 4 | 11.77944 | 12.50547 | 3.86978% | 44.99219 | 29.40039 |
| 3 | 13.33546 | 13.77383 | 3.85646% | 38.30469 | 29.24609 |
| 2 | 17.83062 | 18.74324 | 5.56342% | 34.68750 | 26.28320 |

Three workers offer effectively unchanged variability and 13.2% slower median.
Two workers are slower and more variable. Retain the current four-worker default;
neither lower budget is adopted for the variability target. Resource counters
are sampled at 25 ms; CPU values are lower bounds, not exact lifetime totals.

Evidence: `artifacts/all-gold-loop/iteration-11/directory-concurrency-4d56ef771fe84b0e85a0ab97d77eba1b/{observations,summary}.json`.
The exact diagnostic executable is archived beside the data.

The three-run preflight is kept separately in `directory-concurrency-6005d4445a444e9c822856b38364267b`.
Its first scan is much slower, and it is not combined with the planned cohort.
Preflight exits 1 after all scans complete because its in-memory runner calls
an undefined lock-release helper. The script now uses the actual mutex release
and disposal API; the complete thirty-scan runner exits 0. No preflight result
is used as a performance improvement claim.

## Next measured memory hypothesis

The compiled `winblaze-core` record-layout example reports FileRecord=136 bytes,
String=24, Box<str>=16 and Option<i64>=16 on this x64 build. Evidence is saved in
`iteration-11/file-record-layout.json`. Around 1.79 million file records make a
24-byte per-record reduction approximately 41 MiB before allocator effects.

A lossless timestamp group could hold three i64 values plus presence bits in
32 bytes instead of the existing 48. No i64 value may be reserved as a sentinel:
None, zero, i64::MIN and i64::MAX must all round-trip distinctly. An immutable
owned filename could save another eight bytes relative to String. Together
these suggest a 112-byte record target. Full paths, all timestamps, snapshot
encoding/reopen, search/filter/export and rename semantics must be preserved.
This is a hypothesis and a proposed red/green resource target, not an implemented
change or measured process-memory saving. Validate retained memory and fidelity
before any production migration, then run the required repository gates and all
35 WinBlaze medals with frozen competitor baselines.
