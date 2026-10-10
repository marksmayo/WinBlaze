param(
    [string]$AppPath = (Join-Path $PSScriptRoot '../../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'),
    [string]$ReferenceIconAppPath = ''
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
public static class OwnedWindowIcons {
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll", EntryPoint="LoadImageW")] private static extern IntPtr LoadResourceIcon(IntPtr instance, IntPtr resource, uint type, int width, int height, uint flags);
    [DllImport("user32.dll", EntryPoint="LoadImageW", CharSet=CharSet.Unicode)] private static extern IntPtr LoadFileIcon(IntPtr instance, string path, uint type, int width, int height, uint flags);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll")] private static extern bool FreeLibrary(IntPtr module);
    [DllImport("user32.dll")] private static extern bool DestroyIcon(IntPtr icon);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    private static void CheckOwner(IntPtr window, int process) {
        uint owner;
        GetWindowThreadProcessId(window, out owner);
        if (owner != process) throw new Exception("Icon target owner mismatch");
    }
    private static string Pixels(IntPtr handle) {
        using (var icon = Icon.FromHandle(handle))
        using (var bitmap = icon.ToBitmap())
        using (var bytes = new MemoryStream())
        using (var hash = SHA256.Create()) {
            bitmap.Save(bytes, System.Drawing.Imaging.ImageFormat.Png);
            return BitConverter.ToString(hash.ComputeHash(bytes.ToArray()));
        }
    }
    public static string VerifyAsset(string actualPath, string referencePath, int size) {
        var actual = LoadFileIcon(IntPtr.Zero, actualPath, 1, size, size, 0x0010);
        var reference = LoadFileIcon(IntPtr.Zero, referencePath, 1, size, size, 0x0010);
        try {
            if (actual == IntPtr.Zero || reference == IntPtr.Zero) throw new Exception("Icon asset load failed");
            var pixels = Pixels(actual);
            if (pixels != Pixels(reference)) throw new Exception("Icon asset pixels changed at size " + size);
            return pixels;
        } finally {
            if (actual != IntPtr.Zero) DestroyIcon(actual);
            if (reference != IntPtr.Zero) DestroyIcon(reference);
        }
    }
    public static string Verify(IntPtr window, int process, string path, bool small) {
        CheckOwner(window, process);
        var actual = SendMessage(window, 0x007F, new IntPtr(small ? 0 : 1), IntPtr.Zero);
        if (actual == IntPtr.Zero) throw new Exception(small ? "Small window icon missing" : "Taskbar window icon missing");
        int width, height;
        using (var icon = Icon.FromHandle(actual)) { width = icon.Width; height = icon.Height; }
        var module = LoadLibraryEx(path, IntPtr.Zero, 0x00000002);
        if (module == IntPtr.Zero) throw new Exception("Executable resource module could not load");
        var expected = LoadResourceIcon(module, new IntPtr(1), 1, width, height, 0);
        if (expected == IntPtr.Zero) { FreeLibrary(module); throw new Exception("Embedded WinBlaze icon could not load"); }
        try {
            var pixels = Pixels(actual);
            if (pixels != Pixels(expected)) throw new Exception("Window icon pixels differ from its embedded WinBlaze resource");
            return pixels;
        } finally { DestroyIcon(expected); FreeLibrary(module); }
    }
    public static void Close(IntPtr window, int process) {
        CheckOwner(window, process);
        if (!PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero)) throw new Exception("Owned close failed");
    }
}
'@
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$work = Join-Path $repo ('artifacts/ui-icons-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$start = [Diagnostics.ProcessStartInfo]::new((Resolve-Path $AppPath).Path)
$start.UseShellExecute = $false
$start.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $work 'appdata'
$app = [Diagnostics.Process]::Start($start)
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    $ownedWindow = [System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$app.Id),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'WinBlaze'))
    do {
        $element = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children,$ownedWindow)
        if ($element) {
            $scan = $element.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Start scan'))
            if ($scan -and $scan.Current.IsEnabled) {
                $window = [IntPtr]$element.Current.NativeWindowHandle
                break
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if (!$window -or $window -eq [IntPtr]::Zero) { throw 'Owned initialized main window missing' }
    # OnLaunched finishes assigning icons after activation.
    Start-Sleep -Milliseconds 500
    $asset = (Resolve-Path $(if ($ReferenceIconAppPath) { $ReferenceIconAppPath } else { $AppPath })).Path
    $small = [OwnedWindowIcons]::Verify($window, $app.Id, $asset, $true)
    $large = [OwnedWindowIcons]::Verify($window, $app.Id, $asset, $false)
    [OwnedWindowIcons]::Close($window, $app.Id)
    if (!$app.WaitForExit(10000) -or $app.ExitCode -ne 0) { throw 'Icon check did not close normally' }
    [ordered]@{binary_sha256=(Get-FileHash $AppPath).Hash;icon_reference_sha256=(Get-FileHash $asset).Hash;small_pixels_sha256=$small;large_pixels_sha256=$large;normal_exit=0} |
        ConvertTo-Json | Set-Content (Join-Path $work 'result.json')
    Write-Output "PASS: small and taskbar icons match embedded WinBlaze resource pixels; normal exit; evidence=$work"
} catch {
    [ordered]@{error=$_.Exception.Message;binary_sha256=(Get-FileHash $AppPath).Hash} |
        ConvertTo-Json | Set-Content (Join-Path $work 'failure.json')
    throw
} finally {
    if (!$app.HasExited) { $app.Kill(); $app.WaitForExit() }
    $app.Dispose()
}
