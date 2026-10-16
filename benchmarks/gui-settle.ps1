# Dot-source: visual settle timing for GUI events that have no product-specific
# "done drawing" signal (EXTENDED_MEDALS_PLAN.md #53). Captures the window's screen
# pixels (per-monitor DPI aware, so physical pixels) and reports when they last
# changed before a stable period.
if (-not ('WinBlazeBench.GuiSettle' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;
namespace WinBlazeBench {
public static class GuiSettle {
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr hwnd);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr hwnd, IntPtr dc);
    [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr dc);
    [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr dc);
    [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr obj);
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr dest, int x, int y, int w, int h, IntPtr src, int sx, int sy, uint rop);
    [DllImport("gdi32.dll")] static extern IntPtr CreateDIBSection(IntPtr dc, ref BITMAPINFOHEADER info, uint usage, out IntPtr bits, IntPtr section, uint offset);
    [StructLayout(LayoutKind.Sequential)] struct BITMAPINFOHEADER {
        public int biSize, biWidth, biHeight; public short biPlanes, biBitCount; public int biCompression, biSizeImage, biXPelsPerMeter, biYPelsPerMeter, biClrUsed, biClrImportant;
    }
    static readonly IntPtr TOPMOST = new IntPtr(-1), NOTOPMOST = new IntPtr(-2);
    const uint SWP_NOACTIVATE = 0x0010, SWP_SHOWWINDOW = 0x0040, SRCCOPY = 0x00CC0020, CAPTUREBLT = 0x40000000;

    static void Aware() { SetThreadDpiAwarenessContext(new IntPtr(-4)); } // PER_MONITOR_AWARE_V2

    public static void Place(IntPtr hwnd, int x, int y, int w, int h) {
        Aware();
        ShowWindow(hwnd, 9); // SW_RESTORE
        SetForegroundWindow(hwnd);
        SetWindowPos(hwnd, TOPMOST, x, y, w, h, SWP_SHOWWINDOW);
    }
    public static void Release(IntPtr hwnd) { SetWindowPos(hwnd, NOTOPMOST, 0, 0, 0, 0, 0x0001 | 0x0002 | SWP_NOACTIVATE); }

    static ulong Frame(IntPtr screen, IntPtr memory, IntPtr bits, int x, int y, int w, int h, int[] buffer) {
        BitBlt(memory, 0, 0, w, h, screen, x, y, SRCCOPY | CAPTUREBLT);
        Marshal.Copy(bits, buffer, 0, buffer.Length);
        ulong hash = 14695981039346656037UL;
        for (int i = 0; i < buffer.Length; i++) { hash ^= (uint)buffer[i]; hash *= 1099511628211UL; }
        return hash;
    }
    // Milliseconds from $action until the region (x, y, w, h) last changed before
    // stableMs without change; -1 on timeout. The action runs after the first frame.
    public static double MeasureSettle(Action action, int x, int y, int w, int h, int stableMs, int timeoutMs) {
        Aware();
        IntPtr screen = GetDC(IntPtr.Zero), memory = CreateCompatibleDC(screen), bits;
        var info = new BITMAPINFOHEADER { biSize = 40, biWidth = w, biHeight = -h, biPlanes = 1, biBitCount = 32 };
        IntPtr bitmap = CreateDIBSection(screen, ref info, 0, out bits, IntPtr.Zero, 0);
        IntPtr old = SelectObject(memory, bitmap);
        var buffer = new int[w * h];
        try {
            ulong last = Frame(screen, memory, bits, x, y, w, h, buffer);
            var watch = Stopwatch.StartNew();
            action();
            double changed = 0;
            while (watch.ElapsedMilliseconds < timeoutMs) {
                ulong now = Frame(screen, memory, bits, x, y, w, h, buffer);
                double t = watch.Elapsed.TotalMilliseconds;
                if (now != last) { last = now; changed = t; }
                else if (t - changed >= stableMs) return changed;
                Thread.Sleep(1);
            }
            return -1;
        } finally {
            SelectObject(memory, old); DeleteObject(bitmap); DeleteDC(memory); ReleaseDC(IntPtr.Zero, screen);
        }
    }
    public static double MeasureResize(IntPtr hwnd, int x, int y, int fromW, int fromH, int toW, int toH, int stableMs, int timeoutMs) {
        return MeasureSettle(() => SetWindowPos(hwnd, TOPMOST, x, y, toW, toH, SWP_NOACTIVATE), x, y, Math.Max(fromW, toW), Math.Max(fromH, toH), stableMs, timeoutMs);
    }
}
}
'@
}
