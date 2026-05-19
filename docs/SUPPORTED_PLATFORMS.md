# Supported Platforms

## Windows Versions

- Target platform: Windows Desktop 10.0.19041.0 or later.
- Maximum tested platform in the app manifest: 10.0.22621.0.
- Primary host architecture: x64.

## Filesystem Targets

### First-Class Target

- NTFS volumes are the primary target for the first implementation.
- NTFS is the path expected to receive the most direct metadata access and the best
  scan performance once MFT-based enumeration is implemented.

### Secondary Targets

- ReFS, FAT32, and exFAT are expected to use fallback directory traversal until a
  volume-specific optimization is added.
- The fallback traversal path has generated-dataset correctness and timing
  coverage through the directory-walk benchmark; physical removable-media
  calibration is release validation rather than a separate MVP scope decision.
- Local removable volumes are in scope when mounted normally. If the device is
  removed during a scan, Windows device-not-ready, no-media, disconnected-device,
  operation-aborted, request-aborted, timeout, and I/O-device errors are treated
  as transient scan issues rather than fatal application failures.

### Out of Scope for the Initial Release

- Network shares and UNC paths
- Cloud-synced virtual drives
- Exotic or vendor-specific filesystem layers

Network paths are out of scope for v1 performance guarantees. UNC/network paths
may be attempted through directory walking in development builds, but correctness
and performance are not release targets yet. User-facing v1 documentation should
present network scanning as unsupported until explicit benchmarks and
disconnect/error handling are added.

## Compatibility Notes

- Long paths should be handled where Windows API support is available.
- Reparse points, junctions, symlinks, and mount points require explicit policy
  decisions before the scanner treats them as first-class traversal targets.
- Filesystem support may expand as the scanner gains volume-specific access paths.
