param(
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [string]$OutputPath = (Join-Path $PSScriptRoot "environment.json")
)

$ErrorActionPreference = "Stop"

function Get-DriveRoot {
    param([string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    $root = [System.IO.Path]::GetPathRoot($full)
    if ([string]::IsNullOrWhiteSpace($root)) {
        return $null
    }
    return $root
}

function Get-CimOrNull {
    param(
        [string]$ClassName,
        [string]$Filter = ""
    )

    try {
        if ([string]::IsNullOrWhiteSpace($Filter)) {
            return Get-CimInstance $ClassName -ErrorAction Stop
        }
        return Get-CimInstance $ClassName -Filter $Filter -ErrorAction Stop
    } catch {
        return $null
    }
}

$computer = Get-CimOrNull -ClassName "Win32_ComputerSystem"
$os = Get-CimOrNull -ClassName "Win32_OperatingSystem"
$cpu = Get-CimOrNull -ClassName "Win32_Processor" | Select-Object -First 1
$driveRoot = Get-DriveRoot -Path $DatasetRoot
$volume = $null
if (-not [string]::IsNullOrWhiteSpace($driveRoot)) {
    $driveLetter = $driveRoot.TrimEnd("\")
    $volume = Get-CimOrNull -ClassName "Win32_LogicalDisk" -Filter "DeviceID='$driveLetter'"
}

$environment = [pscustomobject]@{
    captured_utc = (Get-Date).ToUniversalTime().ToString("o")
    machine = [pscustomobject]@{
        name = $env:COMPUTERNAME
        manufacturer = if ($computer) { $computer.Manufacturer } else { $null }
        model = if ($computer) { $computer.Model } else { $null }
    }
    os = [pscustomobject]@{
        caption = if ($os) { $os.Caption } else { [System.Environment]::OSVersion.Platform.ToString() }
        version = if ($os) { $os.Version } else { [System.Environment]::OSVersion.Version.ToString() }
        build_number = if ($os) { $os.BuildNumber } else { [System.Environment]::OSVersion.Version.Build }
    }
    cpu = [pscustomobject]@{
        name = if ($cpu) { $cpu.Name } else { $env:PROCESSOR_IDENTIFIER }
        cores = if ($cpu) { $cpu.NumberOfCores } else { $null }
        logical_processors = if ($cpu) { $cpu.NumberOfLogicalProcessors } else { [System.Environment]::ProcessorCount }
    }
    memory = [pscustomobject]@{
        total_physical_bytes = if ($computer) { [int64]$computer.TotalPhysicalMemory } else { $null }
    }
    dataset_storage = [pscustomobject]@{
        requested_root = $DatasetRoot
        drive_root = $driveRoot
        filesystem = if ($volume) { $volume.FileSystem } else { $null }
        size_bytes = if ($volume) { [int64]$volume.Size } else { $null }
        free_bytes = if ($volume) { [int64]$volume.FreeSpace } else { $null }
    }
    power = [pscustomobject]@{
        active_scheme = (powercfg /getactivescheme 2>$null)
    }
}

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}

$environment | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
$environment | ConvertTo-Json -Depth 5
