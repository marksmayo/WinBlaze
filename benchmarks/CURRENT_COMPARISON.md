# Disk analyzer comparison — 17 October 2026

The [offline HTML report](CURRENT_COMPARISON.html) contains the refreshed results, medals, individual observations and executable identities.

Ten validated observations per metric and product. This batch refreshes WinBlaze, WinDirStat, WizTree. Standard-user batch `surface-book-20261012T115706Z` is complete. Administrator collection: complete. 76/35 original medal categories have complete accepted evidence.

| Product | Gold | Silver | Bronze |
|---|---:|---:|---:|
| WinBlaze | 69 | 6 | 3 |
| WinDirStat | 4 | 42 | 19 |
| Everything | 4 | 13 | 10 |
| SpaceSniffer | 4 | 6 | 3 |
| Directory Report | 2 | 10 | 21 |
| TreeSize Free | 2 | 0 | 2 |
| WizTree | 1 | 2 | 22 |

## Full-drive standard-user workflow

Startup + scan/indexing + complete CSV export, excluding GUI rendering.

| Product | Median time | Sampled CPU | Sampled peak working set | Sampled peak private memory |
|---|---:|---:|---:|---:|
| WinBlaze | 21.67 s | 77.20 CPU-s | 319.69 MiB | 331.38 MiB |
| WinDirStat | 33.28 s | 108.82 CPU-s | 436.33 MiB | 412.49 MiB |
| WizTree | 45.04 s | 126.76 CPU-s | 897.85 MiB | 877.70 MiB |

## Startup

Unchanged hidden-window first-idle protocol; this does not establish visible, usable-window readiness.

| Product | First idle | CPU at first idle | Working set at first idle |
|---|---:|---:|---:|
| WinBlaze | 60.25 ms | 46.88 ms | 14.09 MiB |
| WinDirStat | 64.33 ms | 46.88 ms | 15.82 MiB |
| WizTree | 224.26 ms | 218.75 ms | 31.27 MiB |

Every competitor retains the lowest validated n=10 statistic per medal metric and executable version/hash. Subsequent iterations reuse complete competitor baselines until their version or executable hash changes; missing baselines require collection. WinBlaze is rerun and uses the latest validated batch per metric; pending refreshes retain their dated previous result. Each metric may select a different dated batch. Raw latest observations remain separately dated.

## All 81 medal results

| Measurement | WinBlaze | WinDirStat | WizTree |
|---|---:|---:|---:|
| GUI lognormal · refresh after a small change | 214.72 ms | 673.16 ms | 1492.78 ms |
| GUI full drive · responsiveness p95 | 0.47 ms | 35.05 ms | 0.30 ms |
| GUI session · reload saved results | 656.46 ms | 1034.90 ms | 2352.13 ms |
| Install footprint · 64-bit GUI files | 4.60 MiB (refresh pending) | 2.77 MiB | 10.51 MiB |
| Full C: volume · context switches | 395655.50 switches | 832973.00 switches | 1774781.50 switches |
| Full C: volume · disk bytes written | 516.79 MiB | 686.13 MiB | 558.79 MiB |
| GUI full drive · memory with results | 715.12 MiB | 711.45 MiB | 1695.72 MiB |
| GUI · usable-window startup | 182.93 ms | 515.95 ms | 836.03 ms |
| GUI session · redraw after resize | 67.15 ms | 442.86 ms | 100.00 ms |
| Full C: volume · kernel CPU | 154.46 CPU-s | 207.08 CPU-s | 231.09 CPU-s |
| Deep long paths · scan workflow | 390.55 ms | 374.15 ms | 776.57 ms |
| Reliability · failed scans on hostile datasets | 0.00 % | 0.00 % | 0.00 % |
| Startup CPU at first idle | 0.05 CPU-s | 0.05 CPU-s | 0.22 CPU-s |
| GUI session · time to first result | 405.22 ms | 412.17 ms | 2443.65 ms |
| Full C: volume · peak threads | 8.00 threads | 18.00 threads | 17.00 threads |
| Startup · first idle message queue | 60.25 ms | 64.33 ms | 224.26 ms |
| Standard-user · full-drive CPU consumption | 77.20 CPU-s | 108.82 CPU-s | 126.76 CPU-s |
| Startup working set at first idle | 14.09 MiB | 15.82 MiB | 31.27 MiB |
| Startup peak working set at first idle | 14.09 MiB | 15.82 MiB | 31.27 MiB |
| Administrator · full-drive private memory | 475.60 MiB (refresh pending) | 787.74 MiB | 955.15 MiB |
| Full C: volume · peak commit | 635.06 MiB | 741.89 MiB | 1619.43 MiB |
| 500k empty folders · scan workflow | 26303.59 ms | 31265.21 ms | 38119.39 ms |
| Administrator · full-drive peak memory | 456.96 MiB (refresh pending) | 807.89 MiB | 978.90 MiB |
| GUI full drive · scan to results | 46515.22 ms | 84950.32 ms | 55495.41 ms |
| Full C: volume · memory per million files | 184.41 MiB | 228.66 MiB | 491.73 MiB |
| Full C: volume · page faults | 178320.00 faults | 221519.00 faults | 498694.50 faults |
| Standard-user · full-drive private memory | 331.38 MiB | 412.49 MiB | 877.70 MiB |
| User profile · scan workflow | 40399.13 ms | 54965.56 ms | 79838.65 ms |
| Standard-user · full-drive peak memory | 319.69 MiB | 436.33 MiB | 897.85 MiB |
| Windows folder · scan workflow | 4417.17 ms | 6196.51 ms | 8795.17 ms |
| Full C: volume · 90th percentile completion | 22139.24 ms | 33559.98 ms | 49471.20 ms |
| Full C: volume · scan workflow | 21673.05 ms | 33275.54 ms | 45038.72 ms |
| Full C: volume · package energy | 702.62 J | 1111.33 J | 1539.59 J |
| Empty-file scale · CPU consumption | 0.08 CPU-s | 0.16 CPU-s | 1.13 CPU-s |
| Empty-file scale · private memory | 3.87 MiB | 8.53 MiB | 52.65 MiB |
| GUI full drive · exit with results | 104.04 ms | 723.66 ms | 3642.77 ms |
| Full C: volume · disk bytes read | 0.01 MiB | 0.11 MiB | 21.16 MiB |
| Full C: volume · user CPU | 15.02 CPU-s | 33.27 CPU-s | 87.73 CPU-s |
| node_modules tree · scan workflow | 452.85 ms | 1011.02 ms | 2630.23 ms |
| Administrator · full-drive workflow | 5588.50 ms (refresh pending) | 14292.22 ms | 29188.57 ms |
| GUI full drive · USER objects with results | 36.00 objects | 119.00 objects | 100.00 objects |
| Empty-file scale · peak memory | 7.92 MiB | 26.75 MiB | 76.92 MiB |
| Lognormal size mix · scan workflow | 357.74 ms | 1061.88 ms | 3225.13 ms |
| Scan while disk is busy · scan workflow | 460.36 ms | 1379.36 ms | 4546.75 ms |
| GUI lognormal · repeat scan | 202.03 ms | 664.13 ms | 1423.43 ms |
| 1M files under a 1 GiB memory limit · scan workflow | 1019.35 ms | 3225.14 ms | 11295.44 ms |
| Administrator · full-drive CPU consumption | 5.37 CPU-s (refresh pending) | 18.70 CPU-s | 31.11 CPU-s |
| Unicode names · scan workflow | 126.22 ms | 447.00 ms | 1195.86 ms |
| Full C: volume · peak handles | 73.00 handles | 326.00 handles | 464.00 handles |
| Lognormal mix under CPU load · scan workflow | 472.32 ms | 1881.37 ms | 7352.69 ms |
| Files changing during scan · scan workflow | 442.99 ms | 1832.81 ms | 6520.72 ms |
| 1-million-file scale · scan workflow | 733.98 ms | 3145.30 ms | 10751.92 ms |
| Hard-link fixture · scan workflow | 134.55 ms | 599.96 ms | 1592.87 ms |
| Empty-file scale · 90th percentile completion | 81.93 ms | 400.71 ms | 916.51 ms |
| Quick folder · peak memory | 3.37 MiB | 26.30 MiB | 76.94 MiB |
| Feature fixture · scan/CSV peak memory | 3.38 MiB | 41.87 MiB | 76.94 MiB |
| Nonempty dense folder · 90th percentile completion | 68.21 ms | 351.96 ms | 789.72 ms |
| Empty-file scale · scan workflow | 74.27 ms | 383.31 ms | 901.51 ms |
| Feature fixture · scan/CSV private memory | 0.71 MiB | 11.51 MiB | 52.68 MiB |
| Quick folder · private memory | 0.71 MiB | 8.53 MiB | 52.59 MiB |
| Nonempty dense folder · CPU consumption | 0.02 CPU-s | 0.16 CPU-s | 0.79 CPU-s |
| Nonempty dense folder · peak memory | 3.38 MiB | 26.75 MiB | 76.97 MiB |
| GUI full drive · GDI objects with results | 22.00 objects | 763.00 objects | 235.00 objects |
| Quick folder · 90th percentile completion | 46.19 ms | 337.73 ms | 765.61 ms |
| Nonempty dense folder · scan workflow | 54.46 ms | 339.68 ms | 781.20 ms |
| Git repository · scan workflow | 46.58 ms | 333.44 ms | 804.23 ms |
| Churned folder · scan workflow | 44.25 ms | 328.00 ms | 810.06 ms |
| Quick folder · scan workflow | 32.36 ms | 311.86 ms | 723.35 ms |
| Feature fixture · scan/CSV time | 31.20 ms | 308.94 ms | 715.14 ms |
| Nonempty dense folder · private memory | 0.71 MiB | 7.90 MiB | 52.60 MiB |
| 10 GiB of 1 MiB files · scan workflow | 32.86 ms | 334.26 ms | 814.37 ms |
| OneDrive cloud placeholders · scan workflow | 27.30 ms | 305.92 ms | 775.38 ms |
| Access-denied subtree · scan workflow | 25.76 ms | 295.53 ms | 720.64 ms |
| Sparse and compressed files · scan workflow | 26.14 ms | 303.46 ms | 738.68 ms |
| Single 50 GiB sparse file · scan workflow | 19.55 ms | 291.33 ms | 686.69 ms |
| Alternate data streams · scan workflow | 26.34 ms | 322.91 ms | 728.56 ms |
| Junction maze · scan workflow | 20.94 ms | 303.81 ms | 736.02 ms |
| GUI full drive · cancel latency | 7.17 ms | 171.03 ms | 601.75 ms |
| Quick folder · CPU consumption | 0.00 CPU-s | 0.17 CPU-s | 0.79 CPU-s |
| Feature fixture · scan/CSV CPU | 0.00 CPU-s | 0.27 CPU-s | 0.69 CPU-s |
| GUI lognormal · memory growth over 10 rescans | 8.54 MiB | -1.97 MiB | 18.94 MiB |

## Validation and limitations

Original category coverage is recorded in [current-medal-rerun-validation.json](current-medal-rerun-validation.json). Feature output validation accepts only verified measurements. Executable SHA-256 identities and WinBlaze companion DLL identities are embedded in the evidence.

Caches are uncontrolled, and live C: contents and accessible counts can change between runs. Sampled CPU and memory are lower bounds. The three original administrator categories retain their documented mixed scope: WinBlaze native scan/index versus competitor startup/scan/export; native process resources include deferred persistence. Medal definitions come from original-medal-events.json (81 categories); definition changes, such as the retired timing-variability medals, are recorded in benchmarks/README.md. Supplemental historical GUI, engine and feature support evidence remains separately dated. This benchmark rerun does not establish a fresh full repository test or coverage gate.
