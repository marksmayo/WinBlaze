// Companion for GUI-only products in whole-process benchmarks: waits until a
// window of the given class (owned by a process with the given image name)
// shows a status-bar part matching a pattern, writes the window caption and
// all status-bar parts to a file (for validation), then closes the window so
// the measured process exits on its own.
// Usage: gui-ready-closer.exe <timeout-s> <process-name> <window-class> <ready-regex> <output-file>
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;

static class GuiReadyCloser
{
    delegate bool EnumProc(IntPtr hwnd, IntPtr lparam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc proc, IntPtr lparam);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc proc, IntPtr lparam);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder name, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern IntPtr VirtualAllocEx(IntPtr process, IntPtr address, UIntPtr size, uint type, uint protect);
    [DllImport("kernel32.dll")] static extern bool VirtualFreeEx(IntPtr process, IntPtr address, UIntPtr size, uint type);
    [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr process, IntPtr address, byte[] buffer, UIntPtr size, out UIntPtr read);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);

    static string ClassOf(IntPtr hwnd) { var s = new StringBuilder(256); GetClassName(hwnd, s, s.Capacity); return s.ToString(); }

    // Status-bar parts live in the target process: SB_GETTEXTW needs a remote buffer.
    static List<string> StatusParts(IntPtr bar, uint pid) {
        var texts = new List<string>(); IntPtr result;
        if (SendMessageTimeout(bar, 0x0406 /* SB_GETPARTS */, IntPtr.Zero, IntPtr.Zero, 2, 500, out result) == IntPtr.Zero) return texts;
        int parts = (int)result.ToInt64();
        IntPtr process = OpenProcess(0x0008 | 0x0010 | 0x0020, false, pid);
        if (process == IntPtr.Zero) return texts;
        IntPtr remote = VirtualAllocEx(process, IntPtr.Zero, (UIntPtr)4096, 0x3000, 0x04);
        try {
            for (int i = 0; i < parts; i++) {
                SendMessageTimeout(bar, 0x040D /* SB_GETTEXTW */, (IntPtr)i, remote, 2, 500, out result);
                int chars = Math.Min((int)(result.ToInt64() & 0xFFFF), 2047);
                var bytes = new byte[chars * 2]; UIntPtr read;
                ReadProcessMemory(process, remote, bytes, (UIntPtr)bytes.Length, out read);
                texts.Add(Encoding.Unicode.GetString(bytes));
            }
        } finally { VirtualFreeEx(process, remote, UIntPtr.Zero, 0x8000); CloseHandle(process); }
        return texts;
    }

    static int Main(string[] args)
    {
        if (args.Length < 5) { return 2; }
        var deadline = DateTime.UtcNow.AddSeconds(double.Parse(args[0], System.Globalization.CultureInfo.InvariantCulture));
        string processName = args[1], windowClass = args[2], output = args[4];
        var ready = new Regex(args[3]);
        while (DateTime.UtcNow < deadline) {
            var pids = new HashSet<uint>();
            foreach (var p in Process.GetProcessesByName(processName)) { pids.Add((uint)p.Id); p.Dispose(); }
            IntPtr form = IntPtr.Zero; uint formPid = 0;
            EnumWindows((hwnd, _) => {
                uint pid; GetWindowThreadProcessId(hwnd, out pid);
                if (pids.Contains(pid) && IsWindowVisible(hwnd) && ClassOf(hwnd) == windowClass) { form = hwnd; formPid = pid; return false; }
                return true;
            }, IntPtr.Zero);
            if (form != IntPtr.Zero) {
                var parts = new List<string>();
                EnumChildWindows(form, (child, _) => {
                    if (ClassOf(child).IndexOf("StatusBar", StringComparison.OrdinalIgnoreCase) >= 0) parts.AddRange(StatusParts(child, formPid));
                    return true;
                }, IntPtr.Zero);
                if (parts.Exists(part => ready.IsMatch(part))) {
                    var caption = new StringBuilder(1024); GetWindowText(form, caption, caption.Capacity);
                    File.WriteAllLines(output, new[] { caption.ToString(), string.Join(" | ", parts) }, new UTF8Encoding(false));
                    PostMessage(form, 0x0010 /* WM_CLOSE */, IntPtr.Zero, IntPtr.Zero);
                    return 0;
                }
            }
            Thread.Sleep(20);
        }
        return 1;
    }
}
