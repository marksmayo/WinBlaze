# WinBlaze

A blazingly fast disk usage analyzer for Windows that leverages NTFS internals for unmatched performance.

## Overview

WinBlaze is a high-performance disk usage analyzer designed specifically for Windows systems. It combines the speed of low-level NTFS MFT (Master File Table) access with a modern, GPU-accelerated visualization interface to deliver instant insights into your disk usage.

### Key Features

- **Lightning-Fast Scanning**: Direct NTFS MFT enumeration for speeds that rival and often beat WizTree
- **Real-Time Visualization**: Live treemap updates during scanning with GPU-accelerated Direct2D rendering
- **Persistent Indexing**: Save and reload scan results instantly, with incremental rescan capabilities
- **Instant Search**: Find files and folders across millions of entries in milliseconds
- **Memory Efficient**: Optimized to handle tens of millions of files while maintaining low memory footprint
- **Modern UI**: Native WinUI 3 interface with smooth animations and responsive controls

## Performance

WinBlaze consistently matches or outperforms leading disk analyzers:

| Dataset Size | Files | WinBlaze | WizTree | Speed Advantage |
|-------------|-------|----------|---------|-----------------|
| Small | 1,536 | 571ms | 754ms | **1.32x faster** |
| Large | 16,384 | 456ms | 820ms | **1.80x faster** |
| Dense | 8,192 | 505ms | 545ms | **1.08x faster** |

*Benchmarks performed on Windows 11 with NVMe storage. Results show median elapsed time from 3 runs.*

## Installation

### Requirements

- Windows 10 version 1903 or later (Windows 11 recommended)
- x64 architecture
- Administrator privileges (for NTFS MFT access)
- Visual C++ Runtime 2022

### Quick Start

1. Download the latest release from [Releases](https://github.com/marksmayo/WinBlaze/releases)
2. Run the MSI installer or extract the portable ZIP
3. Launch WinBlaze with administrator privileges for optimal performance

## Usage

### Basic Scanning

1. Select a drive or folder to scan from the dropdown
2. Click "Start Scan" to begin analysis
3. View results in real-time as the scan progresses
4. Navigate the treemap to explore space usage visually

### Search and Filter

- **Instant Search**: Type in the search box to find files/folders instantly
- **Size Filters**: Filter by minimum file size to focus on space hogs
- **Type Filters**: Filter by file extension or type

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+S` | Start/Stop scan |
| `Ctrl+F` | Focus search box |
| `Ctrl+R` | Incremental rescan |
| `Escape` | Cancel current operation |

## Technical Architecture

WinBlaze achieves its performance through a carefully designed multi-layer architecture:

### Core Technologies

- **Scanner Backend**: Pure Rust for memory safety and zero-allocation hot paths
- **NTFS Integration**: Direct MFT enumeration bypassing traditional filesystem APIs
- **UI Frontend**: C++/WinRT with WinUI 3 for native Windows integration
- **GPU Rendering**: Direct2D/DirectWrite for hardware-accelerated visualization
- **Data Persistence**: Compact binary snapshots for instant save/load

### Performance Optimizations

- Streaming scan architecture with batched UI updates
- Lock-free data structures for concurrent access
- Virtualized UI controls (256-row pages, 8,192-entry catalog cap)
- Coalesced GPU surface redraws
- Frame-time and input-latency instrumentation

## Building from Source

### Prerequisites

- Visual Studio 2022 with C++ workload
- Rust toolchain (stable)
- Windows SDK 10.0.22621.0 or later

### Build Steps

```powershell
# Clone the repository
git clone https://github.com/marksmayo/WinBlaze.git
cd WinBlaze

# Build Rust components
cargo build --release

# Build UI (Debug)
& "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe" `
  src\WinBlaze.UI\WinBlaze.UI.vcxproj /p:Configuration=Debug /p:Platform=x64

# Run tests and smoke checks
powershell -ExecutionPolicy Bypass -File scripts\check-local.ps1 -Configuration Debug
```

## Benchmarking

WinBlaze includes comprehensive benchmarking tools to measure and validate performance:

```powershell
# Generate test datasets
powershell -ExecutionPolicy Bypass -File benchmarks\make-datasets.ps1 -Size tiny

# Run performance benchmarks
powershell -ExecutionPolicy Bypass -File benchmarks\run-ui-benchmark.ps1 -Size tiny

# Compare with competitors
powershell -ExecutionPolicy Bypass -File benchmarks\record-competitor-baselines.ps1
```

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](docs/CONTRIBUTING.md) for guidelines.

### Development Setup

1. Fork and clone the repository
2. Install prerequisites (VS2022, Rust, Windows SDK)
3. Run `scripts\check-local.ps1` to verify setup
4. Make changes and ensure tests pass
5. Submit a pull request

## Comparison with Alternatives

| Feature | WinBlaze | WizTree | WinDirStat | Everything |
|---------|----------|---------|------------|------------|
| NTFS MFT Scanning | ✅ | ✅ | ❌ | ✅ |
| Real-time Updates | ✅ | ❌ | ✅ | N/A |
| GPU Acceleration | ✅ | ❌ | ❌ | ❌ |
| Persistent Index | ✅ | ❌ | ❌ | ✅ |
| Incremental Rescan | ✅ | ❌ | ❌ | ✅ |
| Memory Efficiency | ✅ | ✅ | ❌ | ✅ |
| Open Source | ✅ | ❌ | ✅ | ❌ |

## License

WinBlaze is open source software released under the MIT License. See [LICENSE](LICENSE) for details.

## Support

- **Issues**: [GitHub Issues](https://github.com/marksmayo/WinBlaze/issues)
- **Discussions**: [GitHub Discussions](https://github.com/marksmayo/WinBlaze/discussions)
- **Documentation**: [Project Docs](docs/)

## Roadmap

### Current (v1.0)
- ✅ NTFS MFT scanning
- ✅ Real-time treemap visualization
- ✅ Persistent indexing
- ✅ Search and filtering
- ✅ Incremental rescanning

### Planned
- 🔄 Network drive support
- 🔄 Cloud storage integration
- 🔄 Duplicate file detection
- 🔄 Scheduled scanning
- 🔄 Export to various formats

## Acknowledgments

WinBlaze stands on the shoulders of giants. Special thanks to the developers of WizTree for pioneering NTFS MFT-based scanning, and to the Rust and Windows development communities for their excellent tools and libraries.

---

**Ready to reclaim your disk space at blazing speed?** [Download WinBlaze](https://github.com/marksmayo/WinBlaze/releases) today!