# Competitor Baseline Report

Generated from local benchmark artifacts.

## WinBlaze Local Baselines

| Profile | Files | Directories | Elapsed ms | Working set MB | Peak frame ms | Peak flush ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| tiny | 72 | 13 | 481 | 168.7 | 85 | 13 |
| fanout | 2048 | 3 | 466 | 175.3 | 51 | 27 |
| scale | 16384 | 545 | 441 | 189.9 | 82 | 59 |

## WinBlaze Release Medians

| Profile | Runs | First elapsed ms | Warmed median ms | Overall median ms | Median working set MB |
| --- | ---: | ---: | ---: | ---: | ---: |
| tiny | 3 | 657 | 658 | 658 | 167.5 |
| small | 3 | 550 | 608 | 608 | 174.7 |
| fanout | 3 | 562 | 561 | 562 | 177.2 |
| fanout-large | 3 | 548 | 541 | 547 | 191.3 |
| scale | 3 | 1229 | 637 | 723 | 203.3 |

## Competitor Tool Inventory

| Tool | Installed | Version | Manual timing ms | Path |
| --- | --- | --- | ---: | --- |
| WizTree | yes | 4.31 | not recorded | C:\Program Files\WizTree\WizTree64.exe |
| WinDirStat | yes | 2.6.0 | not recorded | C:\Program Files\WinDirStat\WinDirStat.exe |
| Everything | no | not recorded | not recorded | not recorded |

## Dataset Used For Competitor Timing

- Profile: scale
- Root: C:\tmp\WinBlazeBench\scale
- Files: 16384
- Directories: 545
- Bytes: 0

## Notes

- WinBlaze local baselines are single-run UI-driven measurements; Release medians are separate repeated-run measurements when `winblaze-release-medians.json` is present.
- Competitor timings are manual fields in `competitor-baselines.json`; blank values intentionally render as `not recorded`.
- Tool inventory is still useful because it records which comparison targets are locally available before timed runs.
- To add manual timings, rerun benchmarks\record-competitor-baselines.ps1 with -WizTreeElapsedMs, -WinDirStatElapsedMs, or -EverythingElapsedMs, then regenerate this report.
- Source WinBlaze baseline: C:\Users\markm\Github\WinBlaze\benchmarks\winblaze-baselines.json.
- Source Release median baseline: C:\Users\markm\Github\WinBlaze\benchmarks\winblaze-release-medians.json.
- Source competitor baseline: C:\Users\markm\Github\WinBlaze\benchmarks\competitor-baselines.json.
