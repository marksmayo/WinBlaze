$ErrorActionPreference = 'Stop'
# FILE_STANDARD_INFO reports cluster allocation independently of tool accounting.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class BenchmarkAllocation {
    [StructLayout(LayoutKind.Sequential)] public struct StandardInfo {
        public long AllocationSize;
        public long EndOfFile;
        public uint NumberOfLinks;
        [MarshalAs(UnmanagedType.U1)] public bool DeletePending;
        [MarshalAs(UnmanagedType.U1)] public bool Directory;
    }
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool GetFileInformationByHandleEx(SafeFileHandle file, int infoClass, out StandardInfo info, uint size);
}
'@
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$rows = foreach ($profile in @('tiny', 'fanout-large')) {
    $root = Join-Path $repo "artifacts/benchmark-datasets/$profile"
    $file = Get-ChildItem -LiteralPath $root -Recurse -File | Select-Object -First 1
    $stream = [IO.File]::Open($file.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        $info = New-Object BenchmarkAllocation+StandardInfo
        $ok = [BenchmarkAllocation]::GetFileInformationByHandleEx($stream.SafeFileHandle, 1, [ref]$info, [Runtime.InteropServices.Marshal]::SizeOf($info))
        if (!$ok) { throw "FILE_STANDARD_INFO failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
        [pscustomobject]@{ profile=$profile; sample_file=$file.Name; logical_bytes=$info.EndOfFile; allocated_bytes=$info.AllocationSize; method='GetFileInformationByHandleEx(FileStandardInfo)'; scope='one deterministic file per profile, not aggregate allocation'; utc=[DateTime]::UtcNow.ToString('o') }
    } finally { $stream.Dispose() }
}
ConvertTo-Json -InputObject @($rows) -Depth 6 | Set-Content (Join-Path $PSScriptRoot 'current-allocation-results.json') -Encoding UTF8
$rows
