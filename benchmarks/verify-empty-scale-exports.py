"""Verify all thirty complete scan exports against the owned empty-file fixture."""
import csv
import hashlib
import json
import os
import sys
from pathlib import Path

repo = Path(__file__).resolve().parent.parent
profile = sys.argv[1] if len(sys.argv) > 1 else "scale"
assert profile in ("scale", "tiny", "fanout-large")
root = repo / f"artifacts/benchmark-datasets/{profile}"
normalize = lambda value: os.path.normpath(str(value)).casefold()
expected = {normalize(Path(directory) / name) for directory, _, names in os.walk(root) for name in names}
count = {"tiny": 72, "scale": 16384, "fanout-large": 8192}[profile]
assert len(expected) == count, "Fixture file count differs from oracle"
if profile == "fanout-large":
    evidence = json.loads((repo / "benchmarks/dense-resident-allocation-validation.json").read_text(encoding="utf-8"))
    assert evidence["verified"] and evidence["files"] == count, "Verify resident-file accounting independently first"
verified = []
for tool in ["WinBlaze", "WinDirStat", "WizTree"]:
    for run in range(1, 11):
        path = repo / f"artifacts/benchmark-exports/{tool}-{profile}-{run}.csv"
        seen = []
        with path.open(encoding="utf-8-sig", newline="") as stream:
            if tool == "WizTree":
                next(stream)
            for row in csv.DictReader(stream):
                if tool == "WinBlaze":
                    is_file = row["Kind"] == "File"
                    name, logical, physical = row["Path"], row["LogicalBytes"], row["AllocatedBytes"]
                elif tool == "WinDirStat":
                    is_file = not (int(row["WinDirStat Attributes"], 16) & 4)
                    name, logical, physical = row["Name"], row["Logical Size"], row["Physical Size"]
                else:
                    is_file = not row["File Name"].endswith("\\") and not (int(row["Attributes"]) & 16)
                    name, logical, physical = row["File Name"], row["Size"], row["Allocated"]
                if is_file:
                    assert int(logical) == {"tiny": 1024, "scale": 0, "fanout-large": 256}[profile], f"Wrong logical bytes in {path}: {name}"
                    expected_physical = 0 if profile == "fanout-large" and tool == "WizTree" else {"tiny": 4096, "scale": 0, "fanout-large": 256}[profile]
                    assert int(physical) == expected_physical, f"Wrong physical bytes in {path}: {name}"
                    seen.append(normalize(name))
        assert len(seen) == len(set(seen)) == count, f"Missing or duplicate files: {path}"
        assert set(seen) == expected, f"Exported paths differ from fixture: {path}"
        verified.append({"tool": tool, "run": run, "files": len(seen), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
output = repo / f"benchmarks/{profile}-output-validation.json"
output.write_text(json.dumps({"verified": True, "outputs": verified, "allocation_semantics": "Resident fixture: Windows allocation256 for WinBlaze/WinDirStat; WizTree allocation0, consistent with no nonresident extents" if profile == "fanout-large" else "Matching logical/physical file totals"}, indent=2) + "\n", encoding="utf-8")
print(f"PASS: all30 {profile} exports contain every expected file exactly once, with exact logical/physical bytes.")
