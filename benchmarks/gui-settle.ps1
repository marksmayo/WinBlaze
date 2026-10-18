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
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr hwnd, int attribute, out RECT value, int size);
    [StructLayout(LayoutKind.Sequential)] struct RECT { public int Left, Top, Right, Bottom; }
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
        // Park the pointer outside every measured region: a pointer left over
        // the window makes hover visuals repaint at arbitrary times after a
        // resize, which reads as late redraw for whichever product is under it.
        SetCursorPos(0, 0);
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
    // Invisible resize-border widths (window rect minus DWM's visible frame).
    // Pixels there show whatever is behind the window (a blinking caret, a
    // clock), which is not the product redrawing, so captures exclude them.
    static void FrameInsets(IntPtr hwnd, out int left, out int top, out int right, out int bottom) {
        left = top = right = bottom = 0;
        RECT window, frame;
        if (!GetWindowRect(hwnd, out window)) return;
        if (DwmGetWindowAttribute(hwnd, 9, out frame, Marshal.SizeOf(typeof(RECT))) != 0) return; // DWMWA_EXTENDED_FRAME_BOUNDS
        left = Math.Max(0, frame.Left - window.Left); top = Math.Max(0, frame.Top - window.Top);
        right = Math.Max(0, window.Right - frame.Right); bottom = Math.Max(0, window.Bottom - frame.Bottom);
    }
    // The window's visible area at (x, y, w, h) settles (no-op action).
    public static double SettleWindow(IntPtr hwnd, int x, int y, int w, int h, int stableMs, int timeoutMs) {
        Aware();
        int l, t, r, b; FrameInsets(hwnd, out l, out t, out r, out b);
        return MeasureSettle(() => { }, x + l, y + t, w - l - r, h - t - b, stableMs, timeoutMs);
    }
    public static double MeasureResize(IntPtr hwnd, int x, int y, int fromW, int fromH, int toW, int toH, int stableMs, int timeoutMs) {
        Aware();
        int l, t, r, b; FrameInsets(hwnd, out l, out t, out r, out b);
        return MeasureSettle(() => SetWindowPos(hwnd, TOPMOST, x, y, toW, toH, SWP_NOACTIVATE), x + l, y + t, Math.Max(fromW, toW) - l - r, Math.Max(fromH, toH) - t - b, stableMs, timeoutMs);
    }
}
}
'@
}
