# WinBlaze

<p align="center">
  <img src="docs/winblaze.png" alt="WinBlaze" width="256" />
</p>

A blazingly fast disk usage analyzer for Windows that leverages NTFS internals for real-time insight into where your space went.

## Overview

WinBlaze combines a Rust scanning engine (raw NTFS MFT access with a parallel directory-walk fallback) with a C++/WinRT WinUI 3 frontend: a live expandable folder tree, a GPU-accelerated squarified treemap colored by extension, and instant search over a persistent index — wrapped in the **High Velocity** design system.

### Key Features

- **Live folder tree**: directories appear in the tree *as the scan discovers them*, with full size rollups landing the moment the scan completes
- **Real NTFS MFT scanning**: raw-volume Master File Table reader (boot-sector geometry, runlist extents, USA fixups) with correct handling of deleted records, named streams, extension records, and hardlinks
- **Hierarchical treemap**: squarified, extension-colored Direct2D tiles with progressive deepening that never stalls the UI thread
- **Persistent indexing**: compact binary snapshots reload instantly; incremental rescans diff against the previous state
- **Memory disciplined**: file paths are derived (not stored) per record — a 2.2M-file index fits in ~1 GB working set and a 323 MB snapshot
- **Responsive by measurement**: the UI ships its own frame/flush latency counters, and scan-time work is budgeted per frame
- **Self-updating**: checks GitHub for a newer release on launch (and on demand), then downloads, **SHA-256-verifies**, and installs the update in-app before relaunching — no manual reinstall

## Performance

Compared against WizTree and WinDirStat on the live `C:\` system volume — **~2.3M files / ~546k directories / 464 GB** — Windows 11, NVMe, warm cache, elevated; timed 2026-07-07. GUI tools were timed launch → scan-complete-and-populated via a CPU-idle probe; WinBlaze uses its own reported scan duration plus UI-idle detection.

<p align="center">
  <img src="docs/perf-comparison.svg" alt="Scan to interactive view on a warm C:\ drive (lower is better): WinBlaze engine ~2.4 s, WinBlaze in-app ~4.0 s, WinDirStat 2.6 ~10.5 s, WizTree 4.31 ~14.0 s" width="820" />
</p>

| Tool | Backend | Scan → interactive | Notes |
|---|---|---|---|
| **WinBlaze** (engine) | NTFS MFT | **~2.4 s** | scan → summary (`mft_scan_repro`); post-optimization |
| **WinBlaze** (in-app) | NTFS MFT | **~4.0 s** | Release UI idle→idle, incl. tree + treemap |
| WinDirStat 2.6.0 | directory walk (multithreaded) | ~10.5 s | modern fork; ~60 s CPU across cores |
| WizTree 4.31 | NTFS MFT | raw read ~2–3 s; to interactive ~14–55 s | see caveat |

**WizTree caveat:** its raw MFT read is as fast as WinBlaze's, but the GUI materializes the full ~2.9M-file list and treemap up front, so *time to an interactive view* is larger and highly variable (14–55 s observed; the chart shows the best run). Its all-file CLI export took 28–35 s, but that is dominated by writing a 437 MB CSV, not scanning. WinBlaze reaches an interactive view sooner because it pages and caps the UI (8,192-entry catalog, paged tree, deferred snapshot) rather than materializing every file. Single-machine, warm-cache figures — see [benchmarks/perf-overhaul-baselines.md](benchmarks/perf-overhaul-baselines.md) for methodology and raw runs.

The **engine** figure reflects the optimized scanner (sparse MFT read that skips free-record runs, a pipelined parser/emit, and a direct-read reader — ~15% faster than the original 2026-07-07 measurement); the competitor bars are that same dated snapshot and were not re-timed. The scan is now **read-bound at the volume's ~2 GB/s raw-read ceiling**, so sub-2 s is not reachable in software on this hardware — see [docs/ENGINE_SCAN_PERFORMANCE.md](docs/ENGINE_SCAN_PERFORMANCE.md) for the profiling and why.

Generated-dataset budgets (tiny/fanout/fanout-large/scale) are enforced in CI and locally via `benchmarks\performance-budgets*.json`; competitor methodology notes live in `docs\BENCHMARK_METHODOLOGY.md` and `benchmarks\competitor-report.md`.

## Project Stats

| | |
|---|---|
| Tracked files | 147 |
| Rust | 10,300 lines across 32 files (5 crates) |
| C++/WinRT | 7,072 lines across 17 files |
| PowerShell automation | 2,768 lines across 28 scripts |
| Documentation | 2,544 lines across 38 markdown files |
| Rust unit/integration tests | 89 (`cargo test`), plus scripted UI smoke, negative smoke, and budgeted benchmarks |

## Installation

### Requirements

- Windows 10 version 1903 or later (Windows 11 recommended)
- x64 architecture
- Administrator privileges for the NTFS MFT fast path (the directory-walk fallback runs unelevated)
- Visual C++ Runtime 2022

### Quick Start

1. Download the latest release from [Releases](https://github.com/marksmayo/WinBlaze/releases)
2. Run the MSI installer or extract the portable ZIP
3. Launch WinBlaze — run as Administrator to enable raw MFT scanning

## Usage

1. The root path defaults to `C:\` — adjust it, then click **Start scan**
2. Watch folders stream into the tree live; sizes fill in at completion
3. Double-click folders to expand them; children load on demand
4. Click treemap tiles to select; colors match the extension legend
5. Use **Incremental rescan** to refresh only what changed

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+F` | Focus search box |
| `Ctrl+4` | Reveal search panel |
| `Ctrl+5` | Reveal diagnostics panel |
| `Escape` | Cancel current scan |

## Technical Architecture

- **src/WinBlaze.Core** — domain models, rollup aggregation, change/lineage detection
- **src/WinBlaze.Scanner** — raw-MFT reader (volume handle, runlist extents, fixups) and parallel directory-walk fallback with reparse-cycle protection
- **src/WinBlaze.Index** — binary snapshot persistence, incremental diffing, search, and the arena tree read model that serves the UI's paged children queries
- **src/WinBlaze.Native** — C ABI bridge: batched scan events, cached index model, paged `wb_tree_*` APIs
- **src/WinBlaze.UI** — C++/WinRT WinUI 3 shell: live tree arena, Direct2D squarified treemap, High Velocity design system

### Performance Design

- Directory events cross the FFI in 4,096-entry batches; UTF-8→UTF-16 conversion happens off the event pipeline
- The read model is published before the UI hears "Completed", so post-scan reloads hit a hot cache
- Treemap paints budget node materialization per frame and refine progressively
- Debug UI builds automatically ship the newest (typically release-profile) native DLL

## Building from Source

### Prerequisites

- Visual Studio 2022 with C++ workload
- Rust toolchain (stable)
- Windows SDK 10.0.22621.0 or later

### Build Steps

```powershell
git clone https://github.com/marksmayo/WinBlaze.git
cd WinBlaze

# Optimized scanner engine (Debug UI builds pick this up automatically)
cargo build --release -p winblaze-native

# UI (Debug)
& "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe" `
  src\WinBlaze.UI\WinBlaze.UI.vcxproj /p:Configuration=Debug /p:Platform=x64

# Full local gate: tests, build, UI smoke, packaging
powershell -ExecutionPolicy Bypass -File scripts\check-local.ps1 -Configuration Debug
```

## Benchmarking

```powershell
# Generate test datasets
powershell -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size tiny

# Run performance benchmarks
powershell -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size tiny

# Record competitor inventory / manual timings
powershell -ExecutionPolicy Bypass -File benchmarks\record-competitor-baselines.ps1
```

## Privacy

WinBlaze has **no telemetry**. It writes only local files: structured logs at `%LOCALAPPDATA%\WinBlaze\logs` and the scan index at `%LOCALAPPDATA%\WinBlaze\index`. See `docs\PRODUCTION_SECURITY_REVIEW.md`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Run `scripts\check-local.ps1` before submitting changes; performance budgets are enforced.

## License

MIT — see [LICENSE](LICENSE).

## Roadmap

### Done
- ✅ Raw NTFS MFT scanning (elevated) with directory-walk fallback
- ✅ Live expandable folder tree with per-folder rollups
- ✅ Squarified extension-colored GPU treemap
- ✅ Persistent index, instant search, incremental rescan
- ✅ High Velocity design system

### Next
- 🔄 End-to-end record batching through the scan pipeline (WizTree-class times for both backends)
- 🔄 Donut used-space gauge, Explorer file table, and Cleanup center from the design mockups
- 🔄 Duplicate file detection
- 🔄 Export to various formats
