# Supported console allocation policy

The previous Windows-subsystem experiment broke interactive CMD waiting and
error status, so it was rejected. The current candidate retains console PE
subsystem 3 and embeds `consoleAllocationPolicy=detached` instead.

Microsoft documents support starting with Windows 11 24H2 / Windows Server 2025:
[console allocation policy](https://learn.microsoft.com/en-us/windows/console/console-allocation-policy).
This machine is build 26300.9550. Older Windows versions retain their existing
console behavior; no additional executable or benchmark boundary is introduced.

`tests/native/cli-console-policy.ps1` failed on the current D576E8F1 CLI because
the embedded policy was absent. The isolated candidate passed: console subsystem
unchanged, full three-file/six-byte CSV, JSON before the following CMD command,
success status 0, and usage stderr/status 2. Build and portable-package gates now
exercise this same contract on their actual output binaries.

The twenty AB/BA observations in `artifacts/all-gold-loop/iteration-31` completed
with all 72-file exports and exit codes verified. Median hidden launch elapsed
time was 84.71 ms baseline and 48.455 ms candidate. This diagnostic does not
replace medal observations. Sampled CPU often reads zero and cannot establish a
CPU reduction. Candidate child sampled peak/private memory was higher, so this
experiment does not establish a memory reduction either. Console-host resource
savings are not quantified. Full product gates/adoption are tracked with
iteration 32's memory candidate.

An earlier sandbox escalation review hit a usage limit before this diagnostic
could start. The same explicitly reviewed command subsequently succeeded after
the continuation; no permission boundary or UAC prompt was bypassed.
