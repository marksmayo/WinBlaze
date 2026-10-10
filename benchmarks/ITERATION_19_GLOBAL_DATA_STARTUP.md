# Iteration 19: global data elimination rejected

The isolated full OS XAML host was rebuilt with MSVC /Gw and explicit /OPT:REF, preserving the bitmap icons, SegmentHeap manifest, full UI sources, and compact-record native DLL. Microsoft documents /Gw as packaging global data so unused data can be removed by /OPT:REF: https://learn.microsoft.com/en-us/cpp/build/reference/gw-optimize-global-data?view=msvc-170. This experiment did not enable /GL or link-time code generation.

The unchanged resource verifier first failed against iteration 18 at 16.28125 MiB versus the retained WinDirStat target of 16.244140625 MiB. The rebuilt image remained exactly 1,932,288 bytes. The new ten-run canonical cohort measured 16.95703125 MiB, so this variant is rejected. All samples, including the first 23.77734375 MiB observation, are retained. The original verifier also rejected the observed zero CPU sample in run 3 under its positive-resource assertion before reaching the memory comparison; this is not represented as a successful resource test or silently repaired.

UI SHA256: 4755DA1A021DF902B50E8B2DB93619F79408030A163358C8EBC793A5062815F0. Native SHA256: 9FF4B2BAC5DAFD0160FF06874D21528044C7EDC2B9E1F2AC25996808834A9F62.

Icon pixels pass against the preceding candidate. Twenty matched diagnostic launches pass all five controls, owned-window checks, per-monitor-v2 DPI, exact OS runtime, and normal shutdown. See artifacts/all-gold-loop/iteration-19/paired-summary.json and iteration-9/full-ui-paired-0d23197ea1fd48c989a53a66ce5c6b61/observations.json. These diagnostics are not medal measurements or proof of a causal improvement.

No production source or official report values changed. The report retains all 35 results at 27 gold, 7 silver, and 1 bronze. All 70 competitor best records remain frozen. The next isolated hypothesis starts from iteration 18 without the rejected /Gw flag: disable obsolete OLE DDE setup using a balanced CoInitializeEx call while preserving WinRT initialization and window/XAML release order. It requires fresh fidelity and canonical evidence before any adoption decision.
