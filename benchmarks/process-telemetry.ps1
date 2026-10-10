# Use the owned kernel handle: managed process-list snapshots can lose a short
# process after exit, although its lifetime counters remain queryable.
if(!('WinBlazeBenchmarkTelemetry' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public sealed class WinBlazeTelemetrySnapshot {
    public double? cpu_ms { get; set; }
    public long? peak_working_set_bytes { get; set; }
    public long? private_bytes { get; set; }
}
public static class WinBlazeBenchmarkTelemetry {
    [StructLayout(LayoutKind.Sequential)]
    private struct MemoryCounters {
        public uint Size, PageFaults;
        public UIntPtr PeakWorkingSet, WorkingSet, PeakPagedPool, PagedPool;
        public UIntPtr PeakNonPagedPool, NonPagedPool, Commit, PeakCommit, Private;
    }
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetProcessTimes(IntPtr process, out long created,
        out long exited, out long kernel, out long user);
    [DllImport("psapi.dll", SetLastError = true)]
    private static extern bool GetProcessMemoryInfo(IntPtr process,
        ref MemoryCounters counters, uint size);
    public static WinBlazeTelemetrySnapshot Capture(IntPtr handle) {
        var result = new WinBlazeTelemetrySnapshot();
        long created, exited, kernel, user;
        if (GetProcessTimes(handle, out created, out exited, out kernel, out user))
            result.cpu_ms = ((double)kernel + user) / 10000.0;
        var counters = new MemoryCounters();
        counters.Size = (uint)Marshal.SizeOf(typeof(MemoryCounters));
        if (GetProcessMemoryInfo(handle, ref counters, counters.Size)) {
            ulong peak = counters.PeakWorkingSet.ToUInt64();
            ulong bytes = counters.Private.ToUInt64();
            if (peak > 0 && peak <= long.MaxValue) result.peak_working_set_bytes = (long)peak;
            if (bytes > 0 && bytes <= long.MaxValue) result.private_bytes = (long)bytes;
        }
        return result;
    }
}
'@
    # Warm the interop outside child measurement, so the first short process
    # does not exit while the collector's own methods are first compiled.
    $telemetryWarmProcess=[Diagnostics.Process]::GetCurrentProcess()
    try { $null=[WinBlazeBenchmarkTelemetry]::Capture($telemetryWarmProcess.Handle) }
    finally { $telemetryWarmProcess.Dispose() }
}
function Get-BenchmarkProcessTelemetry {
    param([Diagnostics.Process]$Process)
    try {
        return [WinBlazeBenchmarkTelemetry]::Capture($Process.Handle)
    } catch {
        return [WinBlazeTelemetrySnapshot]::new()
    }
}

function Add-BenchmarkProcessTelemetry {
    param([System.Collections.IDictionary]$State,[object]$Snapshot)
    $observed=$false
    foreach($name in @('cpu_ms','peak_working_set_bytes','private_bytes')) {
        if($null -ne $Snapshot.$name) {
            $State[$name]=if($null -eq $State[$name]){$Snapshot.$name}else{[Math]::Max($State[$name],$Snapshot.$name)}
            $observed=$true
        }
    }
    if($observed){$State['sample_count']++}
}
