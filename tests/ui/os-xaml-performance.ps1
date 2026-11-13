#requires -Version 7.0
# Builds tests/native/os-xaml-performance.cpp against an existing OS XAML build
# and checks the performance contracts of the Release host.
param(
    [Parameter(Mandatory)][string]$BuildDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$build = [IO.Path]::GetFullPath($BuildDirectory, $repo)
$output = [IO.Path]::GetFullPath($OutputDirectory, $repo)
$artifacts = (Join-Path $repo 'artifacts') + [IO.Path]::DirectorySeparatorChar
if (!$build.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase) -or !$output.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase)) { throw 'Test build/output must remain in repository artifacts' }
New-Item -ItemType Directory -Path $output -Force | Out-Null

# Contracts with no observable counter: checked in the shipping sources.
$failures = @()
$hostMain = Get-Content -LiteralPath (Join-Path $repo 'src/WinBlaze.UI/Hosts/OsXaml/main.cpp') -Raw
$window = Get-Content -LiteralPath (Join-Path $repo 'src/WinBlaze.UI/src/MainWindow.cpp') -Raw
if ($hostMain -match 'ShowWindow\(owner,\s*SW_MAXIMIZE\)' -or $hostMain -notmatch 'Activate\(SW_SHOWMAXIMIZED\)') {
    $failures += 'The host must show its window maximized in one step, not show it and then maximize it'
}
if ($window -match 'Present\(1,\s*0\)' -or $window -notmatch 'Present\(0,\s*0\)') {
    $failures += 'Treemap frames must be presented without blocking the UI thread on vertical sync'
}
if ($window -notmatch '->ResizeBuffers\(') {
    $failures += 'A treemap resize must resize the swap chain buffers'
}

$existing = Get-Content -LiteralPath (Join-Path $build 'build.cmd') -Raw
$compile = [regex]::Match($existing, '(?m)^(cl .+?) /Fo').Groups[1].Value
$setup = [regex]::Match($existing, '(?m)^call "[^"]+vcvars64.bat"').Value
$link = [regex]::Match($existing, '(?m) /link (.+)$').Groups[1].Value.Trim().Replace('/SUBSYSTEM:WINDOWS', '/SUBSYSTEM:CONSOLE')
if (!$compile -or !$setup -or !$link) { throw 'Review changed OS XAML compiler command' }
$candidate = Join-Path $build 'full-os-candidate'
$executable = Join-Path $candidate 'os-xaml-performance.exe'
$source = Join-Path $repo 'tests/native/os-xaml-performance.cpp'
$command = "$setup`r`nif errorlevel 1 exit /b 1`r`n$compile /I`"$build`" /Fo`"$output/`" /Fe`"$executable`" `"$source`" `"$candidate/MainWindow.obj`" `"$candidate/NativeBridge.obj`" `"$build/app.res`" /link $link`r`n"
$script = Join-Path $output 'build.cmd'
[IO.File]::WriteAllText($script, $command)
& $env:ComSpec /d /c ('call "' + $script + '"')
if ($LASTEXITCODE) { throw 'Performance contract test failed to build' }

# An empty profile: no previous catalog to restore, nothing of the user's is read.
$profile = Join-Path $output ('appdata-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $profile -Force | Out-Null
$fixture = Join-Path $output 'saved scan'
New-Item -ItemType Directory -Path (Join-Path $fixture 'tree/a') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'tree/a/one.txt') -Value 'one'
Set-Content -LiteralPath (Join-Path $fixture 'tree/two.txt') -Value 'two'
$csv = Join-Path $fixture 'scan.csv'
& (Join-Path $candidate 'winblaze-cli.exe') (Join-Path $fixture 'tree') $csv | Out-Null
if ($LASTEXITCODE -or !(Test-Path -LiteralPath $csv)) { throw 'Could not export the saved-scan fixture' }

$savedProfile = $env:LOCALAPPDATA
try {
    $env:LOCALAPPDATA = $profile
    foreach ($case in @(@{name='idle'; arguments=@()}, @{name='saved-scan'; arguments=@('"' + $csv + '"')},
            @{name='folder-scan'; arguments=@('"' + (Join-Path $fixture 'tree') + '"')})) {
        $out = Join-Path $output "$($case.name).stdout.log"
        $err = Join-Path $output "$($case.name).stderr.log"
        $start = @{FilePath=$executable; WorkingDirectory=$candidate; WindowStyle='Hidden'; PassThru=$true; Wait=$true
            RedirectStandardOutput=$out; RedirectStandardError=$err}
        if ($case.arguments.Count) { $start.ArgumentList = $case.arguments }
        $process = Start-Process @start
        Get-Content -LiteralPath $out
        Get-Content -LiteralPath $err
        if ($process.ExitCode) { $failures += "Runtime contract '$($case.name)' failed with exit $($process.ExitCode)" }
    }
} finally { $env:LOCALAPPDATA = $savedProfile }
if ($failures.Count) {
    $failures | ForEach-Object { Write-Output "FAIL: $_" }
    throw "OS XAML performance contracts failed: $($failures.Count)"
}
Write-Output 'PASS: OS XAML performance contracts'
