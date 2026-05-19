# WinBlaze Performance Calibration Report

Generated: 2026-05-19T00:57:05.1602508Z

## Environment

- Machine: MM-TEREM
- OS: Win32NT 10.0.26200.0 build 26200
- CPU: Intel64 Family 6 Model 154 Stepping 3, GenuineIntel, logical processors 20
- Dataset storage: C:\tmp\WinBlazeBench on C:\, filesystem n/a
- Power: Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)

## Release Medians

| Profile | First ms | Warmed median ms | Median ms | Working set MB | Last latency ms | Input ms | Peak frame ms | Peak flush ms | Scan duration ms | Treemap renders | Budget result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| tiny | 567 | 512 | 567 | 173.4 | 19 | 0 | 16 | 14 | 6 | 2/3 | pass |
| small | 572 | 547 | 571 | 181.5 | 3 | 0 | 12 | 33 | 20 | 2/3 | not budgeted |
| fanout | 577 | 575 | 577 | 184.5 | 13 | 0 | 14 | 32 | 11 | 2/3 | pass |
| fanout-large | 493 | 505 | 505 | 202 | 50 | 0 | 14 | 69 | 31 | 2/4 | pass |
| scale | 433 | 456 | 456 | 204.7 | 26 | 0 | 12 | 69 | 135 | 3/5 | pass |

## Calibration Notes

- The report is generated from checked-in Release repeated-run medians and local Release budgets.
- Treat these values as machine-specific stability gates until multiple Windows machines have recorded comparable environment captures and Release medians.
- Re-run `benchmarks\record-environment.ps1` and `benchmarks\run-release-baseline-set.ps1` before updating release budgets.
