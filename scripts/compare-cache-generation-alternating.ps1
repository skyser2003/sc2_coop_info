[CmdletBinding()]
param(
    [Alias("HeadRef")]
    [string]$ComparisonRef = "HEAD",
    [Nullable[int]]$RecentReplayCount = $null,
    [int]$Workers = 8,
    [switch]$NoAnalyzerTimings,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$scriptPath = Join-Path $PSScriptRoot "compare-cache-generation.ps1"
$arguments = @{
    ComparisonRef = $ComparisonRef
    Runs = 10
    Workers = $Workers
    WarmupRunsPerVariant = 1
}

if ($null -ne $RecentReplayCount) {
    $arguments.RecentReplayCount = $RecentReplayCount
}

if (-not $NoAnalyzerTimings.IsPresent) {
    $arguments.AnalyzerTimings = $true
}

if ($KeepArtifacts.IsPresent) {
    $arguments.KeepArtifacts = $true
}

& $scriptPath @arguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
