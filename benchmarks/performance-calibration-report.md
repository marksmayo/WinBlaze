# WinBlaze Performance Calibration Report

Generated: 2026-05-19T01:46:33.4084127Z

## Environment

- Machine: MM-TEREM
- OS: Win32NT 10.0.26200.0 build 26200
- CPU: Intel64 Family 6 Model 154 Stepping 3, GenuineIntel, logical processors 20
- Dataset storage: C:\tmp\WinBlazeBench on C:\, filesystem n/a
- Power: Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)

## Release Medians

| Profile | First ms | Warmed median ms | Median ms | Working set MB | Last latency ms | Input ms | Peak frame ms | Peak flush ms | Scan duration ms | Treemap renders | Budget result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| tiny | 516 | 576 | 576 | 173.2 | 23 | 0 | 12 | 13 | 7 | 2/3 | pass |
| small | 542 | 505 | 542 | 180.9 | 9 | 0 | 14 | 30 | 19 | 2/3 | not budgeted |
| fanout | 561 | 638 | 638 | 185.2 | 12 | 0 | 11 | 33 | 11 | 2/3 | pass |
| fanout-large | 593 | 541 | 593 | 204.3 | 52 | 0 | 15 | 79 | 39 | 2/4 | pass |
| scale | 569 | 492 | 493 | 208.8 | 68 | 0 | 14 | 91 | 97 | 3/5 | pass |

## Calibration Notes

- The report is generated from checked-in Release repeated-run medians and local Release budgets.
- Treat these values as machine-specific stability gates until multiple Windows machines have recorded comparable environment captures and Release medians.
- Re-run `benchmarks\record-environment.ps1` and `benchmarks\run-release-baseline-set.ps1` before updating release budgets.
