$ErrorActionPreference = 'Stop'
$publishTokens = $null
$publishErrors = $null
$source = [System.Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $PSScriptRoot 'run-comparison.ps1'), [ref]$publishTokens, [ref]$publishErrors)
if ($publishErrors.Count) { throw 'Comparison runner must parse successfully.' }
$publisher = $source.Find({ param($item)
    $item -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $item.Name -eq 'Publish-Report'
}, $true)
Invoke-Expression $publisher.Extent.Text

# Exercise publication from a nested product runner without launching products
# or writing report files. PowerShell resolves parent functions dynamically.
$manifest = @{ batch_id = 'publication-fixture' }
$batch = 'publication-fixture'
$results = 'publication-fixture/workflow-results.json'
$workflowResultsPath = $results
$script:publishedSource = $null
function Test-Path { param($LiteralPath) return $true }
function Join-Path { param($Path, $ChildPath) return "$Path/$ChildPath" }
function Set-Content { param($LiteralPath, $Encoding) process {} }
function Copy-Item { param($LiteralPath, $Destination) $script:publishedSource = $LiteralPath }
function node { $global:LASTEXITCODE = 0 }
& {
    param($callback)
    $results = [Collections.Generic.List[object]]::new()
    $results.Add([pscustomobject]@{ tool = 'WinBlaze'; run = 10 })
    & $callback
} { Publish-Report }
if ($script:publishedSource -ne 'publication-fixture/workflow-results.json') {
    throw 'Completed-product publication must copy the workflow evidence file, not the nested runner collection.'
}
Write-Output 'PASS: completed-product callback publishes the intended workflow file from a nested runner.'
