# Dot-source: exact whole-process counters read from the kernel at exit.
# elapsed_ms is the kernel's exit-minus-create time (no launcher overhead);
# wall_ms is the launcher's Start-Process-to-exit time used by older events.
# CPU, I/O, peak memory, commit and page faults come from the exited process
# handle. Handle peaks are polled every PollMilliseconds and thread peaks every
# 250 ms (a system-wide snapshot), so both are lower bounds.
if (-not ('WinBlazeBench.ProcessMeasure' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
namespace WinBlazeBench {
public static class ProcessMeasure {
    [StructLayout(LayoutKind.Sequential)]
    public struct IO_COUNTERS { public ulong ReadOperationCount, WriteOperationCount, OtherOperationCount, ReadTransferCount, WriteTransferCount, OtherTransferCount; }
    [StructLayout(LayoutKind.Sequential)]
    public struct PROCESS_MEMORY_COUNTERS_EX {
        public uint cb, PageFaultCount;
        public UIntPtr PeakWorkingSetSize, WorkingSetSize, QuotaPeakPagedPoolUsage, QuotaPagedPoolUsage,
            QuotaPeakNonPagedPoolUsage, QuotaNonPagedPoolUsage, PagefileUsage, PeakPagefileUsage, PrivateUsage;
    }
    [DllImport("kernel32.dll", SetLastError = true)] public static extern bool GetProcessIoCounters(IntPtr h, out IO_COUNTERS c);
    [DllImport("psapi.dll", SetLastError = true)] public static extern bool GetProcessMemoryInfo(IntPtr h, out PROCESS_MEMORY_COUNTERS_EX c, uint cb);
    [DllImport("kernel32.dll", SetLastError = true)] public static extern bool GetProcessTimes(IntPtr h, out long create, out long exit, out long kernel, out long user);
    [DllImport("kernel32.dll", SetLastError = true)] public static extern bool GetProcessHandleCount(IntPtr h, out uint count);
    [StructLayout(LayoutKind.Sequential)]
    struct THREADENTRY32 { public uint dwSize, cntUsage, th32ThreadID, th32OwnerProcessID; public int tpBasePri, tpDeltaPri; public uint dwFlags; }
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint pid);
    [DllImport("kernel32.dll")] static extern bool Thread32First(IntPtr snap, ref THREADENTRY32 entry);
    [DllImport("kernel32.dll")] static extern bool Thread32Next(IntPtr snap, ref THREADENTRY32 entry);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    public static int ThreadCount(uint pid) {
        IntPtr snap = CreateToolhelp32Snapshot(4 /* TH32CS_SNAPTHREAD */, 0);
        if (snap == new IntPtr(-1)) return 0;
        int count = 0;
        try {
            var entry = new THREADENTRY32 { dwSize = (uint)Marshal.SizeOf(typeof(THREADENTRY32)) };
            for (bool ok = Thread32First(snap, ref entry); ok; ok = Thread32Next(snap, ref entry))
                if (entry.th32OwnerProcessID == pid) count++;
        } finally { CloseHandle(snap); }
        return count;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct JOBOBJECT_BASIC_LIMIT_INFORMATION { public long PerProcessUserTimeLimit, PerJobUserTimeLimit; public uint LimitFlags; public UIntPtr MinimumWorkingSetSize, MaximumWorkingSetSize; public uint ActiveProcessLimit; public UIntPtr Affinity; public uint PriorityClass, SchedulingClass; }
    [StructLayout(LayoutKind.Sequential)]
    struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION { public JOBOBJECT_BASIC_LIMIT_INFORMATION Basic; public IO_COUNTERS Io; public UIntPtr ProcessMemoryLimit, JobMemoryLimit, PeakProcessMemoryUsed, PeakJobMemoryUsed; }
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateJobObject(IntPtr attributes, string name);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int infoClass, ref JOBOBJECT_EXTENDED_LIMIT_INFORMATION info, uint length);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    // Committed-memory cap for one process (JOB_OBJECT_LIMIT_PROCESS_MEMORY);
    // allocations beyond it fail inside the process. The job handle is kept open.
    public static IntPtr LimitProcessMemory(IntPtr process, ulong bytes) {
        IntPtr job = CreateJobObject(IntPtr.Zero, null);
        var info = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
        info.Basic.LimitFlags = 0x00000100;
        info.ProcessMemoryLimit = (UIntPtr)bytes;
        if (!SetInformationJobObject(job, 9, ref info, (uint)Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION))) || !AssignProcessToJobObject(job, process))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        return job;
    }
    [DllImport("ntdll.dll")] static extern int NtQuerySystemInformation(int infoClass, IntPtr info, int length, out int returned);
    // Highest observed context-switch count per thread of `pid`, merged into
    // `perThread` (thread id -> count). x64 layouts: SYSTEM_PROCESS_INFORMATION
    // threads start at 0x100, pid at 0x50; SYSTEM_THREAD_INFORMATION is 0x50
    // bytes with ClientId.UniqueThread at 0x30 and ContextSwitches at 0x40.
    public static void SampleContextSwitches(int pid, System.Collections.Generic.Dictionary<long, long> perThread) {
        int size = 1 << 20, returned;
        for (int attempt = 0; attempt < 4; attempt++) {
            IntPtr buffer = Marshal.AllocHGlobal(size);
            try {
                int status = NtQuerySystemInformation(5, buffer, size, out returned);
                if (status == unchecked((int)0xC0000004)) { size = Math.Max(size * 2, returned + 65536); continue; }
                if (status != 0) return;
                long offset = 0;
                while (true) {
                    IntPtr entry = new IntPtr(buffer.ToInt64() + offset);
                    if (Marshal.ReadIntPtr(entry, 0x50).ToInt64() == pid) {
                        int threads = Marshal.ReadInt32(entry, 4);
                        for (int i = 0; i < threads; i++) {
                            IntPtr thread = new IntPtr(entry.ToInt64() + 0x100 + i * 0x50);
                            long tid = Marshal.ReadIntPtr(thread, 0x30).ToInt64();
                            long switches = (uint)Marshal.ReadInt32(thread, 0x40);
                            long previous;
                            if (!perThread.TryGetValue(tid, out previous) || switches > previous) perThread[tid] = switches;
                        }
                        return;
                    }
                    int next = Marshal.ReadInt32(entry, 0);
                    if (next == 0) return;
                    offset += next;
                }
            } finally { Marshal.FreeHGlobal(buffer); }
        }
    }
    public static PROCESS_MEMORY_COUNTERS_EX Memory(IntPtr h) {
        PROCESS_MEMORY_COUNTERS_EX c; GetProcessMemoryInfo(h, out c, (uint)Marshal.SizeOf(typeof(PROCESS_MEMORY_COUNTERS_EX))); return c;
    }
}
}
'@
}

function Invoke-MeasuredProcess {
    param([Parameter(Mandatory)][string]$Executable, [string]$Arguments = '', [int]$TimeoutSeconds = 600, [int]$PollMilliseconds = 10,
        # Optional committed-memory cap applied right after launch (Job object).
        [long]$MemoryLimitBytes = 0,
        # Integrate CPU package power (RAPL 'Energy Meter' counter, milliwatts)
        # over the run. Package-wide, so only meaningful on a quiet machine.
        [switch]$MeasureEnergy,
        # Optional client started right after launch that drives the measured
        # process to finish (Everything's es.exe export, then exit). Its own
        # resources are not counted.
        [string]$CompanionExecutable = '', [string]$CompanionArguments = '')
    $power = $null; $energyJoules = 0.0; $lastPowerSample = $null
    if ($MeasureEnergy) {
        $power = [Diagnostics.PerformanceCounter]::new('Energy Meter', 'Power', 'rapl_package0_pkg', $true)
        $null = $power.NextValue()
    }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $options = @{ FilePath = $Executable; WindowStyle = 'Hidden'; PassThru = $true }
    if ($Arguments) { $options.ArgumentList = $Arguments }
    $process = Start-Process @options
    $handle = $process.Handle  # Keep the handle open so counters survive exit.
    $companion = if ($CompanionExecutable) { Start-Process -FilePath $CompanionExecutable -ArgumentList $CompanionArguments -WindowStyle Hidden -PassThru } else { $null }
    if ($MemoryLimitBytes -gt 0) { $null = [WinBlazeBench.ProcessMeasure]::LimitProcessMemory($handle, [uint64]$MemoryLimitBytes) }
    $peakHandles = 0; $peakThreads = 0; $nextThreadSample = 0.0
    $switchesPerThread = [Collections.Generic.Dictionary[long,long]]::new()
    while (-not $process.WaitForExit($PollMilliseconds)) {
        if ($timer.Elapsed.TotalSeconds -gt $TimeoutSeconds) {
            Stop-Process -Id $process.Id -Force
            # The whole tree: a cmd.exe companion's children (es.exe) outlive it otherwise.
            if ($companion -and !$companion.HasExited) { & taskkill.exe /PID $companion.Id /T /F 2>&1 | Out-Null }
            throw "$Executable timed out after $TimeoutSeconds s"
        }
        try {
            $count = [uint32]0
            if ([WinBlazeBench.ProcessMeasure]::GetProcessHandleCount($handle, [ref]$count)) { $peakHandles = [math]::Max($peakHandles, [int]$count) }
            if ($timer.Elapsed.TotalMilliseconds -ge $nextThreadSample) {
                $peakThreads = [math]::Max($peakThreads, [WinBlazeBench.ProcessMeasure]::ThreadCount([uint32]$process.Id))
                [WinBlazeBench.ProcessMeasure]::SampleContextSwitches($process.Id, $switchesPerThread)
                $nextThreadSample = $timer.Elapsed.TotalMilliseconds + 250
                if ($power) {
                    $now = $timer.Elapsed.TotalSeconds
                    $watts = $power.NextValue() / 1000.0
                    if ($lastPowerSample -ne $null) { $energyJoules += $watts * ($now - $lastPowerSample) }
                    $lastPowerSample = $now
                }
            }
        } catch { }
    }
    $wall = $timer.Elapsed.TotalMilliseconds
    $process.WaitForExit()
    $io = New-Object WinBlazeBench.ProcessMeasure+IO_COUNTERS
    [void][WinBlazeBench.ProcessMeasure]::GetProcessIoCounters($handle, [ref]$io)
    $memory = [WinBlazeBench.ProcessMeasure]::Memory($handle)
    $create = 0L; $exit = 0L; $kernel = 0L; $user = 0L
    [void][WinBlazeBench.ProcessMeasure]::GetProcessTimes($handle, [ref]$create, [ref]$exit, [ref]$kernel, [ref]$user)
    [pscustomobject]@{
        exit_code = $process.ExitCode
        elapsed_ms = [math]::Round(($exit - $create) / 10000.0, 2)
        wall_ms = [math]::Round($wall, 2)
        kernel_cpu_ms = [math]::Round($kernel / 10000.0, 2)
        user_cpu_ms = [math]::Round($user / 10000.0, 2)
        read_bytes = [long]$io.ReadTransferCount
        write_bytes = [long]$io.WriteTransferCount
        read_operations = [long]$io.ReadOperationCount
        write_operations = [long]$io.WriteOperationCount
        peak_working_set_bytes = [long]$memory.PeakWorkingSetSize.ToUInt64()
        peak_commit_bytes = [long]$memory.PeakPagefileUsage.ToUInt64()
        page_faults = [long]$memory.PageFaultCount
        peak_handles_sampled = $peakHandles
        peak_threads_sampled = $peakThreads
        package_energy_joules = if ($power) { [math]::Round($energyJoules, 2) } else { $null }
        # Lower bound: threads that start and exit between 250 ms samples are missed.
        context_switches_sampled = [long]($switchesPerThread.Values | Measure-Object -Sum).Sum
    }
}
