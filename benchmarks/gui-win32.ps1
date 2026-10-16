# Dot-source: Win32 window inspection for GUIs that expose no UI Automation text
# (WinDirStat's MFC and WizTree's VCL status bars). Reads child window text and
# status-bar panes, including the cross-process SB_GETTEXT buffer.
if (-not ('WinBlazeBench.Win32Window' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace WinBlazeBench {
public static class Win32Window {
    delegate bool EnumProc(IntPtr hwnd, IntPtr lparam);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc proc, IntPtr lparam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder name, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, StringBuilder lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern IntPtr VirtualAllocEx(IntPtr process, IntPtr address, UIntPtr size, uint type, uint protect);
    [DllImport("kernel32.dll")] static extern bool VirtualFreeEx(IntPtr process, IntPtr address, UIntPtr size, uint type);
    [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr process, IntPtr address, byte[] buffer, UIntPtr size, out UIntPtr read);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    const uint WM_GETTEXT = 0x000D, SB_GETPARTS = 0x0406, SB_GETTEXTW = 0x040D, SMTO_ABORTIFHUNG = 0x0002;

    public static string Text(IntPtr hwnd) {
        var text = new StringBuilder(1024); IntPtr result;
        SendMessageTimeout(hwnd, WM_GETTEXT, (IntPtr)text.Capacity, text, SMTO_ABORTIFHUNG, 500, out result);
        return text.ToString();
    }
    // Toolbar command ids (TB_BUTTONCOUNT + TB_GETBUTTON through a remote TBBUTTON buffer).
    public static int[] ToolbarCommands(IntPtr toolbar) {
        IntPtr r;
        SendMessageTimeout(toolbar, 0x0418 /* TB_BUTTONCOUNT */, IntPtr.Zero, IntPtr.Zero, SMTO_ABORTIFHUNG, 500, out r);
        int count = (int)r.ToInt64();
        uint pid; GetWindowThreadProcessId(toolbar, out pid);
        IntPtr process = OpenProcess(0x0008 | 0x0010 | 0x0020, false, pid);
        var ids = new List<int>();
        if (process == IntPtr.Zero) return ids.ToArray();
        IntPtr remote = VirtualAllocEx(process, IntPtr.Zero, (UIntPtr)64, 0x3000, 0x04);
        try {
            for (int i = 0; i < count; i++) {
                SendMessageTimeout(toolbar, 0x0417 /* TB_GETBUTTON */, (IntPtr)i, remote, SMTO_ABORTIFHUNG, 500, out r);
                var bytes = new byte[32]; UIntPtr read;
                ReadProcessMemory(process, remote, bytes, (UIntPtr)32, out read);
                ids.Add(BitConverter.ToInt32(bytes, 4)); // TBBUTTON.idCommand follows iBitmap.
            }
        } finally { VirtualFreeEx(process, remote, UIntPtr.Zero, 0x8000); CloseHandle(process); }
        return ids.ToArray();
    }
    // LVM_GETITEMCOUNT needs no buffer, so it works across processes.
    public static int ListItems(IntPtr hwnd) { IntPtr r; SendMessageTimeout(hwnd, 0x1004, IntPtr.Zero, IntPtr.Zero, SMTO_ABORTIFHUNG, 500, out r); return (int)r.ToInt64(); }
    public static IntPtr[] ChildrenOfClass(IntPtr top, string cls) {
        var found = new List<IntPtr>();
        EnumChildWindows(top, (hwnd, _) => { if (ClassOf(hwnd) == cls) found.Add(hwnd); return true; }, IntPtr.Zero);
        return found.ToArray();
    }
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc proc, IntPtr lparam);
    // Visible top-level window of a class owned by a process (VCL apps report
    // their hidden TApplication window as the process main window).
    public static IntPtr TopLevelOfClass(int pid, string cls) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, _) => {
            uint owner; GetWindowThreadProcessId(hwnd, out owner);
            if (owner == (uint)pid && IsWindowVisible(hwnd) && ClassOf(hwnd) == cls) { found = hwnd; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static string ClassOf(IntPtr hwnd) { var name = new StringBuilder(256); GetClassName(hwnd, name, name.Capacity); return name.ToString(); }
    [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr hwnd, uint cmd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    // A top-level window's caption without messaging its thread (GetWindowText
    // reads another process's cached caption; WM_GETTEXT would make the
    // measured UI thread answer every poll).
    public static string Title(IntPtr hwnd) { var text = new StringBuilder(1024); GetWindowText(hwnd, text, text.Capacity); return text.ToString(); }
    // A process's application window: .NET's MainWindowHandle rule (visible,
    // unowned, top-level) minus tooltip/IME windows, which it can return while
    // a tooltip is showing; titled windows only.
    public static IntPtr AppWindow(int pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, _) => {
            uint owner; GetWindowThreadProcessId(hwnd, out owner);
            if (owner != (uint)pid || !IsWindowVisible(hwnd) || GetWindow(hwnd, 4 /* GW_OWNER */) != IntPtr.Zero) return true;
            string cls = ClassOf(hwnd);
            if (cls == "MicrosoftWindowsTooltip" || cls == "tooltips_class32" || cls == "IME" || cls == "MSCTFIME UI") return true;
            if (Title(hwnd).Length == 0) return true;
            found = hwnd; return false;
        }, IntPtr.Zero);
        return found;
    }

    // Status-bar parts live in the target process, so SB_GETTEXTW needs a remote buffer.
    public static string[] StatusParts(IntPtr hwnd) {
        IntPtr result;
        if (SendMessageTimeout(hwnd, SB_GETPARTS, IntPtr.Zero, IntPtr.Zero, SMTO_ABORTIFHUNG, 500, out result) == IntPtr.Zero) return new string[0];
        int parts = (int)result.ToInt64();
        uint pid; GetWindowThreadProcessId(hwnd, out pid);
        IntPtr process = OpenProcess(0x0008 | 0x0010 | 0x0020, false, pid); // VM_OPERATION | VM_READ | VM_WRITE
        if (process == IntPtr.Zero) return new string[0];
        var texts = new List<string>();
        IntPtr remote = VirtualAllocEx(process, IntPtr.Zero, (UIntPtr)4096, 0x3000, 0x04);
        try {
            for (int i = 0; i < parts; i++) {
                SendMessageTimeout(hwnd, SB_GETTEXTW, (IntPtr)i, remote, SMTO_ABORTIFHUNG, 500, out result);
                int chars = Math.Min((int)(result.ToInt64() & 0xFFFF), 2047);
                var bytes = new byte[chars * 2]; UIntPtr read;
                ReadProcessMemory(process, remote, bytes, (UIntPtr)bytes.Length, out read);
                texts.Add(Encoding.Unicode.GetString(bytes));
            }
        } finally { VirtualFreeEx(process, remote, UIntPtr.Zero, 0x8000); CloseHandle(process); }
        return texts.ToArray();
    }

    // Visible descendants with their class and text; status bars report their parts.
    public static string[] Describe(IntPtr top) { return Describe(top, false); }
    public static string[] Describe(IntPtr top, bool includeEmpty) {
        var lines = new List<string>();
        EnumChildWindows(top, (hwnd, _) => {
            if (!IsWindowVisible(hwnd)) return true;
            if (includeEmpty) { string c = ClassOf(hwnd); string t = c.IndexOf("statusbar", StringComparison.OrdinalIgnoreCase) >= 0 ? string.Join(" | ", StatusParts(hwnd)) : Text(hwnd); lines.Add(c + ": " + t); return true; }
            string cls = ClassOf(hwnd);
            string text = cls.IndexOf("statusbar", StringComparison.OrdinalIgnoreCase) >= 0 ? string.Join(" | ", StatusParts(hwnd)) : Text(hwnd);
            if (text.Length > 0) lines.Add(cls + ": " + text);
            return true;
        }, IntPtr.Zero);
        return lines.ToArray();
    }
}
}
'@
}

# The process's application window and its title (see AppWindow); falls back
# to MainWindowHandle before the first titled window exists.
function Get-AppWindow($process) {
    $hwnd = [WinBlazeBench.Win32Window]::AppWindow($process.Id)
    if ($hwnd -eq [IntPtr]::Zero) { $process.Refresh(); $hwnd = $process.MainWindowHandle }
    # An exited process has no MainWindowHandle ($null): report no window.
    if ($null -eq $hwnd) { return [IntPtr]::Zero }
    return $hwnd
}
function Get-AppTitle($process) {
    $hwnd = Get-AppWindow $process
    if ($hwnd -eq [IntPtr]::Zero) { return '' }
    return [WinBlazeBench.Win32Window]::Title($hwnd)
}
