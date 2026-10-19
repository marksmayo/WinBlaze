# Extended medal events (70 candidates)

Goal: WinBlaze gold in every official medal event. Each candidate is lower-is-better,
uses absolute units and the same boundary for WinBlaze, WinDirStat and WizTree,
and needs n=10 accepted runs per product with validated output.

Triage rule (user decision, 2026-10-11): if WinDirStat or WizTree cannot perform
an event's operation, the event is removed. If only WinBlaze cannot, WinBlaze
implements it. Events that cannot be measured on this host without hardware or
privileges we lack are listed as blocked, with the reason.

Status values: planned · measuring · adopted · removed (competitor unsupported) ·
blocked (host/privilege) · winblaze-work (WinBlaze feature needed).

Precise counters come from `process-measure.ps1`: elapsed, kernel/user CPU, I/O
bytes, peak working set, peak commit and page faults read from the exited
process handle. Handle and thread peaks are polled every 10 ms (lower bounds,
meaningful only for long full-drive runs).

| # | Event | Family | Status | Notes |
|---|---|---|---|---|
| 1 | Usable-window startup | GUI | measuring | run-gui-comparison.ps1 |
| 2 | GUI full-drive scan to results | GUI | measuring | WinBlaze implemented command-line scan and a scan-state title |
| 3 | GUI memory with results open | GUI | measuring | run-gui-comparison.ps1 |
| 4 | Time to first result | GUI | measuring | run-gui-session-comparison.ps1: first result row while scanning (WinDirStat list items, WinBlaze UIA list item; WizTree draws rows only after its scan) |
| 5 | UI responsiveness during scan | GUI | measuring | run-gui-comparison.ps1 |
| 6 | Cancel latency | GUI | measuring | WinDirStat toolbar stop (command 32820), WizTree Cancel button, WinBlaze Cancel button (UIA) |
| 7 | Reload saved results | GUI/CLI | measuring | run-gui-session-comparison.ps1: WinDirStat /LoadFrom, WizTree opens its CSV, WinBlaze opens its CSV |
| 8 | Rescan after small change | GUI | measuring | run-gui-rescan-comparison.ps1: one added file, then WinBlaze incremental rescan / WinDirStat Refresh All / WizTree Scan |
| 9 | Disk bytes read, full drive | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 10 | Disk bytes written, full drive | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 11 | Energy per full-drive scan | resources | measuring | RAPL package power integrated over the full-drive run |
| 12 | Admin full-drive private memory | admin | adopted | Already an official event: 'Administrator · full-drive private memory' (elevated stage) |
| 13 | 1-million-file scale | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 14 | Deep and long paths | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 15 | 500k empty directories | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 16 | Non-NTFS volume | volume | planned | exFAT VHD; needs one-time admin setup |
| 17 | Second NTFS volume | volume | planned | NTFS VHD; needs one-time admin setup |
| 18 | Cold-cache full drive | admin | planned | Standby-list flush needs admin |
| 19 | Scan under background CPU load | robustness | measuring | Runner condition @cpuload |
| 20 | Search over full-drive results | GUI/CLI | planned | |
| 21 | Single huge sparse file | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 22 | Hard-link fixture | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 23 | Sparse and compressed files | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 24 | Alternate data streams | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 25 | Junction maze with loops | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 26 | Cloud placeholders (OneDrive) | dataset | measuring | WinBlaze implemented cloud-placeholder folder traversal; counts now match WinDirStat exactly |
| 27 | Unicode-heavy names | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 28 | Access-denied subtree | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 29 | Files changing during scan | robustness | measuring | Runner condition @changing (waits for the writer to start) |
| 30 | C:\Windows only | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 31 | User profile only | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 32 | node_modules-style tree | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 33 | Git repository | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 34 | 10 GB of 1 MB files | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 35 | Lognormal size mix | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 36 | Fragmented directory | dataset | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 37 | BitLocker-encrypted volume | volume | planned | Needs one-time admin setup |
| 38 | NTFS-compressed volume | volume | planned | Needs one-time admin setup |
| 39 | Network share (loopback SMB) | volume | planned | Needs a share (admin) |
| 40 | Mapped drive letter | volume | planned | Needs a share (admin) |
| 41 | Peak handle count, full drive | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 42 | Peak thread count, full drive | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 43 | Page faults, full drive | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 44 | Peak commit, full drive | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 45 | Kernel CPU, full drive | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending; 4 walk workers restored for lower kernel CPU |
| 46 | User CPU, full drive | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 47 | Context switches, full drive | resources | measuring | NtQuerySystemInformation per-thread samples (lower bound) |
| 48 | Memory per million files | resources | measuring | Measured n=10 for all three (2026-10-11); WinBlaze rerun pending after scanner changes |
| 49 | GUI GDI and USER objects after scan | GUI | measuring | GetGuiResources at results-ready (run-gui-comparison.ps1) |
| 50 | On-disk footprint | static | removed | No longer a medal event: on-disk size is not performance. The measurement script was retired. |
| 51 | Expand a 10k-child folder | GUI | planned | |
| 52 | Sort largest folder by size | GUI | planned | |
| 53 | Treemap redraw after resize | GUI | measuring | run-gui-session-comparison.ps1 + gui-settle.ps1: pixels last change before 400 ms stable |
| 54 | Treemap hover highlight | GUI | planned | |
| 55 | Navigate to deepest folder | GUI | planned | |
| 56 | Open a new scan target | GUI | planned | |
| 57 | Extension list populate | GUI/CLI | planned | WizTree /exportfiletypes |
| 58 | Scroll full file list | GUI | planned | |
| 59 | Second launch with warm OS cache | GUI | removed | Duplicate of #1: every startup run after the first is already a warm-cache second launch |
| 60 | Exit with full-drive results loaded | GUI | measuring | run-gui-comparison.ps1 |
| 61 | CSV export of loaded results | CLI | blocked | WinDirStat re-exports a loaded scan only through GUI menus (/LoadFrom with /SaveTo writes nothing); WizTree supports it on the command line |
| 62 | CSV export throughput | CLI | blocked | Depends on #61; WinDirStat and WizTree export only as part of a scan or load |
| 63 | Import a competitor's CSV | GUI/CLI | removed (competitor unsupported) | WinDirStat /LoadFrom opens only its own CSV; WizTree never finished loading another product's CSV |
| 64 | Filtered export | CLI | removed (competitor unsupported) | WinDirStat's command line has only /SaveTo, /LoadFrom and /SaveDupesTo; no filtered export |
| 65 | Top-N largest files | CLI | removed (competitor unsupported) | Neither WinDirStat nor WizTree exports a top-N list from the command line; the GUI views fill as part of the scan (covered by scan-to-results) |
| 66 | Scan while disk is busy | robustness | measuring | Runner condition @diskbusy |
| 67 | Scan under a 1 GB memory limit | robustness | measuring | Runner condition @memlimit (Job object, 1 GiB commit) |
| 68 | First scan after boot | robustness | removed | Not repeatable ten times without unattended reboots |
| 69 | Memory growth over 10 GUI rescans | GUI | measuring | run-gui-rescan-comparison.ps1: working set after 10 rescans minus after the first scan |
| 70 | Crash-free rate over 100 scans | robustness | measuring | run-reliability-comparison.ps1: 10x10 scans over hostile datasets; mean failure share |
