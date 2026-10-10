"""Independently check resident-file accounting in the owned dense fixture."""
import ctypes as c
from ctypes import wintypes as w
import json
from pathlib import Path

repo = Path(__file__).resolve().parent.parent
files = list((repo / "artifacts/benchmark-datasets/fanout-large").rglob("*.bin"))
assert len(files) == 8192
kernel = c.WinDLL("kernel32", use_last_error=True)
kernel.CreateFileW.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, c.c_void_p, w.DWORD, w.DWORD, w.HANDLE]
kernel.CreateFileW.restype = w.HANDLE
kernel.GetFileInformationByHandleEx.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD]
kernel.GetFileInformationByHandleEx.restype = w.BOOL
kernel.DeviceIoControl.argtypes = [w.HANDLE, w.DWORD, c.c_void_p, w.DWORD, c.c_void_p, w.DWORD, c.POINTER(w.DWORD), c.c_void_p]
kernel.DeviceIoControl.restype = w.BOOL
kernel.CloseHandle.argtypes = [w.HANDLE]

class Standard(c.Structure):
    _fields_ = [("allocation", c.c_int64), ("length", c.c_int64), ("links", w.DWORD), ("delete_pending", c.c_ubyte), ("directory", c.c_ubyte)]

for path in files:
    handle = kernel.CreateFileW(str(path), 0, 7, None, 3, 0, None)
    assert handle not in (None, c.c_void_p(-1).value), str(path)
    try:
        info = Standard()
        assert kernel.GetFileInformationByHandleEx(handle, 1, c.byref(info), c.sizeof(info)), str(path)
        assert (info.length, info.allocation, info.links) == (256, 256, 1), str(path)
        start = c.c_int64(0)
        extents = c.create_string_buffer(4096)
        returned = w.DWORD()
        c.set_last_error(0)
        result = kernel.DeviceIoControl(handle, 0x90073, c.byref(start), c.sizeof(start), extents, c.sizeof(extents), c.byref(returned), None)
        assert not result and c.get_last_error() == 38, f"Expected resident stream without extents: {path}; error {c.get_last_error()}"
    finally:
        kernel.CloseHandle(handle)

evidence = {
    "verified": True, "files": len(files), "logical_bytes_per_file": 256,
    "windows_standard_allocation_bytes_per_file": 256,
    "retrieval_pointers": "All streams report ERROR_HANDLE_EOF at VCN 0; no nonresident extents",
    "interpretation": "WinBlaze/WinDirStat use Windows allocation bytes; WizTree exports zero cluster allocation for these resident streams. Do not compare physical totals as identical accounting.",
    "reference": "https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_get_retrieval_pointers",
}
(repo / "benchmarks/dense-resident-allocation-validation.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
print("PASS: all8192 files have Windows allocation256 and no nonresident extents; accounting difference verified")
