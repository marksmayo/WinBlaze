# Product Definition

## MVP

- Fast scan of a chosen local Windows volume.
- Live progress and partial results while the scan is running.
- Folder totals, file totals, and a summary view.
- Persistent index load and save.
- Basic search over the persisted index.

## Success Metrics

- Time to first visible progress event.
- Total scan throughput in files per second and bytes per second.
- UI responsiveness during scan and search.
- Memory usage per indexed file.
- Index freshness after rescans.

## Non-Goals for v1

- Network share scanning.
- Cloud-synced drive semantics.
- Cross-machine sync.
- Custom file format migration tooling beyond simple upgrades.

## Benchmark Comparisons

- Compare cold scan throughput against WizTree.
- Compare warm startup and indexed query latency against Everything.
- Compare tree and filter responsiveness against WinDirStat.

## Performance Budget

- Keep the scanner hot path low-allocation.
- Keep scan progress emission off the UI thread.
- Avoid blocking the UI while search or indexing work is running.
- Prefer small, incremental updates over bulk UI refreshes.
