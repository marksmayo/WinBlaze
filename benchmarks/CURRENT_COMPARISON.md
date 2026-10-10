# Disk analyzer comparison — 10 October 2026

The [offline HTML report](CURRENT_COMPARISON.html) contains the refreshed results, medals, individual observations and executable identities.

Ten validated observations per metric and product. This batch refreshes WinBlaze. Standard-user batch `refresh-20261009T135820398Z` is complete. Administrator collection: complete. 35/35 original medal categories have complete accepted evidence.

| Product | Gold | Silver | Bronze |
|---|---:|---:|---:|
| WinBlaze | 30 | 5 | 0 |
| WizTree | 3 | 2 | 30 |
| WinDirStat | 2 | 28 | 5 |

## Full-drive standard-user workflow

Startup + scan/indexing + complete CSV export, excluding GUI rendering.

| Product | Median time | Sampled CPU | Sampled peak working set | Sampled peak private memory |
|---|---:|---:|---:|---:|
| WinBlaze | 17.08 s | 61.58 CPU-s | 448.01 MiB | 469.89 MiB |
| WinDirStat | 25.91 s | 78.46 CPU-s | 473.29 MiB | 444.84 MiB |
| WizTree | 38.09 s | 133.92 CPU-s | 946.52 MiB | 923.88 MiB |

## Startup

Unchanged hidden-window first-idle protocol; this does not establish visible, usable-window readiness.

| Product | First idle | CPU at first idle | Working set at first idle |
|---|---:|---:|---:|
| WinBlaze | 50.17 ms | 31.25 ms | 14.64 MiB |
| WinDirStat | 66.61 ms | 62.50 ms | 16.24 MiB |
| WizTree | 235.61 ms | 210.94 ms | 31.66 MiB |

WinDirStat and WizTree retain the lowest validated n=10 statistic per medal metric and executable version/hash. Subsequent iterations reuse complete competitor baselines until their version or executable hash changes; missing baselines require collection. WinBlaze is rerun and uses the latest validated batch per metric; pending refreshes retain their dated previous result. Each metric may select a different dated batch. Raw latest observations remain separately dated.

## All 35 medal results

| Measurement | WinBlaze | WinDirStat | WizTree |
|---|---:|---:|---:|
| Quick folder · timing variability | 11.73 % | 14.85 % | 2.96 % |
| Empty-file scale · timing variability | 8.39 % | 20.11 % | 4.01 % |
| Nonempty dense folder · timing variability | 8.56 % | 18.58 % | 5.40 % |
| Full C: volume · timing variability | 6.20 % | 5.23 % | 26.43 % |
| Standard-user · full-drive private memory | 469.89 MiB | 444.84 MiB | 923.88 MiB |
| Standard-user · full-drive peak memory | 448.01 MiB | 473.29 MiB | 946.52 MiB |
| Startup working set at first idle | 14.64 MiB | 16.24 MiB | 31.66 MiB |
| Startup peak working set at first idle | 14.64 MiB | 16.24 MiB | 39.76 MiB |
| Full C: volume · 90th percentile completion | 24203.03 ms | 27647.20 ms | 48326.16 ms |
| Standard-user · full-drive CPU consumption | 61.58 CPU-s | 78.46 CPU-s | 133.92 CPU-s |
| Startup · first idle message queue | 50.17 ms | 66.61 ms | 235.61 ms |
| Full C: volume · scan workflow | 17080.21 ms | 25911.71 ms | 38085.12 ms |
| Administrator · full-drive peak memory | 645.00 MiB | 1124.64 MiB | 1081.67 MiB |
| Startup CPU at first idle | 0.03 CPU-s | 0.06 CPU-s | 0.21 CPU-s |
| Empty-file scale · CPU consumption | 0.09 CPU-s | 0.36 CPU-s | 1.23 CPU-s |
| Administrator · full-drive CPU consumption | 6.02 CPU-s | 46.82 CPU-s | 32.59 CPU-s |
| Administrator · full-drive workflow | 2904.00 ms | 16254.14 ms | 30045.28 ms |
| Empty-file scale · scan workflow | 69.68 ms | 391.94 ms | 726.82 ms |
| Quick folder · 90th percentile completion | 75.70 ms | 426.76 ms | 691.33 ms |
| Empty-file scale · 90th percentile completion | 80.27 ms | 483.75 ms | 871.98 ms |
| Nonempty dense folder · 90th percentile completion | 61.51 ms | 402.10 ms | 753.26 ms |
| Quick folder · private memory | 1.52 MiB | 11.04 MiB | 36.92 MiB |
| Nonempty dense folder · scan workflow | 46.38 ms | 341.26 ms | 691.05 ms |
| Feature fixture · scan/CSV time | 44.39 ms | 354.31 ms | 674.11 ms |
| Feature fixture · scan/CSV peak memory | 5.59 MiB | 49.07 MiB | 63.21 MiB |
| Quick folder · peak memory | 5.32 MiB | 52.31 MiB | 63.10 MiB |
| Nonempty dense folder · peak memory | 4.93 MiB | 49.47 MiB | 64.53 MiB |
| Quick folder · scan workflow | 36.20 ms | 364.27 ms | 595.40 ms |
| Empty-file scale · peak memory | 5.11 MiB | 54.81 MiB | 68.35 MiB |
| Nonempty dense folder · private memory | 0.92 MiB | 12.56 MiB | 39.65 MiB |
| Feature fixture · scan/CSV private memory | 0.99 MiB | 15.03 MiB | 36.78 MiB |
| Feature fixture · scan/CSV CPU | 0.02 CPU-s | 0.25 CPU-s | 0.57 CPU-s |
| Nonempty dense folder · CPU consumption | 0.02 CPU-s | 0.29 CPU-s | 0.63 CPU-s |
| Empty-file scale · private memory | 1.13 MiB | 28.10 MiB | 43.38 MiB |
| Quick folder · CPU consumption | 0.00 CPU-s | 0.30 CPU-s | 0.55 CPU-s |

## Validation and limitations

Original category coverage is recorded in [current-medal-rerun-validation.json](current-medal-rerun-validation.json). Feature output validation accepts only verified measurements. Executable SHA-256 identities and WinBlaze companion DLL identities are embedded in the evidence.

Caches are uncontrolled, and live C: contents and accessible counts can change between runs. Sampled CPU and memory are lower bounds. The three original administrator categories retain their documented mixed scope: WinBlaze native scan/index versus competitor startup/scan/export; native process resources include deferred persistence. No scope or medal definition was changed for this rerun. Supplemental historical GUI, engine and feature support evidence remains separately dated. This benchmark rerun does not establish a fresh full repository test or coverage gate.
