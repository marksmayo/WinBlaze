"""Check the shipped PE's optional window-hook dependency load boundary."""
import struct
import sys
from pathlib import Path

path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe"
data = path.read_bytes()
pe = struct.unpack_from("<I", data, 60)[0]
assert data[pe:pe + 4] == b"PE\0\0"
sections, optional_size = struct.unpack_from("<H", data, pe + 6)[0], struct.unpack_from("<H", data, pe + 20)[0]
optional = pe + 24
assert struct.unpack_from("<H", data, optional)[0] == 0x20B, "Require shipped x64 PE"
section_start = optional + optional_size

def offset(rva):
    for index in range(sections):
        start = section_start + index * 40
        virtual_size, address, raw_size, raw = struct.unpack_from("<IIII", data, start + 8)
        if address <= rva < address + max(virtual_size, raw_size):
            return raw + rva - address
    raise AssertionError(f"Unmapped PE RVA {rva:x}")

def imports(directory, stride, name_offset):
    rva, size = struct.unpack_from("<II", data, optional + 112 + directory * 8)
    if not rva:
        return set()
    result = set()
    start = offset(rva)
    for cursor in range(start, start + size, stride):
        descriptor = data[cursor:cursor + stride]
        if not any(descriptor):
            break
        if directory == 13:
            assert struct.unpack_from("<I", descriptor)[0] & 1, "Require RVA delay descriptor"
        name = offset(struct.unpack_from("<I", descriptor, name_offset)[0])
        result.add(data[name:data.index(b"\0", name)].decode("ascii").casefold())
    return result

eager = imports(1, 20, 12)
delayed = imports(13, 32, 4)
for dependency in ("comctl32.dll", "shell32.dll"):
    assert dependency not in eager, f"Optional shell operations eagerly load {dependency} before first use"
    assert dependency in delayed, f"Shell operations must retain first-use {dependency} linkage"
assert "kernel32.dll" in eager and "user32.dll" in eager, "Core startup dependencies must remain linked"
print("PASS: shipped shell operations retain first-use linkage without eager COMCTL32/SHELL32 imports")
