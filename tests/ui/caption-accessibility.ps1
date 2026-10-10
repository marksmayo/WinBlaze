param([Parameter(Mandatory)][string]$AppPath)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName Accessibility
Add-Type -ReferencedAssemblies Accessibility -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using Accessibility;
public sealed class OwnedCaptionAccessibility {
    private delegate bool Enumerate(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(Enumerate callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr window);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("oleacc.dll")] private static extern int AccessibleObjectFromWindow(IntPtr window, uint objectId, ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IAccessible accessible);
    [DllImport("oleacc.dll")] private static extern int AccessibleChildren(IAccessible accessible, int first, int count,
        [Out, MarshalAs(UnmanagedType.LPArray, ArraySubType=UnmanagedType.Struct, SizeParamIndex=2)] object[] children, out int obtained);
    public sealed class Child {
        public int Id, Role, State;
        public string Name, DefaultAction;
    }
    private readonly IntPtr window;
    private readonly int process;
    private readonly IAccessible titlebar;
    public static IntPtr Find(int process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            var name = new StringBuilder(256); GetWindowText(window, name, name.Capacity);
            if (owner == process && IsWindowVisible(window) && name.ToString() == "WinBlaze") { result = window; return false; }
            return true;
        }, IntPtr.Zero);
        return result;
    }
    private void CheckOwner() {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (owner != process) throw new Exception("Caption target owner mismatch");
    }
    public OwnedCaptionAccessibility(IntPtr window, int process) {
        this.window = window; this.process = process; CheckOwner();
        var iid = typeof(IAccessible).GUID;
        Marshal.ThrowExceptionForHR(AccessibleObjectFromWindow(window, 0xFFFFFFFE, ref iid, out titlebar));
        if (titlebar == null || Convert.ToInt32(titlebar.get_accRole(0)) != 1) throw new Exception("Native titlebar accessibility object missing");
    }
    public Child[] Capture() {
        CheckOwner();
        var children = new List<Child>();
        var ids = new object[titlebar.accChildCount];
        int obtained;
        Marshal.ThrowExceptionForHR(AccessibleChildren(titlebar, 0, ids.Length, ids, out obtained));
        for (int index = 0; index < obtained; ++index) {
            if (!(ids[index] is int)) throw new Exception("Expected native simple titlebar child identifier");
            int id = (int)ids[index];
            children.Add(new Child { Id=id, Role=Convert.ToInt32(titlebar.get_accRole(id)),
                State=Convert.ToInt32(titlebar.get_accState(id)), Name=ReadOptional(() => titlebar.get_accName(id)),
                DefaultAction=ReadOptional(() => titlebar.get_accDefaultAction(id)) });
        }
        return children.ToArray();
    }
    private static string ReadOptional(Func<string> read) {
        try { return read(); }
        catch (COMException error) {
            if (error.ErrorCode != unchecked((int)0x80020003)) throw;
            return null; // Hidden system slots need not expose optional name/action properties.
        }
    }
    public void Invoke(int childId) {
        CheckOwner();
        foreach (var child in Capture()) {
            if (child.Id == childId && child.Role == 43 && (child.State & 0x8001) == 0 &&
                !String.IsNullOrWhiteSpace(child.Name) && !String.IsNullOrWhiteSpace(child.DefaultAction)) {
                titlebar.accDoDefaultAction(child.Id); return;
            }
        }
        throw new Exception("Available named accessible caption action missing: " + childId);
    }
    public void Restore() {
        CheckOwner();
        if (!PostMessage(window, 0x0112, new IntPtr(0xF120), IntPtr.Zero)) throw new Exception("Owned restore rejected");
    }
}
'@
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$work = Join-Path $repo ('artifacts/ui-caption-accessibility-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work -Force | Out-Null
$resolved = (Resolve-Path -LiteralPath $AppPath).Path
$start = [Diagnostics.ProcessStartInfo]::new($resolved)
$start.UseShellExecute = $false
$start.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $work 'appdata'
$app = [Diagnostics.Process]::Start($start)
$result = [ordered]@{binary_sha256=(Get-FileHash -LiteralPath $resolved).Hash;passed=$false
    scope='MSAA native titlebar roles/names/default actions and owned window state; not UIA/Narrator coverage'}
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $owner = [OwnedCaptionAccessibility]::Find($app.Id)
        if ($owner -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($owner -eq [IntPtr]::Zero) { throw 'Owned visible caption window not found' }
    $caption = [OwnedCaptionAccessibility]::new($owner, $app.Id)
    $result.children = @($caption.Capture())
    $caption.Restore()
    Start-Sleep -Milliseconds 300
    $caption.Invoke(2)
    Start-Sleep -Milliseconds 300
    if (![OwnedCaptionAccessibility]::IsIconic($owner)) { throw 'Accessible Minimize did not minimize the owned window' }
    $caption.Restore()
    Start-Sleep -Milliseconds 300
    $caption.Invoke(3)
    Start-Sleep -Milliseconds 300
    if (![OwnedCaptionAccessibility]::IsZoomed($owner)) { throw 'Accessible Maximize did not maximize the owned window' }
    $caption.Invoke(3)
    Start-Sleep -Milliseconds 300
    if ([OwnedCaptionAccessibility]::IsZoomed($owner)) { throw 'Accessible Restore did not restore the owned window' }
    $caption.Invoke(5)
    if (!$app.WaitForExit(15000) -or $app.ExitCode -ne 0) { throw 'Accessible Close did not produce normal shutdown' }
    $result.exit_code = $app.ExitCode
    $result.passed = $true
    Write-Output "PASS: accessible native caption actions minimize/maximize/restore/close owned window; normal exit; evidence=$work"
} catch {
    $result.failure = $_.Exception.Message
    throw
} finally {
    $result | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $work 'result.json')
    if (!$app.HasExited) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
}
