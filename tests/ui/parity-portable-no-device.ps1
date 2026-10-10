param(
    [string]$AppPath = (Join-Path $PSScriptRoot '../../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'),
    [ValidateRange(1,10)][int]$Repetitions = 3,
    [switch]$WithCatalog
)
# No discoverable devices required. Attached-device parity remains separate.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -ReferencedAssemblies UIAutomationClient,UIAutomationTypes -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows.Automation;
public static class ParityInvocation {
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    public static Task Invoke(AutomationElement element) {
        return Task.Factory.StartNew(() => ((InvokePattern)element.GetCurrentPattern(InvokePattern.Pattern)).Invoke());
    }
    public static void CloseOwnedHost(int handle, int process) {
        var window = new IntPtr(handle);
        uint owner;
        GetWindowThreadProcessId(window, out owner);
        if (owner != process || GetAncestor(window, 2) != window)
            throw new Exception("Close target is not this process's top-level window");
        if (!PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero))
            throw new Exception("Owned close delivery failed");
    }
}
'@
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$work = Join-Path $repo ('artifacts/ui-parity-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$fixture = Join-Path $work 'Folder with spaces'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'example.txt') -Value 'fixture'
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = (Resolve-Path -LiteralPath $AppPath).Path
$start.UseShellExecute = $false
$start.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $work 'appdata'
$app = [Diagnostics.Process]::Start($start)
function Find-Control {
    param([string]$Name)
    $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {
        $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
        if($element){break}
        Start-Sleep -Milliseconds 100
    } while([DateTime]::UtcNow -lt $deadline)
    if (!$element) { throw "Control not found: $Name" }
    return $element
}
function Open-OwnedMenuItem([string]$Name) {
    $ownedName=[System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$app.Id),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$Name))
    $condition=[System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$app.Id),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$Name),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::IsOffscreenProperty,$false),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::IsEnabledProperty,$true))
    $menu=Find-Control 'File menu'
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    $lastInvoke=[DateTime]::MinValue
    $attempts=0
    do {
        # XAML flyouts can be separate owned UIA roots. Never search or act on
        # another process, and retry only this app's direct menu invocation.
        $item=[System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
        if($item) {
            $hostItem=$window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
            [ordered]@{name=$Name;process_id=$app.Id;open_attempts=$attempts;visible=$true;enabled=$true;within_host_tree=!!$hostItem}|ConvertTo-Json|Set-Content (Join-Path $work 'menu-open-evidence.json')
            return $item
        }
        $hiddenItem=[System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$ownedName)
        if($hiddenItem) {
            [ordered]@{name=$Name;process_id=$app.Id;offscreen=$hiddenItem.Current.IsOffscreen;enabled=$hiddenItem.Current.IsEnabled;bounds=$hiddenItem.Current.BoundingRectangle.ToString();patterns=@($hiddenItem.GetSupportedPatterns() | ForEach-Object ProgrammaticName)}|ConvertTo-Json|Set-Content (Join-Path $work 'menu-hidden-evidence.json')
            $scrollItem=$null
            if($hiddenItem.TryGetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern,[ref]$scrollItem)) {
                $scrollItem.ScrollIntoView()
                Start-Sleep -Milliseconds 50
                continue
            }
        }
        if(([DateTime]::UtcNow-$lastInvoke).TotalMilliseconds -ge 500) {
            $menu.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
            $lastInvoke=[DateTime]::UtcNow
            $attempts++
        }
        Start-Sleep -Milliseconds 50
    } while([DateTime]::UtcNow -lt $deadline)
    throw "Owned visible enabled menu item not found: $Name"
}
function Read-Outcome {
    foreach($control in $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)) {
        $name=$control.Current.Name
        if($name -eq 'Portable device selection' -or $name.StartsWith('No portable device found.') -or $name.StartsWith('Portable-device discovery failed:')){return $name}
    }
}
function Catalog-Hash { if(Test-Path $snapshot){return (Get-FileHash $snapshot).Hash};return $null }
try {
    $condition=[System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$app.Id),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'WinBlaze'))
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {$window=[System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children,$condition);if($window){break};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $deadline)
    if(!$window){throw 'Owned app window missing'}
    $snapshot=Join-Path $work 'appdata/WinBlaze/index/winblaze.index.bin'
    $fixtureFile=Join-Path $fixture 'example.txt'
    $fixtureHash=(Get-FileHash $fixtureFile).Hash
    $observations=@()
    foreach($iteration in 1..$Repetitions) {
        if($WithCatalog){
            $previousCatalog = Catalog-Hash
            # A new record makes each rescan observable in the persisted catalog.
            Set-Content -LiteralPath (Join-Path $fixture "rescan-$iteration.txt") -Value "rescan $iteration"
            (Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($fixture)
            (Find-Control 'Start scan').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
            $deadline=[DateTime]::UtcNow.AddSeconds(30)
            do {Start-Sleep -Milliseconds 100;if((Find-Control 'Start scan').Current.IsEnabled -and (Test-Path $snapshot) -and (Catalog-Hash) -ne $previousCatalog){break}}while([DateTime]::UtcNow -lt $deadline)
            Start-Sleep -Milliseconds 500
            if(!(Test-Path $snapshot)){throw 'Seed catalog missing'}
            if((Catalog-Hash) -eq $previousCatalog){throw 'Rescan did not publish the changed fixture catalog'}
        }
        (Find-Control 'Explorer').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        if(Read-Outcome){throw 'Stale discovery status invalidates repeat'}
        $before=Catalog-Hash
        if(!$WithCatalog -and $before){throw 'Empty-catalog case unexpectedly has a persisted catalog'}
        $rootBefore=(Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
        $null=[ParityInvocation]::Invoke((Open-OwnedMenuItem 'Scan portable device...'))
        $deadline=[DateTime]::UtcNow.AddSeconds(30)
        do {$outcome=Read-Outcome;if($outcome){break};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $deadline)
        if($outcome -ne 'No portable device found. Connect and unlock a phone in USB file-transfer mode.'){throw "Expected no-device outcome; observed: $outcome"}
        if(!(Find-Control 'Start scan').Current.IsEnabled){throw 'Discovery left scanning disabled'}
        if($before -ne (Catalog-Hash)){throw 'Discovery changed persisted catalog'}
        if($rootBefore -ne (Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value){throw 'Discovery changed root'}
        if($fixtureHash -ne (Get-FileHash $fixtureFile).Hash){throw 'Discovery changed fixture'}
        $observations += [ordered]@{iteration=$iteration;outcome=$outcome;catalog_sha256=$before;scan_enabled=$true;root_preserved=$true;fixture_preserved=$true}
    }
    [ParityInvocation]::CloseOwnedHost($window.Current.NativeWindowHandle,$app.Id)
    if(!$app.WaitForExit(10000) -or $app.ExitCode -ne 0){throw 'No-device workflow failed normal shutdown'}
    [ordered]@{binary_sha256=(Get-FileHash $AppPath).Hash;with_catalog=[bool]$WithCatalog;observations=$observations;normal_exit=0;attached_device_verified=$false}|ConvertTo-Json -Depth 8|Set-Content (Join-Path $work 'portable-no-device-result.json')
    Write-Output "PASS: $Repetitions no-device discoveries; catalog/root/fixture preserved; scanning restored; normal shutdown; evidence=$work"
}catch{
    [ordered]@{error=$_.Exception.Message;outcome=$outcome;observations=$observations;binary_sha256=(Get-FileHash $AppPath).Hash}|ConvertTo-Json -Depth 8|Set-Content (Join-Path $work 'portable-no-device-failure.json')
    throw
}finally{if(!$app.HasExited){$app.Kill();$app.WaitForExit()};$app.Dispose()}
