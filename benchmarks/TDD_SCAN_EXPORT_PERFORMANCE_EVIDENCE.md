# Red/green evidence: startup + scan + CSV performance

The target is the shipped `winblaze-cli` whole-process workflow, including scan, indexing, deferred durable catalog persistence and complete CSV export. Every timing batch uses n=10. The preserved baseline location is recorded in `artifacts/scan-export-baseline-path.txt`.

## Vertical slices

1. **Batched directory metadata.** RED: public `read_directory` allocation/name test failed to compile because the provider was absent. GREEN: implemented supported `GetFileInformationByHandleEx` directory records, then connected the scanner with existing FindFirst/std fallbacks. Independently queried allocation matches empty, resident, larger, sparse and compressed files. Existing cancellation, fanout, parallel/sequential and reparse tests pass.
2. **One name conversion.** RED: 3,000 names allocated 435,009 bytes. GREEN: borrow aligned UTF-16 from the owned directory buffer; 261,003 bytes. The final 300,000-byte budget allows Windows OsString conversion capacity and still fails the previous implementation. No lossy filename conversion is used for filesystem operations.
3. **Compact volume identity ownership.** RED: 100,000 identities allocated 8,650,876 bytes cumulatively against a 5 MB budget. GREEN: store the volume once and full file identities in its set. Cross-volume and high 128-bit identities stay distinct.
4. **Compact common identities.** RED: 100,000 identities plus a wide identity allocated 4,456,880 bytes against a 2.6 MB budget. GREEN: narrow IDs use u64 storage; wide IDs retain all 128 bits in a separate set. No identity truncation or probabilistic deduplication.
5. **Concurrent claims.** RED: concurrent public tracker was absent. GREEN: 64 independent shards; 20 threads repeatedly claiming the same 10,000 files allocate exactly 40,960,000 physical bytes in total. Actual scan tests also verify cross-directory hard links under both sources and worker counts.
6. **Zero-allocation claims.** The public storage-claim slice specifies that a zero-byte claim does not consume an identity. A later nonzero claim is retained. The implementation avoids storing/locking those zero-allocation entries; logical names/counts are unchanged.
7. **Buffered atomic output.** RED: the public atomic writer issued 4,096 OS disk writes for a 32 MiB output, failing a <128-write budget. GREEN: 1 MiB buffering produces 32 writes, preserves every byte, flushes and synchronizes before atomic replacement. Source protection and failure preservation remain in the existing native tests.
8. **Offline report.** RED: report lacked the new scan/export review. GREEN: rendered review shows all 100 candidates, seven implemented changes, measured/estimated benefits and explicit qualification limits while preserving the feature table at the bottom.

## Validation

Strict workspace Clippy and rustfmt pass. The existing workspace coverage gate passed at 92.62% (minimum 92%, unchanged exclusions). New public-interface tests are in Scanner `tests/directory_metadata.rs`, `allocation_tracker.rs`, `performance.rs`, extended `allocation.rs`, and Native `tests/output_performance.rs`.

A 10-run exploratory batched engine probe observed approximately 16.8 s median against the earlier 60.8 s median. Initial scale runs showed cache warming; exploratory full timing is not used as a per-edit causal estimate. The rebuilt final n=10 workflow/feature/UI refresh supplies the report results. Live-volume file coverage and cache state differ across dated observations.

## Limits

The 100-candidate audit is a source-review and profiling backlog. Only seven changes above are implemented; 93 candidates have no demonstrated isolated benefit and are not classified zero/low risk. Component estimates overlap and cannot be summed. Raw-volume administrator measurements remain separate from the standard-user workflow.

Directory batch API reference: https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_extd_dir_info

Final validation: all 450 WinBlaze observations match current binaries; 20 fresh competitor full-drive observations (10 each) complete. GUI parity regression passed under Windows PowerShell: duplicate/ACL exports, automatic watcher publication, move cancellation/success and confirmed recycling. Final medians: WinBlaze 17.61 s, WinDirStat 26.90 s, WizTree 38.10 s.
