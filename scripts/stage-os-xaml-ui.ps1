#requires -Version 7.0
param([Parameter(Mandatory)][string]$BuildDirectory,
    [string]$OutputDirectory = 'src/WinBlaze.UI/bin/x64/Release')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$build = [IO.Path]::GetFullPath($BuildDirectory, $repo)
$output = [IO.Path]::GetFullPath($OutputDirectory, $repo)
$allowed = @((Join-Path $repo 'artifacts'), (Join-Path $repo 'src/WinBlaze.UI/bin'))
if (!@($allowed | Where-Object { $output.StartsWith($_ + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) }).Count) {
    throw 'UI staging output must stay inside repository artifacts or UI build outputs'
}
$inventoryPath = Join-Path (Split-Path $build -Parent) 'build-inventory.json'
$inventory = Get-Content -LiteralPath $inventoryPath -Raw | ConvertFrom-Json
if ($inventory.host -ne 'OS XAML') { throw 'Expected an OS XAML build inventory' }
foreach ($buildInput in $inventory.inputs) {
    if ((Get-FileHash -LiteralPath (Join-Path $repo $buildInput.path)).Hash -ne $buildInput.sha256) {
        throw "Refusing stale UI build input: $($buildInput.path)"
    }
}
$binaries = @{ui='WinBlaze.UI.exe';native='winblaze_native.dll';cli='winblaze-cli.exe';updater='winblaze-updater.exe'}
foreach ($component in $binaries.Keys) {
    if ((Get-FileHash -LiteralPath (Join-Path $build $binaries[$component])).Hash -ne $inventory.($component + '_sha256')) {
        throw "Build inventory does not match $component binary"
    }
}
$backup = Join-Path $repo ('artifacts/ui-stage-backup-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $backup, $output -Force | Out-Null
$previous = @()
foreach ($file in $binaries.Values) {
    $destination = Join-Path $output $file
    if (Test-Path -LiteralPath $destination) {
        Copy-Item -LiteralPath $destination -Destination $backup
        $previous += [ordered]@{file=$file;sha256=(Get-FileHash -LiteralPath $destination).Hash}
    }
}
$previous | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $backup 'previous-binaries.json')
foreach ($file in $binaries.Values) {
    Copy-Item -LiteralPath (Join-Path $build $file) -Destination $output -Force
}
$inventory.adopted = $true
$inventory | Add-Member output_directory $output
$inventory | Add-Member previous_binaries_directory $backup
$inventory | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $output 'winblaze-host.json')
Write-Output "Staged current full OS XAML Release binaries in $output; previous binaries retained in $backup"
