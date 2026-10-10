#requires -Version 7.0
param([string]$OutputDirectory = 'artifacts/os-xaml-ui', [switch]$SkipNativeBuild)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [IO.Path]::GetFullPath($OutputDirectory, $repo)
$artifacts = [IO.Path]::GetFullPath((Join-Path $repo 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (!$output.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'OS XAML build output must stay inside repository artifacts'
}
$ui = Join-Path $repo 'src/WinBlaze.UI'
$hostSource = Join-Path $ui 'Hosts/OsXaml'
$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio/Installer/vswhere.exe'
if (!(Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio Installer vswhere.exe is required' }
$visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or !$visualStudio) { throw 'Visual Studio C++ build tools are required' }
$vcvars = Join-Path $visualStudio 'VC/Auxiliary/Build/vcvars64.bat'
$kits = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots').KitsRoot10
$sdk = Get-ChildItem -LiteralPath (Join-Path $kits 'Include') -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' -and
        (Test-Path -LiteralPath (Join-Path $_.FullName 'cppwinrt/winrt/base.h')) } |
    Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (!$sdk) { throw 'A Windows SDK with C++/WinRT headers is required' }
$cppwinrt = Join-Path $sdk.FullName 'cppwinrt'
[xml]$project = Get-Content (Join-Path $ui 'WinBlaze.UI.vcxproj') -Raw
$delayNodes = $project.SelectNodes("//*[local-name()='DelayLoadDLLs']")
if ($delayNodes.Count -ne 1) { throw 'Review the UI project first-use import policy before building this host' }
$delayDependencies = @($delayNodes[0].InnerText.Split(';') | Where-Object { $_ -and $_ -ne '%(DelayLoadDLLs)' })
if (!$delayDependencies.Count) { throw 'The UI project first-use import policy is empty' }
foreach ($dependency in $delayDependencies) {
    if ($dependency -notmatch '^[A-Za-z0-9_.-]+\.dll$') { throw "Unexpected first-use import name: $dependency" }
}
$delayOptions = ($delayDependencies | ForEach-Object { '/DELAYLOAD:' + $_ }) -join ' '
$package = $project.SelectSingleNode("//*[local-name()='PackageReference' and @Include='Microsoft.WindowsAppSDK']")
$nugetRoot = if ($env:NUGET_PACKAGES) { $env:NUGET_PACKAGES } else { Join-Path $env:USERPROFILE '.nuget/packages' }
$nuspec = Join-Path $nugetRoot "microsoft.windowsappsdk/$($package.Version)/microsoft.windowsappsdk.nuspec"
if (!(Test-Path -LiteralPath $nuspec)) { throw 'Restore the UI NuGet packages before building the alternative host' }
[xml]$dependencies = Get-Content $nuspec -Raw
$winui = $dependencies.SelectSingleNode("//*[local-name()='dependency' and @id='Microsoft.WindowsAppSDK.WinUI']")
$winuiVersion = $winui.version.Trim('[', ']')
if ($winuiVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Review the Windows App SDK WinUI dependency version' }
$winuiInclude = Join-Path $nugetRoot "microsoft.windowsappsdk.winui/$winuiVersion/include"
if (!(Test-Path -LiteralPath (Join-Path $winuiInclude 'microsoft.ui.xaml.window.h'))) {
    throw 'The restored IWindowNative declaration is missing'
}
Push-Location $repo
try {
    if (!$SkipNativeBuild) {
        & cargo build --release -p winblaze-native -p winblaze-updater
        if ($LASTEXITCODE -ne 0) { throw 'Native engine, CLI or updater build failed' }
    }
    & (Join-Path $PSScriptRoot 'prepare-os-xaml-ui.ps1') -OutputDirectory $output
    $bin = Join-Path $output 'full-os-candidate'
    Get-ChildItem -LiteralPath $hostSource -Filter '*.h' -File | Copy-Item -Destination $output -Force
    Copy-Item -LiteralPath (Join-Path $hostSource 'main.cpp') -Destination (Join-Path $output 'full-os-main.cpp') -Force
    Copy-Item -LiteralPath (Join-Path $hostSource 'app.manifest') -Destination (Join-Path $output 'app.manifest') -Force
    $icon = Join-Path $output 'WinBlaze.ico'
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'encode-window-icons.ps1') -SourcePath (Join-Path $ui 'Assets/WinBlaze.ico') -OutputPath $icon
    if ($LASTEXITCODE -ne 0) { throw 'Window icon encoding failed' }
    foreach ($file in @('winblaze_native.dll', 'winblaze-cli.exe', 'winblaze-updater.exe')) {
        $source = Join-Path $repo "target/release/$file"
        if (!(Test-Path -LiteralPath $source)) { throw "Required native output missing: $file" }
        Copy-Item -LiteralPath $source -Destination $bin -Force
    }
    $resource = '1 ICON "' + $icon.Replace('\', '\\') + '"'
    [IO.File]::WriteAllText((Join-Path $output 'app.rc'), $resource)
    $command = @"
@echo off
setlocal DisableDelayedExpansion
call "$vcvars"
if errorlevel 1 exit /b 1
rc /nologo /fo "$output/app.res" "$output/app.rc"
if errorlevel 1 exit /b 1
cl /nologo /std:c++20 /EHsc /O2 /MT /utf-8 /bigobj /DUNICODE /D_UNICODE /I"$cppwinrt" /I"$ui/include" /I"$ui" /I"$winuiInclude" /Fo"$bin/" /Fe"$bin/WinBlaze.UI.exe" "$output/full-os-main.cpp" "$bin/NativeBridge.cpp" "$bin/MainWindow.cpp" "$output/app.res" /link delayimp.lib $delayOptions windowsapp.lib user32.lib ole32.lib shell32.lib d2d1.lib d3d11.lib dwrite.lib dxgi.lib windowscodecs.lib /SUBSYSTEM:WINDOWS /MANIFEST:EMBED /MANIFESTINPUT:"$output/app.manifest"
"@
    $buildFile = Join-Path $output 'build.cmd'
    [IO.File]::WriteAllText($buildFile, $command)
    & $env:ComSpec /d /c ('call "' + $buildFile + '"')
    if ($LASTEXITCODE -ne 0) { throw 'OS XAML UI compile or link failed' }
    $inputPaths = @(
        Get-ChildItem -LiteralPath (Join-Path $ui 'src') -File | Where-Object Extension -in '.h', '.cpp'
        Get-ChildItem -LiteralPath (Join-Path $ui 'include') -Filter '*.h' -File
        Get-ChildItem -LiteralPath $hostSource -File
        Get-Item (Join-Path $ui 'resource.h'), (Join-Path $ui 'Assets/WinBlaze.ico'), (Join-Path $ui 'WinBlaze.UI.vcxproj')
        Get-Item (Join-Path $PSScriptRoot 'build-os-xaml-ui.ps1'), (Join-Path $PSScriptRoot 'prepare-os-xaml-ui.ps1'), (Join-Path $PSScriptRoot 'encode-window-icons.ps1')
        Get-Item (Join-Path $repo 'target/release/winblaze_native.dll'), (Join-Path $repo 'target/release/winblaze-cli.exe'), (Join-Path $repo 'target/release/winblaze-updater.exe')
    )
    $inputs = @($inputPaths | ForEach-Object {
        [ordered]@{path = [IO.Path]::GetRelativePath($repo, $_.FullName); sha256 = (Get-FileHash -LiteralPath $_.FullName).Hash}
    })
    [ordered]@{
        host = 'OS XAML'; adopted = $false; sdk_version = $sdk.Name; winui_headers_version = $winuiVersion
        delay_load_dependencies = $delayDependencies
        ui_sha256 = (Get-FileHash (Join-Path $bin 'WinBlaze.UI.exe')).Hash
        native_sha256 = (Get-FileHash (Join-Path $bin 'winblaze_native.dll')).Hash
        cli_sha256 = (Get-FileHash (Join-Path $bin 'winblaze-cli.exe')).Hash
        updater_sha256 = (Get-FileHash (Join-Path $bin 'winblaze-updater.exe')).Hash
        inputs = $inputs
    } | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $output 'build-inventory.json')
    Write-Output "Built full OS XAML UI and native companions in $bin"
} finally { Pop-Location }
