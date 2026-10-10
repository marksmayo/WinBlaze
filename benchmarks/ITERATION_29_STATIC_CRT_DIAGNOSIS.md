# Isolated static-CRT CLI experiment: not adopted

The current completed iteration28 cohort remains32 gold,3 silver,0 bronze;
startup CPU is pending because one zero reading leaves9 qualifying samples.
All35 rows and70 frozen competitor records/values remain intact.

An isolated pinned1.96.0 offline Release build applies crt-static in a separate
artifact target directory. Product binaries, source defaults and official
benchmark observations are unchanged. The PE dependency contract has actual
RED/GREEN: current D576E8F1 directly imports VCRUNTIME140 and five CRT API DLLs;
candidate704291D6 imports none of those six. Its executable grows from1,060,864
to1,160,704 bytes. This is a direct-import result, not proof of runtime benefit.

Twenty alternating baseline/candidate observations validate all72 fixture files,
the requested root, completion output and zero exit status. All observations,
including the candidate's initial245ms launch, are retained. This direct
Process.Start diagnostic differs from the official hidden launch protocol and
does not replace medal evidence.

| Diagnostic | Current | Static CRT |
|---|---:|---:|
| Whole-process median ms | 25.81125 | 26.34645 |
| IQR / median percent | 8.567001 | 15.125852 |
| Internal phase median ms | 7.382 | 7.6395 |
| Median sampled CPU ms | 15.625 | 15.625 |

The candidate wins only5 of10 paired wall-time observations. No speed or
consistency benefit is established, so it is not adopted. The filesystem
write-time partition is only a coarse proxy and varies between cohorts; it
must not establish exact pre-main versus exit attribution on its own.

Evidence under artifacts/all-gold-loop/iteration-29: static-crt-build.log,
crt-imports-red.json, crt-imports-green.json and static-crt-phases/observations.json.
