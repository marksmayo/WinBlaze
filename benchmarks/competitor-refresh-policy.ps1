# Reuse only complete bests for the installed identity and unchanged medal definitions.
function Get-CompetitorRefreshTools {
    param([object[]]$Definitions,[object[]]$Entries,[hashtable]$Identities,[switch]$RefreshCompetitors)
    'WinBlaze'
    foreach($tool in @('WinDirStat','WizTree')) {
        $identity=$Identities[$tool]
        $complete=!!$identity.sha256 -and $Definitions.Count -gt 0
        foreach($definition in $Definitions) {
            $matching=@($Entries | Where-Object {
                $entry=$_
                $key=ConvertFrom-Json $entry.key
                $key.Count -eq 9 -and $key[0] -eq $tool -and $key[1] -eq $definition.name -and
                $key[2] -eq $definition.unit -and $key[3] -eq $definition.scope -and
                $key[4] -eq $definition.comparable -and $key[5] -eq $definition.observational -and
                $key[6] -eq $identity.product_version -and $key[7] -eq $identity.file_version -and
                $key[8] -eq $identity.sha256 -and $entry.samples -eq 10 -and $entry.values.Count -eq 10 -and
                $null -ne $entry.value -and ![double]::IsNaN($entry.value) -and ![double]::IsInfinity($entry.value) -and
                @($entry.values | Where-Object {$null -eq $_ -or [double]::IsNaN($_) -or [double]::IsInfinity($_)}).Count -eq 0 -and
                (Test-Path -LiteralPath $entry.source -PathType Leaf)
            })
            if($matching.Count -ne 1){$complete=$false;break}
        }
        if($RefreshCompetitors -or !$complete){$tool}
    }
}

function Get-InstalledBenchmarkRefreshTools {
    param([switch]$RefreshCompetitors)
    . (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
    $identities=@{}
    foreach($tool in @('WinDirStat','WizTree')) {
        $exe=if($tool -eq 'WinDirStat'){'C:\Program Files\WinDirStat\WinDirStat.exe'}else{'C:\Program Files\WizTree\WizTree64.exe'}
        $identities[$tool]=Get-BenchmarkIdentity -Tool $tool -Executable $exe
    }
    $definitions=(Read-BenchmarkJson (Join-Path $PSScriptRoot 'original-medal-events.json')).events
    $storePath=Join-Path $PSScriptRoot 'competitor-best-results.json'
    $entries=if(Test-Path -LiteralPath $storePath){(Read-BenchmarkJson $storePath).entries}else{@()}
    Get-CompetitorRefreshTools -Definitions $definitions -Entries $entries -Identities $identities -RefreshCompetitors:$RefreshCompetitors
}
