#requires -Version 7.0
param([Parameter(Mandatory)][string]$BuildDirectory,
    [string]$OutputDirectory = 'artifacts/os-xaml-focus',
    [string]$HostDirectory = 'src/WinBlaze.UI/Hosts/OsXaml')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$build = [IO.Path]::GetFullPath($BuildDirectory, $repo)
$output = [IO.Path]::GetFullPath($OutputDirectory, $repo)
$artifacts = (Join-Path $repo 'artifacts') + [IO.Path]::DirectorySeparatorChar
if (!$output.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Focus test output must stay inside repository artifacts'
}
$commandPath = Join-Path $build 'build.cmd'
$inventoryPath = Join-Path $build 'build-inventory.json'
if (!(Test-Path $commandPath) -or !(Test-Path $inventoryPath)) { throw 'Build the OS XAML UI first' }
New-Item -ItemType Directory -Path $output -Force | Out-Null
$command = Get-Content $commandPath
$compiler = @($command | Where-Object { $_.StartsWith('cl /nologo ') })
if ($compiler.Count -ne 1) { throw 'Expected one known OS XAML compiler command' }
$bin = Join-Path $build 'full-os-candidate'
$testSource = Join-Path $repo 'tests/native/os-xaml-focus.cpp'
$hostSource = [IO.Path]::GetFullPath($HostDirectory, $repo)
$line = $compiler[0].Replace('/Fo"' + $bin + '/"', '/Fo"' + $output + '/"').Replace(
    '/Fe"' + $bin + '/WinBlaze.UI.exe"', '/Fe"' + $output + '/os-xaml-focus.exe"')
$oldSources = '"' + $build + '/full-os-main.cpp" "' + $bin + '/NativeBridge.cpp" "' + $bin + '/MainWindow.cpp"'
if (!$line.Contains($oldSources)) { throw 'Build command source layout changed; review the focus harness' }
$line = $line.Replace($oldSources, '/I"' + $hostSource + '" "' + $testSource + '"').Replace('/SUBSYSTEM:WINDOWS', '/SUBSYSTEM:CONSOLE')
$vcvars = @($command | Where-Object { $_.StartsWith('call "') })
if ($vcvars.Count -ne 1) { throw 'Expected one Visual Studio setup command' }
$script = "@echo off`r`nsetlocal DisableDelayedExpansion`r`n" + $vcvars[0] + "`r`nif errorlevel 1 exit /b 1`r`n" + $line
$scriptPath = Join-Path $output 'build-focus.cmd'
[IO.File]::WriteAllText($scriptPath, $script)
& $env:ComSpec /d /c ('call "' + $scriptPath + '"')
if ($LASTEXITCODE -ne 0) { throw 'Focus harness build failed' }
$exe = Join-Path $output 'os-xaml-focus.exe'
# This is the visible test UI itself, not a background service or helper.
$process = Start-Process -FilePath $exe -PassThru -RedirectStandardOutput (Join-Path $output 'stdout.log') -RedirectStandardError (Join-Path $output 'stderr.log')
if (!$process.WaitForExit(20000)) {
    Stop-Process -Id $process.Id -Force
    throw 'Owned focus harness timed out'
}
$process.Refresh()
Get-Content (Join-Path $output 'stdout.log'), (Join-Path $output 'stderr.log')
[ordered]@{exit_code = $process.ExitCode; executable_sha256 = (Get-FileHash $exe).Hash
    host_sha256 = (Get-FileHash (Join-Path $hostSource 'OsDesktopWindow.h')).Hash
    scope = 'Thread-local native focus handoff; no global input; not physical keyboard parity'} |
    ConvertTo-Json | Set-Content (Join-Path $output 'result.json')
if ($process.ExitCode -ne 0) { throw 'Owned native-to-XAML focus contract failed' }
