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
$existing = Get-Content -LiteralPath (Join-Path $build 'build.cmd') -Raw
$compile = [regex]::Match($existing, '(?m)^(cl .+?) /Fo').Groups[1].Value
$setup = [regex]::Match($existing, '(?m)^call "[^"]+vcvars64.bat"').Value
$link = [regex]::Match($existing, '(?m) /link (.+)$').Groups[1].Value.Trim().Replace('/SUBSYSTEM:WINDOWS', '/SUBSYSTEM:CONSOLE')
if (!$compile -or !$setup -or !$link) { throw 'Review changed OS XAML compiler command' }
$candidate = Join-Path $build 'full-os-candidate'
$executable = Join-Path $candidate 'os-xaml-lazy-panels.exe'
$source = Join-Path $repo 'tests/native/os-xaml-lazy-panels.cpp'
$command = "$setup`r`nif errorlevel 1 exit /b 1`r`n$compile /I`"$build`" /Fo`"$output/`" /Fe`"$executable`" `"$source`" `"$candidate/MainWindow.obj`" `"$candidate/NativeBridge.obj`" `"$build/app.res`" /link $link`r`n"
$script = Join-Path $output 'build.cmd'
[IO.File]::WriteAllText($script, $command)
& $env:ComSpec /d /c ('call "' + $script + '"')
if ($LASTEXITCODE) { throw 'Lazy panel test failed to build' }
$process = Start-Process -FilePath $executable -WorkingDirectory $candidate -WindowStyle Hidden -PassThru -Wait -RedirectStandardOutput (Join-Path $output 'stdout.log') -RedirectStandardError (Join-Path $output 'stderr.log')
Get-Content -LiteralPath (Join-Path $output 'stdout.log')
Get-Content -LiteralPath (Join-Path $output 'stderr.log')
if ($process.ExitCode) { throw "Lazy panel contract failed with exit $($process.ExitCode)" }
