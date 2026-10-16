# Dot-source: read a window's menu bar (text and command ids) across processes,
# and invoke a menu command with WM_COMMAND. Used to drive WinDirStat's GUI.
if (-not ('WinBlazeBench.GuiMenu' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace WinBlazeBench {
public static class GuiMenu {
    [DllImport("user32.dll")] static extern IntPtr GetMenu(IntPtr hwnd);
    [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] static extern uint GetMenuItemID(IntPtr menu, int pos);
    [DllImport("user32.dll")] static extern IntPtr GetSubMenu(IntPtr menu, int pos);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int max, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
    public static string[] Dump(IntPtr hwnd) {
        var lines = new List<string>();
        Walk(GetMenu(hwnd), "", lines);
        return lines.ToArray();
    }
    static void Walk(IntPtr menu, string prefix, List<string> lines) {
        if (menu == IntPtr.Zero) return;
        int count = GetMenuItemCount(menu);
        for (int i = 0; i < count; i++) {
            var text = new StringBuilder(256);
            GetMenuString(menu, (uint)i, text, text.Capacity, 0x400 /* MF_BYPOSITION */);
            IntPtr sub = GetSubMenu(menu, i);
            if (sub != IntPtr.Zero) Walk(sub, prefix + text + " > ", lines);
            else if (text.Length > 0) lines.Add(prefix + text + " = " + GetMenuItemID(menu, i));
        }
    }
    // Finds the command id whose full menu path matches (case-insensitive substring).
    public static int Find(IntPtr hwnd, string pathContains) {
        foreach (var line in Dump(hwnd)) {
            int eq = line.LastIndexOf(" = ");
            if (line.Substring(0, eq).Replace("&", "").IndexOf(pathContains, StringComparison.OrdinalIgnoreCase) >= 0)
                return int.Parse(line.Substring(eq + 3));
        }
        return -1;
    }
    public static bool Invoke(IntPtr hwnd, int commandId) { return PostMessage(hwnd, 0x0111 /* WM_COMMAND */, (IntPtr)commandId, IntPtr.Zero); }
}
}
'@
}
