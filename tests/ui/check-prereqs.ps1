param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe")
)

$ErrorActionPreference = "Stop"

$resolvedAppPath = $null
if (Test-Path -LiteralPath $AppPath -PathType Leaf) {
    $resolvedAppPath = (Resolve-Path -LiteralPath $AppPath).Path
}

$uiaClientLoaded = $false
$uiaTypesLoaded = $false
$uiaError = $null
try {
    Add-Type -AssemblyName UIAutomationClient
    $uiaClientLoaded = $true
    Add-Type -AssemblyName UIAutomationTypes
    $uiaTypesLoaded = $true
} catch {
    $uiaError = $_.Exception.Message
}

$desktopAvailable = $false
$desktopError = $null
if ($uiaClientLoaded -and $uiaTypesLoaded) {
    try {
        $desktopAvailable = $null -ne [System.Windows.Automation.AutomationElement]::RootElement
    } catch {
        $desktopError = $_.Exception.Message
    }
}

$ready = [System.Environment]::UserInteractive -and
    $uiaClientLoaded -and
    $uiaTypesLoaded -and
    $desktopAvailable -and
    (-not [string]::IsNullOrWhiteSpace($resolvedAppPath))

[pscustomobject]@{
    app_path = $resolvedAppPath
    app_found = -not [string]::IsNullOrWhiteSpace($resolvedAppPath)
    user_interactive = [System.Environment]::UserInteractive
    uia_client_loaded = $uiaClientLoaded
    uia_types_loaded = $uiaTypesLoaded
    uia_error = $uiaError
    desktop_root_available = $desktopAvailable
    desktop_error = $desktopError
    ui_smoke_ready = $ready
} | ConvertTo-Json
