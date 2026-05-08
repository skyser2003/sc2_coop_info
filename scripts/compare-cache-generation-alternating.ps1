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

$scriptPath = Join-Path $PSScriptRoot "compare-cache-generation-vs-head.ps1"
$arguments = @(
    "-ComparisonRef",
    $ComparisonRef,
    "-Runs",
    "10",
    "-Workers",
    $Workers.ToString()
)

if ($null -ne $RecentReplayCount) {
    $arguments += @("-RecentReplayCount", $RecentReplayCount.Value.ToString())
}

if (-not $NoAnalyzerTimings.IsPresent) {
    $arguments += "-AnalyzerTimings"
}

if ($KeepArtifacts.IsPresent) {
    $arguments += "-KeepArtifacts"
}

& $scriptPath @arguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
