# Iteration 13 evidence and compact file names

The completed original-35 rerun has **27 gold, 6 silver and 2 bronze**.
All categories have fresh WinBlaze n=10 evidence and all 490 feature
observations validate. Definition, freshness, sample count, report, focus
and publication checks pass. All 70 retained competitor entries, including
their source evidence, are exactly unchanged from iteration 12. The retained
administrator worker skips competitor launches and requires no new UAC.

The scoped allocation hints remove the observed first full-volume memory
spike in this cohort: all ten CLI runs peak around 460–463 MiB. The first
elapsed observation worsens to 43,549 ms and remains included. This does
not prove a speed improvement. Latest full-volume IQR/median variability
is 6.6388965742972745%, still silver against retained WinDirStat 5.2304054807652545%.
Latest full-drive private memory is 473.5 MiB versus 444.83984375 MiB.
Quick-folder variability worsens to 22.16361464968154%, now bronze;
startup working set is 34.5546875 MiB, also bronze. No historical WinBlaze
best replaces these latest values. The all-gold gate remains RED.

Shipping identities for this completed cohort are CLI C5F0B100, native DLL
E8B72AAB and UI AA3F83A5. The separate tiny UI gate remains RED at 2,424 ms
against its unchanged 2,000-ms budget. Final evidence is archived in
iteration-13/final-validation.json and report-data.json under
runs/2026-10-07T23-22-50-764Z.

The next actual public resource RED lowers the published file catalog
budget by eight bytes per file. All 16,384 names, paths and timestamps
(i64::MIN, zero, i64::MAX) remain checked. Existing code retains 2,637,824
bytes against a 2,510,848-byte budget and fails.

FileRecord names now use Box<str>, matching the immutable full-path
representation. Renames replace whole records; directory names and public
search result strings remain String. Scanner/import/snapshot constructors
convert owned names without changing their text. Paths, snapshot encoding,
exports, metadata and the C ABI retain their existing semantics. Unicode
scanner fidelity remains covered; the published catalog allocator test
checks retained storage rather than requiring String's capacity API.

The focused name resource test is GREEN and all workspace targets compile.
Workspace tests, examples, formatting and strict lint validation pass
(session 81426), followed by 92.46% coverage (session 20741). Actual x64
layout is 120 bytes, down from 128. No process-memory gain is claimed.

A second public resource RED tests a further sixteen-byte reduction with
all 64 combinations of None, Some(i64::MIN), Some(0) and Some(i64::MAX).
The 120-byte representation retains 2,506,752 bytes and fails the unchanged
fixture's new 2,248,704-byte budget. FileTimestamps now holds three aligned
i64 values with separate presence bits. Missing values use canonical zero
storage while presence distinguishes None from Some(0); no sentinel consumes
any valid timestamp. The fields are private and accessors return Option<i64>.
FileRecord construction uses this group, while snapshot writing preserves
the original three optional timestamp encodings and the C ABI still exports
the same values and presence flags. No packed or unsafe accesses are added.

The resource test is GREEN; actual x64 FileRecord layout is 104 bytes.
A real persisted snapshot round trip preserves all 64 combinations, names
and paths. Workspace tests, examples, formatting, strict all-target/all-feature
clippy and 92.47% line coverage pass (session 26142). The combined reduction
is 24 bytes per file from the previously benchmarked 128-byte layout.

Previous C5F0B100/E8B72AAB/AA3F83A5 shipping binaries are archived under
iteration-13/pre-compact-record-shipping. Release native and release examples
pass. Local gate 17755 finishes with exit 1: native contracts, ten path/window
cases, smoke, parity, imports, filter and negative UI checks pass, but the
unchanged tiny UI timing budget fails at 2,425 ms against 2,000 ms. The full
local gate remains RED.

New CLI identity is D178578FE2024247A534B519107B481E3B170D8C83D988E7E276037C794C8215.
New DLL is 9FF4B2BAC5DAFD0160FF06874D21528044C7EDC2B9E1F2AC25996808834A9F62,
including the staged UI DLL. UI executable AA3F83A5 is unchanged.
A fresh original-35 WinBlaze rerun remains required to establish actual
process-memory/timing effects. The goal remains active, with all gold unproven.
