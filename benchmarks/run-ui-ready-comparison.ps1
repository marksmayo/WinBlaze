param([ValidateRange(1,100)][int]$Runs=10,[string]$OutputPath=(Join-Path $PSScriptRoot 'current-ui-ready-results.json'),[string]$AppPath=(Join-Path $PSScriptRoot '../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'),[string]$HistoryPath='')
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ReadyWindow {
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
[DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr window);
}
'@
$identityOptions=@{Tool='WinBlaze';Executable=([IO.Path]::GetFullPath($AppPath))}
if($HistoryPath){$identityOptions.HistoryPath=$HistoryPath}
$identity=Get-BenchmarkIdentity @identityOptions
$rows=[Collections.Generic.List[object]]::new()
for($run=1;$run -le $Runs;$run++){
 $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=[IO.Path]::GetFullPath($AppPath);$start.UseShellExecute=$false
 $start.EnvironmentVariables['LOCALAPPDATA']=Join-Path $PSScriptRoot ('../artifacts/ui-ready/'+[guid]::NewGuid().ToString('N'))
 $timer=[Diagnostics.Stopwatch]::StartNew();$app=[Diagnostics.Process]::Start($start)
 try {
  $ready=$false
  while($timer.ElapsedMilliseconds -lt 15000){
   $app.Refresh();if($app.HasExited){throw 'UI exited before ready'}
   $hwnd=$app.MainWindowHandle
   if($hwnd -ne [IntPtr]::Zero -and [ReadyWindow]::IsWindowVisible($hwnd) -and [ReadyWindow]::IsZoomed($hwnd)){
    $window=[System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $condition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Scan root path')
    $path=$window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
    $condition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Start scan')
    $scan=$window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
    if($path -and $scan -and $path.Current.IsEnabled -and !$path.Current.IsOffscreen -and $scan.Current.IsEnabled){$ready=$true;break}
   }
   Start-Sleep -Milliseconds 10
  }
  if(!$ready){throw 'Visible maximized window and enabled scan controls did not become ready'}
  $elapsed=$timer.Elapsed.TotalMilliseconds;$app.Refresh()
  $rows.Add([pscustomobject]@{tool='WinBlaze';run=$run;ready_ms=[math]::Round($elapsed,2);binary=$identity;utc=[DateTime]::UtcNow.ToString('o');scope='Fresh empty appdata; launch to visible maximized window with accessible enabled path and scan controls; 10ms polling; excludes restored catalog';working_set_bytes=$app.WorkingSet64;cpu_ms=$app.TotalProcessorTime.TotalMilliseconds})
  Write-BenchmarkJson -Path ([IO.Path]::GetFullPath($OutputPath)) -Value @($rows.ToArray())
  Write-Output "Visible ready $run/$Runs : $([math]::Round($elapsed)) ms"
 } finally {if(!$app.HasExited){Stop-Process -Id $app.Id}}
}
