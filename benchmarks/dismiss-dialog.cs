// Companion for unattended benchmark runs: clicks a named button on any
// top-level window with a given title (for example a trial notice that every
// launch shows) and keeps watching until killed or the timeout elapses.
// Usage: dismiss-dialog.exe <timeout-seconds> "<window title>|<button text>" ...
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

static class DismissDialog
{
    delegate bool EnumProc(IntPtr hwnd, IntPtr lparam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc proc, IntPtr lparam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);

    static int Main(string[] args)
    {
        if (args.Length < 2) { return 2; }
        var deadline = DateTime.UtcNow.AddSeconds(double.Parse(args[0], System.Globalization.CultureInfo.InvariantCulture));
        var rules = new List<string[]>();
        for (int i = 1; i < args.Length; i++) { rules.Add(args[i].Split('|')); }
        var clicked = new HashSet<IntPtr>();
        while (DateTime.UtcNow < deadline) {
            foreach (var rule in rules) {
                IntPtr window = FindWindow("#32770", rule[0]);
                if (window == IntPtr.Zero || clicked.Contains(window)) { continue; }
                IntPtr button = IntPtr.Zero;
                EnumChildWindows(window, (child, _) => {
                    var text = new StringBuilder(256);
                    GetWindowText(child, text, text.Capacity);
                    if (text.ToString() == rule[1]) { button = child; return false; }
                    return true;
                }, IntPtr.Zero);
                if (button != IntPtr.Zero) {
                    PostMessage(button, 0x00F5 /* BM_CLICK */, IntPtr.Zero, IntPtr.Zero);
                    clicked.Add(window);
                }
            }
            Thread.Sleep(10);
        }
        return 0;
    }
}
