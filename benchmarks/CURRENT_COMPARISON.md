# Disk analyzer comparison — 16 October 2026

The [offline HTML report](CURRENT_COMPARISON.html) contains the refreshed results, medals, individual observations and executable identities.

Ten validated observations per metric and product. This batch refreshes WinBlaze, WinDirStat, WizTree. Standard-user batch `surface-book-20261012T115706Z` is complete. Administrator collection: complete. 14/35 original medal categories have complete accepted evidence.

| Product | Gold | Silver | Bronze |
|---|---:|---:|---:|
| WinBlaze | 70 | 6 | 3 |
| Everything | 4 | 13 | 10 |
| SpaceSniffer | 4 | 5 | 4 |
| WinDirStat | 2 | 44 | 19 |
| Directory Report | 2 | 10 | 20 |
| TreeSize Free | 2 | 0 | 2 |
| WizTree | 1 | 2 | 22 |

## Full-drive standard-user workflow

Startup + scan/indexing + complete CSV export, excluding GUI rendering.

| Product | Median time | Sampled CPU | Sampled peak working set | Sampled peak private memory |
|---|---:|---:|---:|---:|
| WinBlaze | 17.20 s | 48.13 CPU-s | 292.44 MiB | 304.37 MiB |
| WinDirStat | 33.28 s | 108.82 CPU-s | 436.33 MiB | 412.49 MiB |
| WizTree | 45.04 s | 126.76 CPU-s | 897.85 MiB | 877.70 MiB |

## Startup

Unchanged hidden-window first-idle protocol; this does not establish visible, usable-window readiness.

| Product | First idle | CPU at first idle | Working set at first idle |
|---|---:|---:|---:|
| WinBlaze | 54.47 ms | 39.06 ms | 13.99 MiB |
| WinDirStat | 64.33 ms | 46.88 ms | 15.82 MiB |
| WizTree | 224.26 ms | 218.75 ms | 31.27 MiB |

Every competitor retains the lowest validated n=10 statistic per medal metric and executable version/hash. Subsequent iterations reuse complete competitor baselines until their version or executable hash changes; missing baselines require collection. WinBlaze is rerun and uses the latest validated batch per metric; pending refreshes retain their dated previous result. Each metric may select a different dated batch. Raw latest observations remain separately dated.

## All 81 medal results

| Measurement | WinBlaze | WinDirStat | WizTree |
|---|---:|---:|---:|
| GUI lognormal · refresh after a small change | 196.31 ms | 673.16 ms | 1492.78 ms |
| GUI full drive · responsiveness p95 | 0.52 ms | 35.05 ms | 0.30 ms |
| GUI session · reload saved results | 658.74 ms | 1034.90 ms | 2352.13 ms |
| Install footprint · 64-bit GUI files | 4.60 MiB (refresh pending) | 2.77 MiB | 10.51 MiB |
| Full C: volume · disk bytes written | 515.77 MiB (refresh pending) | 686.13 MiB | 558.79 MiB |
| Full C: volume · context switches | 346108.50 switches (refresh pending) | 832973.00 switches | 1774781.50 switches |
| GUI session · redraw after resize | 75.93 ms | 442.86 ms | 100.00 ms |
| GUI full drive · memory with results | 727.14 MiB | 711.45 MiB | 1695.72 MiB |
| GUI · usable-window startup | 171.07 ms | 515.95 ms | 836.03 ms |
| Full C: volume · kernel CPU | 150.87 CPU-s (refresh pending) | 207.08 CPU-s | 231.09 CPU-s |
| GUI lognormal · memory growth over 10 rescans | -2.23 MiB | -1.97 MiB | 18.94 MiB |
| GUI session · time to first result | 419.40 ms | 412.17 ms | 2443.65 ms |
| Reliability · failed scans on hostile datasets | 0.00 % (refresh pending) | 0.00 % | 0.00 % |
| Windows folder · scan workflow | 5857.26 ms (refresh pending) | 6196.51 ms | 8795.17 ms |
| Full C: volume · peak threads | 8.00 threads (refresh pending) | 18.00 threads | 17.00 threads |
| Startup working set at first idle | 13.99 MiB (refresh pending) | 15.82 MiB | 31.27 MiB |
| Startup peak working set at first idle | 13.99 MiB (refresh pending) | 15.82 MiB | 31.27 MiB |
| Administrator · full-drive private memory | 475.60 MiB (refresh pending) | 787.74 MiB | 955.15 MiB |
| Full C: volume · peak commit | 634.47 MiB (refresh pending) | 741.89 MiB | 1619.43 MiB |
| Startup · first idle message queue | 54.47 ms (refresh pending) | 64.33 ms | 224.26 ms |
| Administrator · full-drive peak memory | 456.96 MiB (refresh pending) | 807.89 MiB | 978.90 MiB |
| Startup CPU at first idle | 0.04 CPU-s (refresh pending) | 0.05 CPU-s | 0.22 CPU-s |
| Full C: volume · memory per million files | 184.40 MiB (refresh pending) | 228.66 MiB | 491.73 MiB |
| Full C: volume · page faults | 178083.50 faults (refresh pending) | 221519.00 faults | 498694.50 faults |
| GUI full drive · scan to results | 41228.05 ms | 84950.32 ms | 55495.41 ms |
| Standard-user · full-drive private memory | 304.37 MiB (refresh pending) | 412.49 MiB | 877.70 MiB |
| 500k empty folders · scan workflow | 22071.17 ms (refresh pending) | 31265.21 ms | 38119.39 ms |
| Standard-user · full-drive peak memory | 292.44 MiB (refresh pending) | 436.33 MiB | 897.85 MiB |
| User profile · scan workflow | 34547.26 ms (refresh pending) | 54965.56 ms | 79838.65 ms |
| Full C: volume · package energy | 675.81 J (refresh pending) | 1111.33 J | 1539.59 J |
| Standard-user · full-drive CPU consumption | 48.13 CPU-s (refresh pending) | 108.82 CPU-s | 126.76 CPU-s |
| Empty-file scale · CPU consumption | 0.09 CPU-s (refresh pending) | 0.16 CPU-s | 1.13 CPU-s |
| Full C: volume · 90th percentile completion | 18396.97 ms (refresh pending) | 33559.98 ms | 49471.20 ms |
| Full C: volume · scan workflow | 17200.35 ms (refresh pending) | 33275.54 ms | 45038.72 ms |
| Empty-file scale · private memory | 4.09 MiB (refresh pending) | 8.53 MiB | 52.65 MiB |
| GUI full drive · exit with results | 96.83 ms | 723.66 ms | 3642.77 ms |
| Full C: volume · disk bytes read | 0.01 MiB (refresh pending) | 0.11 MiB | 21.16 MiB |
| Administrator · full-drive workflow | 5588.50 ms (refresh pending) | 14292.22 ms | 29188.57 ms |
| Full C: volume · user CPU | 12.84 CPU-s (refresh pending) | 33.27 CPU-s | 87.73 CPU-s |
| Empty-file scale · peak memory | 8.13 MiB (refresh pending) | 26.75 MiB | 76.92 MiB |
| GUI full drive · USER objects with results | 35.00 objects | 119.00 objects | 100.00 objects |
| GUI lognormal · repeat scan | 195.22 ms | 664.13 ms | 1423.43 ms |
| Administrator · full-drive CPU consumption | 5.37 CPU-s (refresh pending) | 18.70 CPU-s | 31.11 CPU-s |
| Full C: volume · peak handles | 73.00 handles (refresh pending) | 326.00 handles | 464.00 handles |
| Git repository · scan workflow | 86.42 ms (refresh pending) | 333.44 ms | 804.23 ms |
| Scan while disk is busy · scan workflow | 295.57 ms (refresh pending) | 1379.36 ms | 4546.75 ms |
| node_modules tree · scan workflow | 213.37 ms (refresh pending) | 1011.02 ms | 2630.23 ms |
| Files changing during scan · scan workflow | 374.08 ms (refresh pending) | 1832.81 ms | 6520.72 ms |
| Empty-file scale · 90th percentile completion | 79.32 ms (refresh pending) | 400.71 ms | 916.51 ms |
| 1-million-file scale · scan workflow | 621.53 ms (refresh pending) | 3145.30 ms | 10751.92 ms |
| Deep long paths · scan workflow | 69.28 ms (refresh pending) | 374.15 ms | 776.57 ms |
| 1M files under a 1 GiB memory limit · scan workflow | 589.84 ms (refresh pending) | 3225.14 ms | 11295.44 ms |
| Nonempty dense folder · CPU consumption | 0.02 CPU-s (refresh pending) | 0.16 CPU-s | 0.79 CPU-s |
| Feature fixture · scan/CSV peak memory | 3.07 MiB (refresh pending) | 41.87 MiB | 76.94 MiB |
| Quick folder · peak memory | 2.95 MiB (refresh pending) | 26.30 MiB | 76.94 MiB |
| GUI full drive · GDI objects with results | 22.00 objects | 763.00 objects | 235.00 objects |
| Lognormal size mix · scan workflow | 183.60 ms (refresh pending) | 1061.88 ms | 3225.13 ms |
| Empty-file scale · scan workflow | 65.54 ms (refresh pending) | 383.31 ms | 901.51 ms |
| Nonempty dense folder · 90th percentile completion | 59.99 ms (refresh pending) | 351.96 ms | 789.72 ms |
| Nonempty dense folder · peak memory | 3.00 MiB (refresh pending) | 26.75 MiB | 76.97 MiB |
| Lognormal mix under CPU load · scan workflow | 287.36 ms (refresh pending) | 1881.37 ms | 7352.69 ms |
| Unicode names · scan workflow | 66.89 ms (refresh pending) | 447.00 ms | 1195.86 ms |
| Hard-link fixture · scan workflow | 80.92 ms (refresh pending) | 599.96 ms | 1592.87 ms |
| Nonempty dense folder · scan workflow | 45.17 ms (refresh pending) | 339.68 ms | 781.20 ms |
| Feature fixture · scan/CSV private memory | 0.49 MiB (refresh pending) | 11.51 MiB | 52.68 MiB |
| Quick folder · 90th percentile completion | 35.08 ms (refresh pending) | 337.73 ms | 765.61 ms |
| Quick folder · scan workflow | 30.64 ms (refresh pending) | 311.86 ms | 723.35 ms |
| Quick folder · private memory | 0.44 MiB (refresh pending) | 8.53 MiB | 52.59 MiB |
| Feature fixture · scan/CSV time | 30.09 ms (refresh pending) | 308.94 ms | 715.14 ms |
| Churned folder · scan workflow | 34.15 ms (refresh pending) | 328.00 ms | 810.06 ms |
| 10 GiB of 1 MiB files · scan workflow | 28.97 ms (refresh pending) | 334.26 ms | 814.37 ms |
| Single 50 GiB sparse file · scan workflow | 19.23 ms (refresh pending) | 291.33 ms | 686.69 ms |
| Sparse and compressed files · scan workflow | 24.15 ms (refresh pending) | 303.46 ms | 738.68 ms |
| Junction maze · scan workflow | 22.33 ms (refresh pending) | 303.81 ms | 736.02 ms |
| OneDrive cloud placeholders · scan workflow | 22.47 ms (refresh pending) | 305.92 ms | 775.38 ms |
| Nonempty dense folder · private memory | 0.45 MiB (refresh pending) | 7.90 MiB | 52.60 MiB |
| Alternate data streams · scan workflow | 22.73 ms (refresh pending) | 322.91 ms | 728.56 ms |
| Access-denied subtree · scan workflow | 18.70 ms (refresh pending) | 295.53 ms | 720.64 ms |
| GUI full drive · cancel latency | 6.56 ms | 171.03 ms | 601.75 ms |
| Quick folder · CPU consumption | 0.00 CPU-s (refresh pending) | 0.17 CPU-s | 0.79 CPU-s |
| Feature fixture · scan/CSV CPU | 0.00 CPU-s (refresh pending) | 0.27 CPU-s | 0.69 CPU-s |

## Validation and limitations

Original category coverage is recorded in [current-medal-rerun-validation.json](current-medal-rerun-validation.json). Feature output validation accepts only verified measurements. Executable SHA-256 identities and WinBlaze companion DLL identities are embedded in the evidence.

Caches are uncontrolled, and live C: contents and accessible counts can change between runs. Sampled CPU and memory are lower bounds. The three original administrator categories retain their documented mixed scope: WinBlaze native scan/index versus competitor startup/scan/export; native process resources include deferred persistence. Medal definitions come from original-medal-events.json (81 categories); definition changes, such as the retired timing-variability medals, are recorded in benchmarks/README.md. Supplemental historical GUI, engine and feature support evidence remains separately dated. This benchmark rerun does not establish a fresh full repository test or coverage gate.
